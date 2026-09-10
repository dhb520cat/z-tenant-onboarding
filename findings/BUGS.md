# T3N ADK — Documentation & SDK Findings

Submitted as part of the Superteam Earn "T3N Agent Build Challenge".
Each finding lists: where, what's wrong, why it matters, and a suggested fix.

---

## BUG-01 · Duplicate `trustAnchor` key and duplicate import in the official AI-assistant skill file

**Page:** `/developers/adk/support/ai-coding-assistants` — "Full skill file — t3n-adk-quickstart/SKILL.md", Step 3

**What's wrong:** The canonical connection snippet contains two distinct duplications.

1. `fetchTrustedManifest` is imported twice in the same import statement.
2. `trustAnchor` appears **twice as a key in the same object literal** passed to `new T3nClient({...})`.

```typescript
import {
  T3nClient,
  setEnvironment,
  loadWasmComponent,
  fetchTrustedManifest,        // <-- first
  eth_get_address,
  metamask_sign,
  createEthAuthInput,
  fetchTrustedManifest,        // <-- duplicate import
} from "@terminal3/t3n-sdk";

const t3n = new T3nClient({
  trustAnchor: await fetchTrustedManifest("testnet"),   // <-- first
  wasmComponent,
  trustAnchor: await fetchTrustedManifest("testnet"),   // <-- duplicate key
  handlers: { EthSign: metamask_sign(address, undefined, T3N_API_KEY) },
});
```

**Why it matters — this is the highest-impact doc bug on the site:**
- This file's *entire purpose* is to be dropped into a project so an AI coding assistant copies from it
  verbatim. A defect here propagates into every project that follows the documented install path.
- `ts` with `--strict` / `noDuplicateObjectKeys`-style lint rules **fails to compile** on the duplicate key.
  A first-time user hits a compile error in the very first file the docs tell them to write.
- Even where it compiles, `await fetchTrustedManifest("testnet")` is evaluated **twice** — a redundant
  network round-trip to the trust-anchor endpoint on every client construction, and the first result is
  silently discarded.
- It contradicts the same file's own pitfalls table, which lists the *missing* `trustAnchor` error and
  tells the reader to "Pass `trustAnchor: await fetchTrustedManifest(env)`" — singular.

**Suggested fix:** Remove the duplicate import and the second `trustAnchor` key.

```typescript
import {
  T3nClient,
  setEnvironment,
  loadWasmComponent,
  fetchTrustedManifest,
  eth_get_address,
  metamask_sign,
  createEthAuthInput,
} from "@terminal3/t3n-sdk";

const t3n = new T3nClient({
  trustAnchor: await fetchTrustedManifest("testnet"),
  wasmComponent,
  handlers: { EthSign: metamask_sign(address, undefined, T3N_API_KEY) },
});
```

**Severity:** High (blocks first-run for strict-mode TypeScript users; propagates to all AI-assisted builds)

---

## BUG-02 · "Payroll Agent" use-case page is listed in navigation but has no content

**Page:** `/developers/adk/use-cases/payroll-agent`

**What's wrong:** The page is listed as a top-level use-case in the sidebar (labelled *"Payroll Agent — Coming Soon"*)
and appears in `llms.txt` as its own entry. Its entire body is:

```markdown
# Payroll Agent

See [Delegate Access to AI Agents](/t3n/use-cases/delegate-access-to-agent#payroll)
```

**Why it matters:** Payroll is the flagship example used in T3N's own marketing (the ADK landing page
animates `payroll-agent` calling `bank.transfer($4,250)`). A developer who arrives from that landing page
looking for the worked example lands on an empty stub. It is also the only entry under "use-cases", so the
entire use-case section of the ADK docs is effectively empty.

**Suggested fix:** Either (a) redirect the URL to the anchor it points at, so it doesn't occupy a
navigation slot with no content, or (b) drop it from the sidebar and `llms.txt` until it has a body.
A stub that only forwards costs the reader a page load and a back-navigation.

**Severity:** Low (no functional impact, but it is the single most-advertised use case)

---

## BUG-03 · Walkthrough's `world.wit` pins host interface versions that do not exist in the reference repo

**Pages:** `/developers/adk/get-started/walkthrough/write-contract`
**Repo:** `github.com/Terminal-3/z-tenant-flight` (cloned at `HEAD`, 2026-09-10)

**What's wrong:** Every host-interface version in the documented `world.wit` differs from the one the
reference implementation actually vendors and imports.

| Import | Docs say | Reference repo actually has |
|---|---|---|
| `host:tenant/tenant-context` | `@1.2.0` | `@1.0.0` |
| `host:interfaces/logging` | `@2.2.0` | `@2.1.0` |
| `host:interfaces/kv-store` | `@2.2.0` | `@2.1.0` |
| `host:interfaces/http` | `@2.2.0` | `@2.1.0` |
| `host:interfaces/http-with-placeholders` | `@2.2.0` | `@2.1.0` |

The prose has the same mismatch — it tells the reader to vendor
"`host-interfaces-2.2.0/` and `host-tenant-1.2.0/`", while `wit/deps/` in the repo contains
`host-interfaces-2.1.0/`, `host-tenant-1.0.0/` and a third package the docs never mention,
`host-outbox-1.0.0/`.

**Why it matters — this is a hard failure, not a cosmetic drift.** The same page states:

> The host links your contract against the matching tenant world and **refuses to load it** if it
> imports an interface that world does not provide.

So a developer who follows the walkthrough literally — writing `world.wit` from the page rather than
using the clone — produces a contract that is rejected at registration/admit time, with an error that
points at the host rather than at the docs. The page's own instruction ("Clone the reference
implementation now rather than typing the Rust code by hand") is what saves most readers; anyone who
skims and types the WIT by hand hits this.

**Suggested fix:** Generate the `world.wit` and repo-structure blocks on this page from the reference
repo at build time, or pin the doc to a tagged release of `z-tenant-flight` and state which cluster
version that release targets. At minimum, correct the five version numbers and add `host-outbox` to the
`wit/deps/` listing.

**Severity:** High (produces a contract the host refuses to load)

---

## BUG-04 · Walkthrough's `Cargo.toml` omits the `hex` dependency that the same documentation set requires

**Pages:** `/developers/adk/get-started/walkthrough/write-contract` (Cargo.toml block)
vs. `/developers/adk/support/ai-coding-assistants` (pitfalls table)

**What's wrong:** The documented `Cargo.toml` lists exactly three dependencies:

```toml
[dependencies]
wit-bindgen = { version = "0.49", ... }
serde       = { version = "1.0",  ... }
serde_json  = { version = "1.0",  ... }
```

The reference repo has a fourth, which the docs omit:

```toml
hex = { version = "0.4", default-features = false, features = ["alloc"] }
```

**Why it matters:** `hex` is not optional — it is required by the one piece of map-path handling the
documentation explicitly calls out as a known pitfall. From the official skill file's pitfalls table:

> *"`tenant_did()` returns raw bytes and must be hex-encoded once when building the `z:<tid>:` map path"*
> — **Fix:** *"Build the path with `hex::encode(&tenant_did())`"*

A developer who scaffolds `Cargo.toml` from the walkthrough and then applies the documented fix gets
`error[E0432]: unresolved import 'hex'`. The two pages are internally inconsistent: one tells you to use
the crate, the other gives you a manifest without it.

**Suggested fix:** Add the `hex` line to the walkthrough's `Cargo.toml` block, matching the reference repo.

**Severity:** Medium (compile error, but with a self-evident fix once hit)

---

## BUG-05 · The SDK ships fully obfuscated, and the repository its `package.json` points to is unreachable

**Package:** `@terminal3/t3n-sdk@5.14.0` (npm) · **Declared licence:** MIT
**Declared repository:** `git+https://github.com/Terminal-3/trinity.git` (directory `client/t3n-sdk`)

**What's wrong:** Three findings that compound each other.

1. **`dist/` is obfuscated, not merely minified.** `dist/index.js` and `dist/index.esm.js` (1.2 MB each)
   contain **96,293** hexadecimal identifiers of the `_0x[0-9a-f]{4,8}` form — the signature of a
   JavaScript *obfuscator*, not a minifier. Minified output keeps string literals and control flow
   inspectable; obfuscated output deliberately does not. Consequently:
   - Not one endpoint URL is recoverable by static analysis. Grepping the whole package for
     `https?://` yields only three GitHub links in metadata — no API host, no RPC endpoint.
   - The 22 `fetch` call sites cannot be attributed to any destination without executing the code.

2. **The declared source repository does not resolve.** `gh repo view Terminal-3/trinity` returns
   *"Could not resolve to a Repository"*. The organisation has many genuine public repos
   (`z-tenant-flight`, `adk-getting-start`, `openfhe-rs`, `zktls`, …), but not this one. The MIT licence
   in `package.json` therefore cannot be exercised — there is no corresponding source to inspect,
   fork or audit.

3. **This package handles a private key.** Its own type declarations state:
   ```typescript
   function eth_get_address(privateKey: string): string;
   function metamask_sign(account: EthAccount, logger?: Logger, privateKey?: string): GuestTo…;
   ```
   and the documented quickstart passes `T3N_API_KEY` straight into both. The API key *is* an
   Ethereum private key, and the code path that signs with it is unauditable.

**Why it matters:** This is not a style complaint — it is a direct contradiction of the product's own
value proposition. T3N sells a *verifiable* trust layer: TEE attestation, an auditable ledger, a trust
anchor the client is required to verify (`fetchTrustedManifest` is mandatory and `T3nClient` throws
without it). The docs even devote a page to *"Verify the trust anchor"*. Yet the client that performs
that verification, and that holds the signing key, is a black box whose source is unavailable.

Any enterprise security review — exactly the buyer this product targets — stops here. A reviewer cannot
answer "where does our signing key go?" from the artefact, and cannot obtain the source to answer it
another way.

**Mitigating factors (verified, and worth stating alongside the finding):** the package contains no
local-exfiltration or dynamic-code primitives. Counts in `dist/index.js`: `eval(` **0**,
`new Function` **0**, `child_process` **0**, `fs.readFile`/`fs.writeFile` **0**, `os.homedir` **0**,
`process.env` **0** — notably, the SDK never reads the environment itself; the key is always passed in
explicitly by the caller. There is no npm `install`/`postinstall` hook, no `build.rs` in the reference
contract, and `z-tenant-flight/.cargo/config.toml` sets only `target`, with no `rustflags`, linker
override or registry substitution. All Rust dependencies resolve to crates.io.

**Suggested fix, in order of value:**
1. Publish `client/t3n-sdk` as a real public repository (or correct the `repository` field to wherever
   the source actually lives), so the declared MIT licence is meaningful.
2. Ship **minified** rather than obfuscated bundles, and include source maps. Nothing in an SDK whose
   selling point is verifiability benefits from obfuscation.
3. Publish with npm **provenance** (`npm publish --provenance`) so the artefact is cryptographically
   linked to the CI run and commit that produced it — the same attestation argument T3N makes for TEEs,
   applied to its own supply chain.
4. Document the exact set of hosts the SDK contacts per environment, so it can be allow-listed by
   egress-restricted enterprise networks.

**Severity:** High (blocks enterprise security review; undermines the product's central claim)

---

## BUG-06 · The user-profile schema is never documented, so `{{profile.<field>}}` cannot be used without guessing

**Pages:** `/developers/adk/tips/placeholders-outbound-calls`, `/developers/adk/reference`,
and `terminal-3-openapi.yml`

**What's wrong:** Placeholder substitution is one of the ADK's five headline concepts — the ADK Tour
states it as *"PII moves through the enclave, never through your code"*. Using it requires knowing which
`{{profile.<field>}}` markers exist. **That list is published nowhere.**

Evidence, across the whole documentation surface:

| Source | What it gives you |
|---|---|
| `placeholders-outbound-calls` | Four markers, inside a code sample: `first_name`, `last_name`, `date_of_birth`, `verified_contacts.email.value` |
| `write-contract` | The same four, in the same sample |
| `reference` ("Every confirmed ADK method, WIT host interface, and API surface in one place") | One row describing the `{{profile.<field>}}` *syntax*. No field list. |
| `terminal-3-openapi.yml` (94 KB, 21 paths, 24 operations) | **Zero occurrences** of `first_name`, `date_of_birth`, `verified_contacts`, or any `UserProfile` schema |

A `user-profile` host interface is listed in the reference table, but only as
*"Confirmed to exist — verify current maturity before relying on one"*, with no schema.

**Why it matters:** The docs explicitly tell the reader the schema has a boundary without saying where
it is:

> *"Fields the schema doesn't carry yet (passport, title) are supplied by your contract directly."*

So a developer knows some fields exist and some don't, and has no way to determine which is which except
by trial and error against a live cluster — where the failure mode is a runtime error,
`placeholder not permitted: <marker>`, on a call that has already consumed credits.

This blocks the design step, not just the coding step. Deciding whether T3N fits a given business
process means asking "is the PII this flow needs resolvable as a placeholder?" — and that question is
currently unanswerable from the documentation. In our own case it invalidated a planned contract design
(supplier bank details) that could not have been known to be unsupported before implementation.

**Suggested fix:** Publish the profile schema as a reference table — field path, type, whether it is
verified, and which delegation scope is required to resolve it — next to the placeholder documentation,
and add the corresponding schema object to the OpenAPI spec so it stays in sync. If the schema is still
in flux, say so explicitly and list what is stable today.

**Severity:** High (a headline capability is undocumented to the point of being unusable without guessing)

---

## BUG-07 · The reference implementation instructs you to declare a manifest that the documentation says does not exist

**Repo:** `github.com/Terminal-3/z-tenant-flight` → `src/lib.rs` lines 18–26
**Pages contradicted:** `capabilities-from-wit-import`, `write-contract`, `register-contract`

**What's wrong:** The official reference contract's own doc-comment tells the reader to declare host
capabilities in a manifest:

```rust
//! # Host-capability requirements
//!
//! Declare in manifest (access to a user's profile is gated by the on-chain
//! agent delegation grant, not a per-field allowlist):
//! ```json
//! {
//!   "host_capabilities": [
//!     "kv_store", "logging", "tenant_context", "http", "http_with_placeholders"
//!   ]
//! }
//! ```
```

The documentation denies the existence of that manifest in three separate places:

| Page | Text |
|---|---|
| `tips/capabilities-from-wit-import` | *"You don't declare capabilities in a manifest — **there isn't one**."* |
| `walkthrough/write-contract` | *"…your contract's entire capability set — **there is no separate manifest**."* |
| `walkthrough/register-contract` | *"The register payload is just `{ tail, version, wasm }`; **there is no manifest**."* |

**Why it matters:** The walkthrough explicitly directs readers into this file —
*"Clone the reference implementation now rather than typing the Rust code by hand"* — so the contradiction
is on the documented happy path, not in some corner. A developer following instructions opens `lib.rs`,
reads that capabilities must be declared in a manifest with a specific JSON shape, and then cannot find
anywhere to put it. The likeliest outcomes are a support question, or time lost searching the SDK for a
manifest parameter that was removed.

The capability model itself is one of the ADK's more subtle concepts (imports in `world.wit` *are* the
capability set, enforced at link time). Leaving a stale contradictory description of it inside the
canonical example undermines the one page written to explain it.

**Suggested fix:** Delete or rewrite the `# Host-capability requirements` block in
`z-tenant-flight/src/lib.rs` so it describes the WIT-import model, e.g. *"Capabilities are the host
interfaces imported in `wit/world.wit`; there is no manifest."* Worth grepping the other example repos
(`adk-getting-start`, `adk-circle-call-centre-agent-demo`) for the same stale block.

**Severity:** Medium (no runtime failure, but it contradicts three doc pages on the documented path)

---

## BUG-08 · The `http-with-placeholders` code sample won't compile — `method` and `headers` have the wrong types

**Page:** `/developers/adk/tips/placeholders-outbound-calls`
**Ground truth:** `wit/deps/host-interfaces-2.1.0/package.wit`, and `z-tenant-flight/src/booking.rs`

**What's wrong:** The documented call site passes a `String` where the interface declares an `enum`, and a
bare `Vec` where it declares an `option`.

Documented sample:

```rust
let resp = hwp::call(&hwp::Request {
    method:  "POST".to_string(),                       // <-- String
    url:     "https://api.duffel.com/air/orders".to_string(),
    headers: vec![                                     // <-- bare Vec
        ("Authorization".to_string(), format!("Bearer {api_key}")),
        ("Duffel-Version".to_string(), "v2".to_string()),
        ("Content-Type".to_string(), "application/json".to_string()),
    ],
    payload: Some(serde_json::to_vec(&body)?),
})?;
```

The WIT interface it binds to:

```wit
enum verb { get, post, put, patch, delete }

record request {
  method:  verb,                                    // not string
  url:     string,
  headers: option<list<tuple<string, string>>>,     // not a bare list
  payload: option<list<u8>>
}
```

And the reference implementation, which does it correctly:

```rust
let resp = hwp::call(&hwp::Request {
    method: hwp::Verb::Post,                  // enum variant
    url: alloc::format!("{DUFFEL_BASE}/air/orders"),
    headers: Some(duffel_headers(&api_key)),  // wrapped in Some
    payload: Some(serde_json::to_vec(&order_body).map_err(|e| e.to_string())?),
})
```

**A third defect in the same snippet:** it sets `Content-Type` explicitly. The reference implementation
warns against exactly this, in a comment on the header builder:

> *"Content-Type is set automatically by the host HTTP function via `.json()` — sending it explicitly
> creates a duplicate that Duffel rejects."*

So a developer who fixes the two type errors still ends up with a request the upstream API rejects, and
the cause is documented only inside the example repo, not on the page that teaches the call.

**Why it matters:** This page is the canonical explanation of placeholder substitution — the ADK's
headline privacy feature. Its only code sample fails at `cargo build` with two type mismatches, and once
those are fixed, fails at runtime against the very API the sample targets.

**Suggested fix:** Replace the snippet with the working call from `booking.rs`, and move the
`Content-Type` warning onto this page.

**Severity:** High (compile failure, then a runtime failure, in the flagship feature's only example)

---

## BUG-09 · WIT interface docs and the ADK docs disagree on who authorizes outbound HTTP

**Sources:** `wit/deps/host-interfaces-2.1.0/package.wit` (`http-with-placeholders.http-error`)
vs. `/developers/adk/tips/outbound-http-auth-by-user`

**What's wrong:** The two normative descriptions of the egress policy name different subjects.

The WIT interface, shipped in the reference repo and read by anyone inspecting the host ABI:

```wit
variant http-error {
  /// Target host is not on the contract's `http_allow_list`. Payload
  /// is the offending host string for operator diagnostics.
  egress-denied(string),
  ...
}
```

The documentation page devoted to this exact question opens by denying that model:

> *"Your TEE contract **does not declare which hosts it may call**. A tenant contract's outbound HTTP
> egress is resolved, on every call, from the **calling user's authorization grant** — the allowed hosts
> the user grants when they delegate to your agent or contract."*

So one says the contract carries an allow-list; the other says the contract carries nothing and the
user's grant decides.

**Why it matters:** The docs flag this as the single most common failure mode —

> *"This is the most common reason a working contract can't reach its API: the code is fine, but no grant
> authorizes the host."*

A developer hitting `host/http.egress_denied` will very reasonably read the error's own documentation —
the WIT comment — and go looking for a contract-level `http_allow_list` to add the host to. No such
thing is configurable anywhere in the documented surface (registration takes only `{tail, version, wasm}`,
and capabilities come from WIT imports). This is the same failure pattern as the stale manifest comment
in BUG-07: an authoritative-looking artefact describing a configuration mechanism that the documentation
says does not exist, encountered at precisely the moment the developer is debugging.

**Suggested fix:** Update the `egress-denied` doc-comment in `host-interfaces` to name the actual
subject, e.g. *"Target host is not on the calling user's allowed-hosts grant."* If `http_allow_list` is a
real internal/operator-side concept, say which layer owns it and note that it is not tenant-configurable.

**Severity:** Medium (misdirects debugging of the documented most-common failure)

---

## BUG-10 · 🔴 BLOCKER — the testnet trust manifest is an older schema than the SDK requires, so no environment can complete a verified handshake

**Severity: Blocker.** This stops the documented Quickstart at its first network call, for every user,
on every environment, with a stock `npm install`.

**Reproduce** (SDK `@terminal3/t3n-sdk@5.14.0`, 2026-09-10):

```typescript
setEnvironment("testnet");
const anchor = await fetchTrustedManifest("testnet");   // throws
```

```
Error: Trust manifest at https://cn-api.sg.testnet.t3n.terminal3.io/api/trust-manifest is malformed.
    at fetchTrustedManifest (…/@terminal3/t3n-sdk/dist/index.esm.js:2:1727615)
```

**The manifest itself is fine.** `GET https://cn-api.sg.testnet.t3n.terminal3.io/api/trust-manifest`
returns HTTP 200, `application/json`, 518 bytes, well-formed and signed:

```json
{
  "cluster": "testnet",
  "version": 1787800421,
  "peer_ids": ["QmPk4AtbFore74…", "QmQBh7yAKG1kwx…", "QmSGy7LgDCF796…"],
  "rtmr3_allowlist": ["+XO6nLsfqnTkX0VcNk9AaXAu79ErxURODtjuGOIF8Sk7OQYq3PVVsMG8jzDEeNJQ"],
  "signed_at": "2026-08-27T03:13:41Z",
  "signature": "387384a9186bd06ab8ce8e2fbb7055ae…"
}
```

**Root cause — a schema-version skew, not corruption.** SDK 5.14.0's own `TrustAnchor` type
(`dist/index.d.ts`) requires four fields; the served manifest supplies two of them:

| Field required by SDK 5.14.0 | In the served manifest? |
|---|---|
| `expected_peer_ids` (from `peer_ids`) | ✅ |
| `rtmr3_allowlist` — *"Must be non-empty"* | ✅ |
| `rtmr1_allowlist` — *"Must be non-empty"* | ❌ **absent** |
| `sev_snp_measurement_allowlist` | ❌ **absent** |

The SDK's own doc-comments explain why this matters, and imply the cluster is behind on a security fix:

> `rtmr3_allowlist`: *"Kept for backward compatibility — see TrustAnchor above for why **this alone no
> longer protects against SP-003**."*
>
> `rtmr1_allowlist`: *"**This is the real SP-003 mitigation.**"*

So the cluster is still publishing a pre-SP-003 manifest (signed 2026-08-27), while the current
published SDK requires the post-SP-003 shape. Beyond blocking onboarding, this means the testnet
attestation policy does not currently carry the SP-003 mitigation the SDK considers mandatory.

**All three environments fail** (probed by hooking `fetch` and calling `fetchTrustedManifest` per env):

| `setEnvironment(…)` | URL the SDK resolves | Result |
|---|---|---|
| `sandbox` | `https://cn-api.sg.testnet.t3n.terminal3.io/api/trust-manifest` | `… is malformed` |
| `testnet` | `https://cn-api.sg.testnet.t3n.terminal3.io/api/trust-manifest` | `… is malformed` |
| `production` | *(no request issued)* | `No trust-manifest operator key pinned for environment "production" — signed trust manifests are not provisioned` |

**There is no correct workaround.** `verify-trust-anchor` states the only alternative,
`{ unsafe_trust_server: true }`, is for local dev nodes only, and warns specifically against catching a
verification failure and substituting it. A developer who follows the documentation exactly is stopped;
one who does not, silently loses attestation. (We proceeded with the opt-out only to continue
evaluating the platform, unconditionally and loudly logged, never inside a `catch` — see
`app/connect.ts`.)

**Two secondary reporting defects surfaced alongside it:**

1. **The error is misleading.** "malformed" suggests a corrupt or unparseable document. The document is
   valid JSON with a valid signature; it is *schema-outdated*. The message should name the missing
   fields, e.g. *"trust manifest is missing required field(s): rtmr1_allowlist,
   sev_snp_measurement_allowlist — the cluster may be serving an older manifest than this SDK version
   requires."* The SDK already has this diagnostic instinct elsewhere: `TrustAnchorSource` exists
   expressly so a failed handshake can "name which manifest disagreed with the cluster instead of
   leaving an operator to guess."
2. **The documentation's troubleshooting accordion doesn't cover this case.** It anticipates only a
   *fetch* error ("an environment can exist in the SDK before its nodes are actually serving one") and
   advises targeting a different environment — advice that cannot work here, since all environments fail.

**Suggested fix:** Publish a manifest carrying `rtmr1_allowlist` and `sev_snp_measurement_allowlist` on
testnet (and provision the production operator key), and make the SDK's validation error enumerate the
missing fields and the manifest's `signed_at`/`version`.

---

## BUG-11 · `setEnvironment("sandbox")` silently targets the testnet cluster

**What's wrong:** `Environment` is typed `"sandbox" | "testnet" | "production"`, but `sandbox` and
`testnet` resolve to the same node URL. Hooking `fetch` around `fetchTrustedManifest` shows both issuing
requests to `https://cn-api.sg.testnet.t3n.terminal3.io/api/trust-manifest`.

**Why it matters:** The signup flow is branded *"T3N Sandbox — Get free test credits"* and the landing
page says *"The T3N **sandbox** gives developers everything…"*, while the Quickstart tells you to write
`setEnvironment("testnet")`. A developer reasonably concludes these are two different places and may
spend time wondering which one their 20,000 credits live in, or whether they claimed against the wrong
cluster. If the two names are deliberately aliases today, the docs should say so; if `sandbox` is meant
to become a distinct cluster later, code written against it today will silently change target when that
happens.

**Suggested fix:** Document the aliasing explicitly (e.g. *"`sandbox` is currently an alias for
`testnet`"*), or drop `sandbox` from the public `Environment` union until it is a real cluster.

**Severity:** Low–Medium (no failure today; latent silent-retarget risk, plus onboarding confusion)

---

## BUG-12 · 🔴 The flagship example cannot work: `z-tenant-flight` and the placeholder docs both use profile fields that do not exist

**Sources:** `z-tenant-flight/src/booking.rs`, `/developers/adk/tips/placeholders-outbound-calls`
**Method:** probed each candidate marker against a live testnet profile, one marker per contract call,
and read the host's verdict (`app/probe-profile-schema.ts`).

**Measured result** — of the five `{{profile.*}}` markers that appear in official material, **two do not
resolve**:

| Marker | Appears in | Host verdict |
|---|---|---|
| `first_name` | docs + `booking.rs` | ✅ resolved |
| `last_name` | docs + `booking.rs` | ✅ resolved |
| `verified_contacts.email.value` | docs + `booking.rs` | ✅ resolved |
| **`date_of_birth`** | **docs + `booking.rs`** | ❌ *"the employee's profile has no field 'date_of_birth'"* |
| **`gender`** | **`booking.rs`** | ❌ *"profile has no field 'gender'"* |

Also probed and absent: `middle_name`, `full_name`, `display_name`, `email`, `phone`.

**Why it matters:** `book-offer` — the worked example the entire walkthrough builds toward — sends
`"born_on": "{{profile.date_of_birth}}"` and `"gender": "{{profile.gender}}"` in the Duffel order body.
Against a real profile, that call fails at the host before it ever reaches Duffel. Anyone completing the
walkthrough as written hits it.

This is BUG-06 (undocumented schema) turning into a concrete failure. Because the schema is published
nowhere, there is no way to notice the samples are stale except by running them and decoding the error.
Note the samples are not obviously wrong — `booking.rs` even carries a careful comment about which
fields are demo-hardcoded *because* the schema lacks them (`passport`, `title`, phone), which reads as
evidence the author checked. `date_of_birth` and `gender` were evidently not rechecked, or the schema
changed afterwards.

**Suggested fix:** Publish the schema (BUG-06), then correct both samples to use only resolvable markers,
or add the missing fields to the profile. A CI check that resolves every `{{profile.*}}` in the docs and
example repos against a live testnet profile would keep them honest — the probe in
`app/probe-profile-schema.ts` is about 40 lines and could be adapted.

**Severity:** High (the canonical end-to-end example fails against a real profile)

---

## BUG-13 · Re-registering a contract silently invalidates its map ACLs

**What's wrong:** `tenant.maps.create` binds `readers`/`writers` to a **numeric `contract_id`**.
Registering a new version of the same contract mints a **new** `contract_id`, and nothing re-points the
existing maps at it. The contract can no longer read the maps it created.

**Reproduce:**

```
register tail=onboarding v0.1.0            → contract_id 960
maps.create readers/writers = only [960]   → ok
register tail=onboarding v0.1.1            → contract_id 961   (same tail!)
execute v0.1.1 → any kv_store::get         → denied
```

```
kv_store.get on 'z:<tid>:onboarding-providers' read denied:
access denied: TenantContract(did:t3n:…)
```

**Why it matters:** The error names the KV layer, not the re-registration that caused it, so it reads as
a permissions bug in the map or the contract's own map-name construction — the two things the docs
warn about most (`AccessDenied` on a map whose `readers` was omitted; the single-vs-double hex-encode
path bug). Both of those are dead ends here: the ACL *was* set, and the path *is* correct. It costs a
real debugging session to arrive at "the id moved".

The docs do warn that "Registering a newer version can reroute calls that pin an older one", so
re-registration is known to have version-pinning consequences — but map ACLs are not mentioned there or
on the `create-kv-maps` page.

**Suggested fix:** Note it on `create-kv-maps` and on the registration page: *"Map ACLs bind to
`contract_id`, which changes on every registration. After registering a new version, update each map's
ACL to include the new id (keep the old one during a rollout)."* Better, make the ACL bindable to the
canonical `z:<tid>:<tail>` name so it survives version changes. Failing that, have the KV denial name
the id it saw and the ids the ACL allows — the operator would then diagnose it instantly.
(Workaround used here: `app/fix-map-acl.ts`, which re-points both maps at `[960, 961]`.)

**Severity:** Medium–High (guaranteed to hit anyone who ships a second version; misleading error)

---

## BUG-14 · Two operational rough edges worth documenting

**a. `fuel_per_minute` quota is undocumented.** A loop issuing ~10 contract calls in quick succession
started failing with:

```
RPC Error: quota exceeded (fuel_per_minute): tenant <tid> on contract z:<tid>:onboarding
```

No rate limit appears in the docs or the reference. The credits page frames consumption as a *balance*
("20,000 test credits — enough for 25 agents and ~5,000 protected actions"), which sets the expectation
that spending is capped in total, not per minute. Anyone writing a batch or a test sweep will hit this
and reasonably read it as having burnt their allocation. Please document the per-minute ceiling next to
the credits figure, and have the error state the limit and the retry-after window.

**b. `contracts.register` is not idempotent, which makes deploy scripts fragile.** Re-running a deploy
with an unchanged version fails:

```
RPC Error: contract version invalid: version 0.1.1 is not higher than current version 0.1.1
```

Strict monotonicity is right for the platform. But it means the natural shape of a deploy script —
"register, create maps, seed config", re-run after any downstream failure — dies on the first line the
second time. Our own run hit exactly this: registration succeeded, a later step failed, and the re-run
could not get past step 1. Worth either documenting the pattern (catch and continue when the version
already exists) or offering a `registerOrGetExisting` helper. `maps.create` already gets this right by
making `MapAlreadyExists` benign.

**Severity:** Low (both are papered over in a few lines once known — but only once known)

---
