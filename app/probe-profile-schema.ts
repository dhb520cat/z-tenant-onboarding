/**
 * Discover which {{profile.*}} markers this cluster can actually resolve.
 *
 * Why this exists: the user-profile schema is not documented anywhere — not on
 * the placeholders page, not in the SDK reference, not in the OpenAPI spec
 * (findings/BUGS.md BUG-06). The only published names come from two code
 * samples, and at least one of those (`date_of_birth`) does not resolve against
 * a real profile. So the only way to learn the schema is to ask the cluster.
 *
 * Method: register a single-marker template per candidate, invoke it, and read
 * the host's answer. The host distinguishes the cases for us:
 *
 *   PlaceholderUnknown  → "profile has no field X"   → not in the schema
 *   PlaceholderDenied   → "not permitted"            → exists, needs a scope
 *   any HTTP result     → resolved                   → exists and is readable
 *
 * Costs one protected action per candidate. Run:
 *   set -a && . ../.secrets/t3n.env && set +a && npx tsx probe-profile-schema.ts
 */

const {
  T3nClient, TenantClient, setEnvironment, getNodeUrl, loadWasmComponent,
  eth_get_address, metamask_sign, createEthAuthInput,
} = await import("@terminal3/t3n-sdk");

setEnvironment("testnet");

const T3N_API_KEY = process.env.T3N_API_KEY;
if (!T3N_API_KEY) throw new Error("T3N_API_KEY is not exported into this shell");

const TAIL = "onboarding";
const VERSION = "0.1.0";
const PROBE_ID = "__schema_probe";

/** Documented names first, then plausible neighbours. */
const CANDIDATES = [
  // from the official samples
  "first_name",
  "last_name",
  "date_of_birth",
  "gender",
  "verified_contacts.email.value",
  // plausible neighbours
  "middle_name",
  "full_name",
  "display_name",
  "email",
  "phone",
  "verified_contacts.phone.value",
  "nationality",
  "country",
  "language",
  "created_at",
];

const address = eth_get_address(T3N_API_KEY);
const t3n: any = new T3nClient({
  trustAnchor: { unsafe_trust_server: true } as any,
  wasmComponent: await loadWasmComponent(),
  handlers: { EthSign: metamask_sign(address, undefined, T3N_API_KEY) },
});
await t3n.handshake();
const tenantDid = (await t3n.authenticate(createEthAuthInput(address))).value;
const tenant: any = new TenantClient({ t3n, baseUrl: getNodeUrl(), tenantDid } as any);
await tenant.tenant.me();

type Verdict = "resolved" | "absent" | "denied" | "other";
const results: Array<{ field: string; verdict: Verdict; detail: string }> = [];

for (const field of CANDIDATES) {
  // One marker per probe, so the host's answer is unambiguous.
  await tenant.executeControl("map-entry-set", {
    map_name: tenant.canonicalName("onboarding-providers"),
    key: PROBE_ID,
    value: JSON.stringify({
      name: `probe ${field}`,
      url: "https://httpbin.org/post",
      method: "POST",
      body: { probe: `{{profile.${field}}}` },
    }),
  });

  let verdict: Verdict = "other";
  let detail = "";
  try {
    const out: any = await tenant.contracts.execute(TAIL, {
      version: VERSION,
      functionName: "enroll",
      input: { provider_id: PROBE_ID, fields: {} },
    });
    verdict = "resolved";
    detail = `http_code=${out?.http_code ?? "?"}`;
  } catch (e: any) {
    const msg = String(e?.message ?? e);
    if (msg.includes("has no field")) {
      verdict = "absent";
      detail = "profile has no such field";
    } else if (msg.includes("not permitted")) {
      verdict = "denied";
      detail = "exists but the grant does not cover it";
    } else {
      detail = msg.replace(/\s*\[[0-9a-f-]{36}\]\s*$/, "").slice(0, 120);
    }
  }
  results.push({ field, verdict, detail });
  const mark = { resolved: "✅", absent: "❌", denied: "🔒", other: "❔" }[verdict];
  console.log(`${mark}  ${field.padEnd(32)} ${detail}`);
}

// Clean up the probe entry so it never shows up in list-providers.
await tenant.executeControl("map-entry-delete", {
  map_name: tenant.canonicalName("onboarding-providers"),
  key: PROBE_ID,
}).catch(() => {
  // Older clusters may not expose map-entry-delete; overwrite instead.
  return tenant.executeControl("map-entry-set", {
    map_name: tenant.canonicalName("onboarding-providers"),
    key: PROBE_ID,
    value: "",
  });
});

console.log("\n--- summary ---");
const by = (v: Verdict) => results.filter((r) => r.verdict === v).map((r) => r.field);
console.log("resolvable:", by("resolved").join(", ") || "(none)");
console.log("absent:    ", by("absent").join(", ") || "(none)");
console.log("denied:    ", by("denied").join(", ") || "(none)");
console.log("unclear:   ", by("other").join(", ") || "(none)");
