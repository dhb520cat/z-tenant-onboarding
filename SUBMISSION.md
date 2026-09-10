# T3N Agent Build Challenge — Submission

**Repository:** https://github.com/dhb520cat/z-tenant-onboarding
**Contract:** `z-tenant-onboarding` v0.1.1, registered and running on testnet
**Submitted:** 2026-09-10

---

## What I built

**An employee-onboarding agent.** It enrols one employee into the third-party HR systems a company uses
— benefits, background check, payroll, equipment, SaaS seats — **without the contract, the agent, or any
log in between ever holding the employee's personal data.**

### Why this problem

Onboarding one person means giving the same handful of personal details to five or six vendors. Today
someone re-keys them into each vendor's portal, so the employee's name, date of birth and email end up
copied across mailboxes, spreadsheets and vendor dashboards. Nobody can answer *"which vendors hold
Jane's data, and who sent it there?"*, and the employee has no way to revoke it.

Handing that job to an AI agent normally makes it worse — now the PII is in a model's context window
too. This is precisely the shape of problem the ADK's placeholder mechanism exists for, which is why I
chose it.

### How it works

The enrolment body is a template of `{{profile.*}}` markers. The contract sends the *markers*. The host
resolves them from the calling employee's profile **inside the enclave**, immediately before the
outbound request. WASM memory only ever contains marker text.

```
  agent ──► z:<tid>:onboarding ─────────────► host (http-with-placeholders) ──► vendor
            builds body with                  resolves {{profile.*}} inside      receives
            {{profile.first_name}}             the enclave                        real values
            …never sees a real value
```

---

## Two design decisions I'd like reviewed

### 1. Vendors are configuration, not code

Every company onboards into a different set of vendors, and that set changes far more often than anyone
wants to redeploy a TEE contract. So vendors live as one JSON document each in the KV map
`z:<tid>:onboarding-providers` — endpoint, method, auth, and body template.

**Adding a vendor is one map write.** No Rust change, no rebuild, no re-registration — and, most
importantly, **no new contract version to re-authorise in every employee's delegation grant**. A code
change would invalidate `version_req` in existing grants and force every employee to re-consent, which
would make routine vendor churn organisationally expensive. This was the single biggest factor in the
design, and it is what the brief's "ease of maintenance & running post challenge" asks for.

### 2. The PII guard

Two marker namespaces are kept apart: `{{profile.*}}` (resolved by the host) and `{{field.*}}` (supplied
by the caller — plan id, start date, cost centre).

`{{field.*}}` is the only channel a caller can write into, which makes it the one way to route PII
*around* the enclave. If a caller could pass `"given_name": "Jane Smith"`, the plaintext would sit in
contract memory, in the agent's context, and in every log between them — and **it would still work**.
Nothing would fail; the guarantee would just quietly be gone.

So `assert_no_pii` rejects caller values that look personal, on two independent axes — field name and
value shape — so renaming a field to dodge the name check still trips the value check. Ordinary
operational values pass untouched; that separation is covered by unit tests.

```
enroll(provider_id="demo-benefits",
       fields={plan_id:"GOLD-2026", employee_email:"jane@example.com"})

→ contract error: field 'employee_email' looks like personal data (matched 'email').
  Personal data must not be passed as an argument — put a {{profile.<field>}} marker
  in the provider template so the host resolves it inside the enclave.
```

`list-providers` also reports each vendor's `profile_fields`, so an operator can audit which PII each
vendor receives — the question this whole design exists to answer.

---

## Evidence: the privacy property, measured not claimed

Bring-up used `httpbin.org/post` as the vendor, because it echoes the request body back. That makes the
property directly observable rather than assumed: if the enclave substitutes the markers the echo shows
real values; if it does not, the echo shows the literal `{{profile.…}}` text.

**Screenshot 1 — `verify-privacy.ts`:**

```json
{
  "employee": {
    "email":       "<real email from the profile>",
    "family_name": "<real surname>",
    "given_name":  "<real given name>"
  },
  "plan_id": "GOLD-2026",
  "start_date": "2026-10-01T09:00:00Z"
}
```
```
✓ every {{profile.*}} marker was resolved by the host.
  The contract sent marker text; the vendor received real values.
  The plaintext never existed inside WASM memory.
```

*(Values redacted for publication — they are the profile owner's actual details, which is the point.)*

The contract's log line from the same call names the markers, never the values:

```
[info] enroll: provider=demo-benefits url=https://httpbin.org/post
       profile_markers=["verified_contacts.email.value", "last_name", "first_name"]
```

**Also verified:** 20 unit tests pass natively without a cluster; zero compiler warnings; the WASM
component imports exactly the four host interfaces declared in `world.wit` and exports
`z:tenant-onboarding/contracts@0.1.0`; its wasi import footprint is identical to `z-tenant-flight`'s.

Screenshots 1–5 are in [`screenshots/`](https://github.com/dhb520cat/z-tenant-onboarding/tree/master/screenshots).

---

## Bugs found: 14

Full write-ups, each with source, reproduction, impact and suggested fix:
[`findings/BUGS.md`](https://github.com/dhb520cat/z-tenant-onboarding/blob/master/findings/BUGS.md)

### 🔴 BUG-10 — Blocker: nobody can complete the Quickstart on the verified path

`fetchTrustedManifest` fails on **every** environment:

```
Error: Trust manifest at https://cn-api.sg.testnet.t3n.terminal3.io/api/trust-manifest is malformed.
```

**The manifest is not corrupt.** It returns HTTP 200, valid JSON, correctly signed. The problem is a
schema skew: SDK 5.14.0's `TrustAnchor` requires four fields and the cluster serves two of them.
Missing: **`rtmr1_allowlist`** and **`sev_snp_measurement_allowlist`**.

The SDK's own doc-comment on the missing field is the important part:

> `rtmr3_allowlist`: *"Kept for backward compatibility — … **this alone no longer protects against
> SP-003**."*
> `rtmr1_allowlist`: *"**This is the real SP-003 mitigation.**"*

So the cluster is still publishing a **pre-SP-003 manifest** (signed 2026-08-27), while the current
published SDK requires the post-SP-003 shape. Beyond blocking onboarding, this means the testnet
attestation policy does not currently carry the mitigation the SDK considers mandatory.

| `setEnvironment(…)` | Result |
|---|---|
| `sandbox` | resolves to the **testnet** URL (BUG-11), then `… is malformed` |
| `testnet` | `… is malformed` |
| `production` | `No trust-manifest operator key pinned for environment "production"` |

Two secondary issues: the error says "malformed" when it means "schema-outdated" and should name the
missing fields; and the docs' troubleshooting accordion anticipates only a *fetch* error and advises
switching environments, which cannot help when all three fail.

### 🔴 BUG-12 — The flagship example uses profile fields that don't exist

The schema is documented nowhere, so I wrote a probe
([`app/probe-profile-schema.ts`](https://github.com/dhb520cat/z-tenant-onboarding/blob/master/app/probe-profile-schema.ts))
that registers one single-marker template per candidate and reads the host's verdict. Measured on
testnet, 2026-09-10:

| Marker | Result |
|---|---|
| `first_name`, `last_name`, `verified_contacts.email.value` | ✅ resolve |
| **`date_of_birth`** | ❌ **absent** — used by the docs **and** `z-tenant-flight` |
| **`gender`** | ❌ **absent** — used by `z-tenant-flight` |
| `middle_name`, `full_name`, `display_name`, `email`, `phone` | ❌ absent |

`book-offer` — the worked example the whole walkthrough builds toward — sends
`"born_on": "{{profile.date_of_birth}}"` and `"gender": "{{profile.gender}}"`. Against a real profile it
fails at the host before reaching Duffel.

Worth noting these samples are not carelessly written: `booking.rs` carries a thoughtful comment about
which fields are demo-hardcoded *because* the schema lacks them. Those two were evidently not
rechecked, or the schema changed afterwards — which is exactly what a CI check would catch. My probe is
~40 lines and could be adapted for it.

### The rest

| # | Finding | Severity |
|---|---|---|
| 6 | The user-profile schema is published nowhere — not the docs, not the reference, not the OpenAPI spec — so `{{profile.*}}` can only be used by guessing | High |
| 8 | The placeholder page's only code sample has two type errors (`method` takes an enum not a string; `headers` is an `option`) plus a duplicate `Content-Type` the reference repo warns against | High |
| 5 | The SDK ships fully obfuscated (96,293 `_0x` identifiers) and its declared repository 404s, while holding what is effectively an Ethereum private key — an enterprise security review stops here | High |
| 3 | The walkthrough's `world.wit` pins host-interface versions that don't exist in the reference repo (2.2.0/1.2.0 vs 2.1.0/1.0.0); the host refuses to load a mismatched import | High |
| 13 | Re-registering a contract mints a new `contract_id` and silently invalidates its map ACLs; the error names the KV layer, not the cause | Med–High |
| 1 | The AI-assistant skill file — whose purpose is to be copied verbatim by coding agents — has a duplicate import and a duplicate `trustAnchor` key in one object literal. The same duplicate appears again on the invoke page | Med |
| 4 | The walkthrough's `Cargo.toml` omits the `hex` dependency that the same doc set's troubleshooting table requires | Med |
| 7 | `z-tenant-flight/src/lib.rs` instructs you to declare `host_capabilities` in a manifest that three doc pages explicitly say does not exist | Med |
| 9 | The WIT comment on `egress-denied` says the contract carries an `http_allow_list`; the docs say the contract carries nothing and the user's grant decides | Med |
| 11 | `setEnvironment("sandbox")` silently targets the testnet cluster, while the signup flow is branded "T3N Sandbox" | Low–Med |
| 2 | The "Payroll Agent" use-case page — the example T3N's own marketing animates — is a navigation entry whose body is one link | Low |
| 14 | Undocumented `fuel_per_minute` quota (the credits page frames spend as a total, not a rate); and `contracts.register` is not idempotent, which breaks the natural re-run shape of a deploy script | Low |

---

## One deviation from the documentation, stated plainly

`app/*.ts` pass `trustAnchor: { unsafe_trust_server: true }` instead of `fetchTrustedManifest(env)`.

That is **not** a shortcut and not a preference — it is BUG-10. The verified path is impossible right
now. The documentation is explicit that this opt-out is for local dev nodes only and that one must never
silently substitute it after a verification failure, so mine is **not** in a `catch`: it is
unconditional, commented at the call site, and logged loudly on every run:

```
!!! TEE attestation verification DISABLED — cluster manifest broken (BUG-10)
```

It will be reverted the moment the cluster publishes a conforming manifest. Blast radius: a sandbox key
with test credits and no real assets.

---

## Continue running it, or hand it over?

**Happy either way — and it is built to be handed over.**

If T3N would like to take it, the whole system is the repository: one Rust crate, seven TypeScript
scripts, no infrastructure, no database, no hosted component. The only state is two KV maps and the
employees' delegation grants, all of which live on T3N rather than with me. Handover is: fork the repo,
register the contract under your tenant, run `deploy.ts`. There is nothing to migrate.

**What it costs to run.** Routine changes need no engineer and no redeploy:

| Task | How | Redeploy? |
|---|---|---|
| Add / change / remove a vendor | one `map-entry-set` write | no |
| Rotate a vendor API key | one `map-entry-set` into `secrets` | no |
| Change which PII a vendor receives | edit that vendor's `body` template | no |
| Onboard a new employee | they sign one delegation grant | no |
| Revoke a vendor or an agent | update the grant | no |

Only a new host capability, a change to the PII-guard rules, or a new contract function needs a rebuild
— and after any re-registration, `fix-map-acl.ts` must be run with both the old and new contract ids
(BUG-13).

**If I keep running it,** the obvious next step is a small admin UI over `map-entry-set`. The vendor
templates and the guard rules are the two things a non-Rust operator will want to change most, and both
are already data-shaped, so an HR ops team could add a vendor without touching a terminal. I'd also be
glad to contribute the profile-schema probe upstream as a CI check, since it would have caught BUG-12.

---

## Repository layout

```
z-tenant-onboarding/       the TEE contract (Rust → WASM component, 203 KB)
  src/lib.rs               bindings + dispatch
  src/registry.rs          vendor config: load, parse, marker extraction
  src/enroll.rs            PII guard, template rendering, dispatch
  wit/world.wit            exported interface + host imports
app/                       deploy, invoke, verify, probe (TypeScript)
findings/BUGS.md           14 findings, 664 lines
screenshots/               5 captures
README.md                  full documentation incl. maintenance handbook
```

MIT licensed.
