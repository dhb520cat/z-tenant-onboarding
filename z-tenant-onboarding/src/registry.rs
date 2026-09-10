//! Vendor registry — the configuration layer that keeps this contract generic.
//!
//! One JSON document per vendor in the tenant KV map
//! `z:<tid>:onboarding-providers`, keyed by a short provider id.
//!
//! ```json
//! {
//!   "name": "Acme Benefits",
//!   "url": "https://api.acme-benefits.example/v1/enrollments",
//!   "method": "POST",
//!   "secret_key": "acme_benefits_api_key",
//!   "auth_header": "Authorization",
//!   "auth_format": "Bearer {key}",
//!   "headers": [["Accept", "application/json"]],
//!   "body": {
//!     "employee": {
//!       "given_name":  "{{profile.first_name}}",
//!       "family_name": "{{profile.last_name}}",
//!       "born_on":     "{{profile.date_of_birth}}",
//!       "email":       "{{profile.verified_contacts.email.value}}"
//!     },
//!     "plan_id":    "{{field.plan_id}}",
//!     "start_date": "{{field.start_date}}"
//!   }
//! }
//! ```
//!
//! Two marker namespaces, deliberately separated:
//!
//! * `{{profile.*}}` — the employee's PII. Left in the body verbatim; the host
//!   substitutes the real value inside the enclave. This contract never sees it.
//! * `{{field.*}}`   — ordinary, non-personal arguments (plan id, start date,
//!   cost centre). Substituted here, from the caller's `fields` object.
//!
//! Keeping them apart is what makes it safe for the caller to pass values at
//! all: `{{field.*}}` is the only channel a caller can write into, and
//! `enroll::assert_no_pii` rejects anything that looks personal.

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Map holding one document per vendor.
pub const PROVIDERS_MAP: &str = "onboarding-providers";
/// Map holding vendor API keys, referenced by `secret_key`.
pub const SECRETS_MAP: &str = "secrets";
/// Upper bound for a single registry scan.
pub const SCAN_LIMIT: u32 = 256;

#[derive(serde::Deserialize, Debug, Clone)]
pub struct ProviderConfig {
    /// Human-readable vendor name, for logs and agent-facing listings.
    pub name: String,
    /// Absolute URL of the enrolment endpoint.
    pub url: String,
    /// HTTP verb. Accepts any case; validated in `enroll::verb_of`.
    #[serde(default = "default_method")]
    pub method: String,
    /// Key in the `secrets` map holding this vendor's API key. Omit for an
    /// endpoint that needs no credential.
    #[serde(default)]
    pub secret_key: Option<String>,
    /// Header the credential goes into. Defaults to `Authorization`.
    #[serde(default)]
    pub auth_header: Option<String>,
    /// Credential format; `{key}` is replaced with the secret's value.
    /// Defaults to `Bearer {key}`.
    #[serde(default)]
    pub auth_format: Option<String>,
    /// Extra static headers.
    ///
    /// Do **not** set `Content-Type` here — the host sets it, and sending it
    /// explicitly produces a duplicate that some upstreams reject. (Documented
    /// only inside the reference repo; see findings/BUGS.md BUG-08.)
    #[serde(default)]
    pub headers: Option<Vec<(String, String)>>,
    /// Request body template, carrying `{{profile.*}}` and `{{field.*}}`.
    pub body: serde_json::Value,
    /// Return the vendor's raw response to the caller. Bring-up only —
    /// see `EnrollResult::echo`. Defaults to off.
    #[serde(default)]
    pub echo_response: bool,
}

fn default_method() -> String {
    "POST".to_string()
}

/// What `list-providers` returns: enough for an agent to choose a vendor and
/// know what to supply, and nothing that could leak a credential.
#[derive(serde::Serialize, Debug)]
pub struct ProviderSummary {
    pub id: String,
    pub name: String,
    pub url: String,
    /// `{{field.*}}` names this vendor's template expects from the caller.
    pub required_fields: Vec<String>,
    /// `{{profile.*}}` markers the host will resolve. Surfaced so an operator
    /// can see which PII each vendor receives — the audit question this whole
    /// design exists to answer.
    pub profile_fields: Vec<String>,
}

#[derive(serde::Serialize, Debug)]
pub struct ProviderList {
    pub providers: Vec<ProviderSummary>,
}

pub fn parse_provider(bytes: &[u8]) -> Result<ProviderConfig, String> {
    serde_json::from_slice(bytes).map_err(|e| format!("provider config is not valid JSON: {e}"))
}

/// Collect every `{{<ns>.<path>}}` marker in a template, in document order,
/// deduplicated.
pub fn collect_markers(value: &serde_json::Value, ns: &str) -> Vec<String> {
    let mut out = Vec::new();
    walk_markers(value, ns, &mut out);
    out
}

fn walk_markers(value: &serde_json::Value, ns: &str, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => {
            let prefix = format!("{{{{{ns}.");
            let mut rest = s.as_str();
            while let Some(i) = rest.find(&prefix) {
                let after = &rest[i + prefix.len()..];
                match after.find("}}") {
                    Some(j) => {
                        let name = &after[..j];
                        if !name.is_empty() && !out.iter().any(|e| e == name) {
                            out.push(name.to_string());
                        }
                        rest = &after[j + 2..];
                    }
                    None => break,
                }
            }
        }
        serde_json::Value::Array(a) => a.iter().for_each(|v| walk_markers(v, ns, out)),
        serde_json::Value::Object(o) => o.values().for_each(|v| walk_markers(v, ns, out)),
        _ => {}
    }
}

pub fn summarize(id: &str, cfg: &ProviderConfig) -> ProviderSummary {
    ProviderSummary {
        id: id.to_string(),
        name: cfg.name.clone(),
        url: cfg.url.clone(),
        required_fields: collect_markers(&cfg.body, "field"),
        profile_fields: collect_markers(&cfg.body, "profile"),
    }
}

// ── host-side ───────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
use crate::host::{
    interfaces::{kv_store, logging},
    tenant::tenant_context,
};

/// Build a fully-qualified tenant map name.
///
/// `tenant_did()` returns the raw 20-byte CompactDid; it must be hex-encoded
/// exactly once. A missing or doubled hex-encode both yield a path that
/// silently matches nothing.
#[cfg(target_arch = "wasm32")]
pub fn map_name(tail: &str) -> String {
    let tid = tenant_context::tenant_did();
    format!("z:{}:{}", hex::encode(&tid), tail)
}

#[cfg(target_arch = "wasm32")]
pub fn load_provider(id: &str) -> Result<ProviderConfig, String> {
    let map = map_name(PROVIDERS_MAP);
    let bytes = kv_store::get(&map, id.as_bytes())
        .map_err(|e| format!("kv read {map}: {e}"))?
        .ok_or_else(|| {
            format!("unknown provider '{id}' — no document at {map}. Register it with a map write; see README.md")
        })?;
    parse_provider(&bytes)
}

#[cfg(target_arch = "wasm32")]
pub fn load_secret(key: &str) -> Result<String, String> {
    let map = map_name(SECRETS_MAP);
    let bytes = kv_store::get(&map, key.as_bytes())
        .map_err(|e| format!("kv read {map}: {e}"))?
        .ok_or_else(|| format!("secret '{key}' not found in {map}"))?;
    String::from_utf8(bytes).map_err(|_| format!("secret '{key}' is not valid UTF-8"))
}

#[cfg(target_arch = "wasm32")]
pub fn list_providers() -> Result<Vec<u8>, String> {
    let map = map_name(PROVIDERS_MAP);
    // Half-open [start, end) over the whole key space.
    let rows = kv_store::scan(&map, b"", &[0xffu8; 32], SCAN_LIMIT)
        .map_err(|e| format!("kv scan {map}: {e}"))?;

    let mut providers = Vec::new();
    for (k, v) in rows {
        let id = String::from_utf8_lossy(&k).to_string();
        match parse_provider(&v) {
            Ok(cfg) => providers.push(summarize(&id, &cfg)),
            // One malformed document must not hide every healthy vendor.
            Err(e) => {
                let _ = logging::error(&format!("skipping provider '{id}': {e}"));
            }
        }
    }
    let _ = logging::info(&format!("list-providers: {} vendor(s)", providers.len()));
    serde_json::to_vec(&ProviderList { providers }).map_err(|e| e.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn list_providers() -> Result<Vec<u8>, String> {
    Err("list_providers requires the wasm32 target and a live tenant context".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> serde_json::Value {
        serde_json::json!({
            "employee": {
                "given_name":  "{{profile.first_name}}",
                "family_name": "{{profile.last_name}}",
                "email":       "{{profile.verified_contacts.email.value}}"
            },
            "plan_id":    "{{field.plan_id}}",
            "start_date": "{{field.start_date}}",
            "tags": ["{{field.plan_id}}", "static-value"]
        })
    }

    #[test]
    fn collects_profile_markers() {
        let m = collect_markers(&sample(), "profile");
        assert_eq!(m.len(), 3);
        assert!(m.contains(&"first_name".to_string()));
        assert!(m.contains(&"verified_contacts.email.value".to_string()));
    }

    #[test]
    fn collects_field_markers_deduplicated() {
        let m = collect_markers(&sample(), "field");
        // plan_id appears twice in the template, once in the result.
        assert_eq!(m.len(), 2, "expected plan_id + start_date, got {m:?}");
        assert!(m.contains(&"plan_id".to_string()));
        assert!(m.contains(&"start_date".to_string()));
    }

    #[test]
    fn namespaces_do_not_bleed_into_each_other() {
        let profile = collect_markers(&sample(), "profile");
        assert!(!profile.iter().any(|m| m.starts_with("plan")));
    }

    #[test]
    fn parses_a_minimal_provider_and_applies_defaults() {
        let cfg = parse_provider(
            br#"{"name":"Acme","url":"https://acme.example/e","body":{"a":1}}"#,
        )
        .expect("should parse");
        assert_eq!(cfg.method, "POST");
        assert!(cfg.secret_key.is_none());
        assert!(cfg.headers.is_none());
    }

    #[test]
    fn rejects_non_json_config_with_a_useful_message() {
        let e = parse_provider(b"not json").unwrap_err();
        assert!(e.contains("not valid JSON"), "got: {e}");
    }

    #[test]
    fn summary_carries_no_secret_material() {
        let cfg = parse_provider(
            br#"{"name":"Acme","url":"https://acme.example/e",
                 "secret_key":"acme_api_key","body":{"x":"{{field.y}}"}}"#,
        )
        .unwrap();
        let s = summarize("acme", &cfg);
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("acme_api_key"), "secret key name leaked: {json}");
    }
}
