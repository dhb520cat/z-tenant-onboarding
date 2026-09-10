//! Enrolment — build one vendor request and dispatch it.
//!
//! The privacy property this module has to preserve:
//!
//! > A `{{profile.*}}` marker must reach the host **unmodified**, and no
//! > personal data may enter the request through any other route.
//!
//! The first half is easy: only `{{field.*}}` is rendered here, and
//! `{{profile.*}}` is left alone. The second half is the interesting one.
//!
//! `fields` is the only channel a caller can write into. If a caller could put
//! `"given_name": "Jane Smith"` there, they would have routed PII around the
//! enclave entirely — the plaintext would sit in this contract's memory, in the
//! agent's context, and in every log between them, which is exactly what the
//! placeholder mechanism exists to prevent. It would still *work*, which is
//! what makes it dangerous: nothing fails, and the guarantee is quietly gone.
//!
//! So `assert_no_pii` rejects caller values that look personal, by field name
//! and by value shape, and the error tells the caller to use a `{{profile.*}}`
//! marker instead.

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

#[derive(serde::Deserialize, Debug)]
pub struct EnrollReq {
    pub provider_id: String,
    /// Non-personal values the vendor template marks as `{{field.*}}`.
    #[serde(default)]
    pub fields: serde_json::Map<String, serde_json::Value>,
}

#[derive(serde::Serialize, Debug)]
pub struct EnrollResult {
    pub provider_id: String,
    pub status: String,
    pub http_code: u16,
    /// Vendor-assigned identifier, when the response carries a recognisable one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// Raw vendor response, returned only when the provider config sets
    /// `"echo_response": true`.
    ///
    /// This exists for bring-up against an echo endpoint, where reading back
    /// what the vendor actually received is the only way to confirm the host
    /// substituted the `{{profile.*}}` markers. Leave it off for real vendors:
    /// a genuine response may contain personal data, and enabling it would put
    /// that data into the agent's context — the exact thing this contract is
    /// built to avoid.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub echo: Option<serde_json::Value>,
}

/// Field names that must never be supplied by the caller. Anything personal
/// belongs in a `{{profile.*}}` marker so the host resolves it in the enclave.
const PII_NAME_FRAGMENTS: &[&str] = &[
    "first_name", "last_name", "given_name", "family_name", "middle_name",
    "full_name", "surname", "forename", "birth", "dob", "age", "gender",
    "email", "phone", "mobile", "telephone", "address", "street", "postcode",
    "zip", "passport", "ssn", "social_security", "national_id", "tax_id",
    "nino", "iban", "bank_account", "account_number", "sort_code", "card",
];

/// Reject caller-supplied values that look like personal data.
///
/// Two independent checks — a caller who renames a field to dodge the name
/// check still trips the value check, and vice versa.
pub fn assert_no_pii(
    fields: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    for (name, value) in fields {
        let lower = name.to_ascii_lowercase();
        if let Some(frag) = PII_NAME_FRAGMENTS.iter().find(|f| lower.contains(**f)) {
            return Err(format!(
                "field '{name}' looks like personal data (matched '{frag}'). \
                 Personal data must not be passed as an argument — put a \
                 {{{{profile.<field>}}}} marker in the provider template so the \
                 host resolves it inside the enclave."
            ));
        }
        if let serde_json::Value::String(s) = value {
            if let Some(why) = value_looks_personal(s) {
                return Err(format!(
                    "value of field '{name}' looks like personal data ({why}). \
                     Use a {{{{profile.<field>}}}} marker instead."
                ));
            }
        }
    }
    Ok(())
}

/// Cheap shape checks. Deliberately conservative — these run on non-personal
/// operational values (plan ids, cost centres) and must not reject them.
fn value_looks_personal(s: &str) -> Option<&'static str> {
    let t = s.trim();
    // name@host.tld
    if let Some(at) = t.find('@') {
        let (user, host) = (&t[..at], &t[at + 1..]);
        if !user.is_empty() && host.contains('.') && !host.starts_with('.') && !host.ends_with('.') {
            return Some("an email address");
        }
    }
    // Long run of digits/separators — card, account or national id.
    let digits = t.chars().filter(char::is_ascii_digit).count();
    if digits >= 9 && t.chars().all(|c| c.is_ascii_digit() || " -()+".contains(c)) {
        return Some("a long numeric identifier such as a phone or account number");
    }
    // YYYY-MM-DD
    let b = t.as_bytes();
    if b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return Some("a date of birth");
    }
    None
}

/// Substitute `{{field.*}}` markers. `{{profile.*}}` is left untouched — the
/// host resolves it after this contract hands the body over.
///
/// A whole-string marker (`"{{field.x}}"`) is replaced by the value with its
/// JSON type intact, so a number stays a number. An embedded marker
/// (`"plan-{{field.x}}"`) is interpolated as text.
pub fn render_fields(
    template: &serde_json::Value,
    fields: &serde_json::Map<String, serde_json::Value>,
) -> Result<serde_json::Value, String> {
    match template {
        serde_json::Value::String(s) => {
            if let Some(name) = whole_marker(s, "field") {
                return fields
                    .get(name)
                    .cloned()
                    .ok_or_else(|| missing_field_msg(name, fields));
            }
            let mut out = String::new();
            let mut rest = s.as_str();
            while let Some(i) = rest.find("{{field.") {
                out.push_str(&rest[..i]);
                let after = &rest[i + "{{field.".len()..];
                let j = after
                    .find("}}")
                    .ok_or_else(|| format!("unterminated marker in template: {s}"))?;
                let name = &after[..j];
                let v = fields
                    .get(name)
                    .ok_or_else(|| missing_field_msg(name, fields))?;
                match v {
                    serde_json::Value::String(sv) => out.push_str(sv),
                    other => out.push_str(&other.to_string()),
                }
                rest = &after[j + 2..];
            }
            out.push_str(rest);
            Ok(serde_json::Value::String(out))
        }
        serde_json::Value::Array(a) => a
            .iter()
            .map(|v| render_fields(v, fields))
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        serde_json::Value::Object(o) => {
            let mut out = serde_json::Map::with_capacity(o.len());
            for (k, v) in o {
                out.insert(k.clone(), render_fields(v, fields)?);
            }
            Ok(serde_json::Value::Object(out))
        }
        other => Ok(other.clone()),
    }
}

fn missing_field_msg(
    name: &str,
    fields: &serde_json::Map<String, serde_json::Value>,
) -> String {
    let mut supplied: Vec<&str> = fields.keys().map(String::as_str).collect();
    supplied.sort_unstable();
    format!(
        "template needs field '{name}', which was not supplied (given: [{}]). \
         Call list-providers to see a vendor's required_fields.",
        supplied.join(", ")
    )
}

/// `Some(name)` when the whole string is exactly one marker in `ns`.
fn whole_marker<'a>(s: &'a str, ns: &str) -> Option<&'a str> {
    let prefix = format!("{{{{{ns}.");
    let inner = s.strip_prefix(&prefix)?.strip_suffix("}}")?;
    if inner.is_empty() || inner.contains("{{") {
        return None;
    }
    Some(inner)
}

pub fn parse_request(input: &[u8]) -> Result<EnrollReq, String> {
    let req: EnrollReq =
        serde_json::from_slice(input).map_err(|e| format!("enroll: bad input: {e}"))?;
    if req.provider_id.trim().is_empty() {
        return Err("enroll: provider_id must not be empty".to_string());
    }
    assert_no_pii(&req.fields)?;
    Ok(req)
}

// ── host-side ───────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
use crate::host::interfaces::{http_with_placeholders as hwp, logging};
#[cfg(target_arch = "wasm32")]
use crate::registry;

#[cfg(target_arch = "wasm32")]
pub fn enroll(input: &[u8]) -> Result<Vec<u8>, String> {
    let req = parse_request(input)?;
    let cfg = registry::load_provider(&req.provider_id)?;

    // {{field.*}} substituted here; {{profile.*}} deliberately survives.
    let body = render_fields(&cfg.body, &req.fields)?;

    let mut headers: Vec<(String, String)> = cfg.headers.clone().unwrap_or_default();
    if let Some(secret_key) = cfg.secret_key.as_deref() {
        let secret = registry::load_secret(secret_key)?;
        let header = cfg.auth_header.clone().unwrap_or_else(|| "Authorization".to_string());
        let format_str = cfg.auth_format.clone().unwrap_or_else(|| "Bearer {key}".to_string());
        headers.push((header, format_str.replace("{key}", &secret)));
    }
    // Content-Type is set by the host; sending it here duplicates it.
    headers.retain(|(k, _)| !k.eq_ignore_ascii_case("content-type"));

    // Log the marker names, never resolved values — this line is safe to keep.
    let _ = logging::info(&format!(
        "enroll: provider={} url={} profile_markers={:?}",
        req.provider_id,
        cfg.url,
        registry::collect_markers(&cfg.body, "profile"),
    ));

    let resp = hwp::call(&hwp::Request {
        method: verb_of(&cfg.method)?,
        url: cfg.url.clone(),
        headers: Some(headers),
        payload: Some(serde_json::to_vec(&body).map_err(|e| e.to_string())?),
    })
    .map_err(|e| format!("provider '{}': {}", req.provider_id, describe_error(e)))?;

    let ok = (200..300).contains(&resp.code);
    if !ok {
        let _ = logging::error(&format!(
            "enroll: provider={} HTTP {}",
            req.provider_id, resp.code
        ));
    }

    let reference = serde_json::from_slice::<serde_json::Value>(&resp.payload)
        .ok()
        .and_then(|v| extract_reference(&v));

    let echo = if cfg.echo_response {
        let _ = logging::debug("enroll: echo_response is on — vendor response returned to caller");
        serde_json::from_slice::<serde_json::Value>(&resp.payload).ok()
    } else {
        None
    };

    let out = EnrollResult {
        provider_id: req.provider_id,
        status: if ok { "enrolled" } else { "rejected" }.to_string(),
        http_code: resp.code,
        reference,
        echo,
    };
    serde_json::to_vec(&out).map_err(|e| e.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn enroll(input: &[u8]) -> Result<Vec<u8>, String> {
    // Validation still runs natively, so `cargo test` covers the guard rails.
    let _ = parse_request(input)?;
    Err("enroll requires the wasm32 target and a live tenant context".to_string())
}

/// Pull a vendor-assigned identifier out of a response, tolerating the handful
/// of shapes vendors actually use. Best-effort: absence is not an error.
pub fn extract_reference(v: &serde_json::Value) -> Option<String> {
    const KEYS: &[&str] = &["id", "reference", "enrollment_id", "confirmation", "ref"];
    let direct = KEYS.iter().find_map(|k| v.get(*k).and_then(|x| x.as_str()));
    if let Some(s) = direct {
        return Some(s.to_string());
    }
    let data = v.get("data")?;
    KEYS.iter()
        .find_map(|k| data.get(*k).and_then(|x| x.as_str()))
        .map(ToString::to_string)
}

#[cfg(target_arch = "wasm32")]
fn verb_of(method: &str) -> Result<hwp::Verb, String> {
    // The WIT interface takes an enum, not a string — see findings/BUGS.md BUG-08.
    match method.to_ascii_uppercase().as_str() {
        "POST" => Ok(hwp::Verb::Post),
        "PUT" => Ok(hwp::Verb::Put),
        "PATCH" => Ok(hwp::Verb::Patch),
        "GET" => Ok(hwp::Verb::Get),
        "DELETE" => Ok(hwp::Verb::Delete),
        other => Err(format!(
            "unsupported method '{other}' in provider config (use GET/POST/PUT/PATCH/DELETE)"
        )),
    }
}

/// Turn a host error into something an operator can act on. Never contains
/// resolved PII — only marker names and host-side reasons.
#[cfg(target_arch = "wasm32")]
fn describe_error(e: hwp::HttpError) -> String {
    match e {
        hwp::HttpError::EgressDenied(host) => format!(
            "egress denied for host '{host}'. Outbound HTTP is authorised by the \
             calling user's grant, not by this contract — add the host to the \
             employee's allowed-hosts grant via member-delegation-update."
        ),
        hwp::HttpError::PlaceholderDenied(m) => format!(
            "placeholder '{m}' not permitted — the delegation grant does not \
             cover this profile field."
        ),
        hwp::HttpError::PlaceholderUnknown(f) => format!(
            "the employee's profile has no field '{f}'. Remove the marker from \
             the provider template, or supply the value as a {{{{field.*}}}} \
             argument if it is not personal data."
        ),
        hwp::HttpError::PlaceholderNoUserContext => {
            "no user context bound — this contract must be invoked on behalf of \
             an employee, not called directly by the tenant."
                .to_string()
        }
        hwp::HttpError::UpstreamError(r) => format!("upstream: {r}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, serde_json::Value)]) -> serde_json::Map<String, serde_json::Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    // ── PII guard ───────────────────────────────────────────────────────────

    #[test]
    fn rejects_pii_by_field_name() {
        for name in ["given_name", "employeeEmail", "date_of_birth", "home_address", "iban"] {
            let f = fields(&[(name, serde_json::json!("x"))]);
            assert!(
                assert_no_pii(&f).is_err(),
                "field '{name}' should have been rejected"
            );
        }
    }

    #[test]
    fn rejects_pii_by_value_shape_even_under_an_innocent_name() {
        // A caller renaming the field must not defeat the guard.
        for v in ["jane.smith@example.com", "1990-04-17", "4111 1111 1111 1111"] {
            let f = fields(&[("code", serde_json::json!(v))]);
            assert!(assert_no_pii(&f).is_err(), "value '{v}' should have been rejected");
        }
    }

    #[test]
    fn allows_ordinary_operational_values() {
        let f = fields(&[
            ("plan_id", serde_json::json!("GOLD-2026")),
            ("cost_centre", serde_json::json!("CC-4417")),
            ("headcount", serde_json::json!(3)),
            ("start_date", serde_json::json!("2026-10-01T09:00:00Z")), // not a bare DOB
            ("active", serde_json::json!(true)),
        ]);
        assert!(assert_no_pii(&f).is_ok(), "{:?}", assert_no_pii(&f));
    }

    #[test]
    fn pii_error_names_the_fix() {
        let f = fields(&[("first_name", serde_json::json!("Jane"))]);
        let e = assert_no_pii(&f).unwrap_err();
        assert!(e.contains("{{profile."), "error should point at the fix: {e}");
    }

    // ── rendering ───────────────────────────────────────────────────────────

    #[test]
    fn profile_markers_survive_rendering_untouched() {
        let tpl = serde_json::json!({
            "given_name": "{{profile.first_name}}",
            "plan": "{{field.plan_id}}"
        });
        let out = render_fields(&tpl, &fields(&[("plan_id", serde_json::json!("GOLD"))])).unwrap();
        assert_eq!(out["given_name"], "{{profile.first_name}}");
        assert_eq!(out["plan"], "GOLD");
    }

    #[test]
    fn whole_string_marker_preserves_json_type() {
        let tpl = serde_json::json!({ "seats": "{{field.seats}}" });
        let out = render_fields(&tpl, &fields(&[("seats", serde_json::json!(4))])).unwrap();
        assert_eq!(out["seats"], serde_json::json!(4), "number became a string");
    }

    #[test]
    fn embedded_marker_interpolates_as_text() {
        let tpl = serde_json::json!({ "sku": "plan-{{field.tier}}-annual" });
        let out = render_fields(&tpl, &fields(&[("tier", serde_json::json!("gold"))])).unwrap();
        assert_eq!(out["sku"], "plan-gold-annual");
    }

    #[test]
    fn renders_inside_arrays_and_nested_objects() {
        let tpl = serde_json::json!({ "a": [{ "b": "{{field.v}}" }] });
        let out = render_fields(&tpl, &fields(&[("v", serde_json::json!("ok"))])).unwrap();
        assert_eq!(out["a"][0]["b"], "ok");
    }

    #[test]
    fn missing_field_error_lists_what_was_supplied() {
        let tpl = serde_json::json!({ "plan": "{{field.plan_id}}" });
        let e = render_fields(&tpl, &fields(&[("other", serde_json::json!("x"))])).unwrap_err();
        assert!(e.contains("plan_id"), "{e}");
        assert!(e.contains("other"), "should list supplied keys: {e}");
        assert!(e.contains("list-providers"), "should point at discovery: {e}");
    }

    #[test]
    fn non_string_leaves_are_untouched() {
        let tpl = serde_json::json!({ "n": 1, "b": true, "z": null });
        let out = render_fields(&tpl, &serde_json::Map::new()).unwrap();
        assert_eq!(out, tpl);
    }

    // ── request parsing ─────────────────────────────────────────────────────

    #[test]
    fn parse_request_rejects_empty_provider_id() {
        let e = parse_request(br#"{"provider_id":"  "}"#).unwrap_err();
        assert!(e.contains("must not be empty"), "{e}");
    }

    #[test]
    fn parse_request_rejects_pii_before_any_network_work() {
        let e = parse_request(
            br#"{"provider_id":"acme","fields":{"email":"jane@example.com"}}"#,
        )
        .unwrap_err();
        assert!(e.contains("personal data"), "{e}");
    }

    #[test]
    fn parse_request_accepts_a_clean_call() {
        let r = parse_request(br#"{"provider_id":"acme","fields":{"plan_id":"GOLD"}}"#).unwrap();
        assert_eq!(r.provider_id, "acme");
        assert_eq!(r.fields.len(), 1);
    }

    // ── response handling ───────────────────────────────────────────────────

    #[test]
    fn extracts_reference_from_common_shapes() {
        assert_eq!(
            extract_reference(&serde_json::json!({"id":"enr_1"})).as_deref(),
            Some("enr_1")
        );
        assert_eq!(
            extract_reference(&serde_json::json!({"data":{"enrollment_id":"e2"}})).as_deref(),
            Some("e2")
        );
        assert_eq!(extract_reference(&serde_json::json!({"nothing":1})), None);
    }
}
