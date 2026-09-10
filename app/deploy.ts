/**
 * Deploy z-tenant-onboarding: register the contract, create its maps, and
 * seed one demo vendor.
 *
 * Idempotent — safe to re-run. `MapAlreadyExists` is treated as success, and
 * re-registering at the same tail just mints a new version.
 *
 * Run:  set -a && . ../.secrets/t3n.env && set +a && npx tsx deploy.ts
 */

import { readFile } from "node:fs/promises";

const hosts = new Map<string, number>();
const origFetch = globalThis.fetch;
globalThis.fetch = async (input: any, init?: any) => {
  const raw = typeof input === "string" ? input : (input?.url ?? String(input));
  try {
    const u = new URL(raw);
    const k = `${u.protocol}//${u.host}`;
    hosts.set(k, (hosts.get(k) ?? 0) + 1);
  } catch {}
  return origFetch(input as any, init);
};

const {
  T3nClient, TenantClient, setEnvironment, getNodeUrl, loadWasmComponent,
  eth_get_address, metamask_sign, createEthAuthInput,
} = await import("@terminal3/t3n-sdk");

setEnvironment("testnet");

const T3N_API_KEY = process.env.T3N_API_KEY;
if (!T3N_API_KEY) throw new Error("T3N_API_KEY is not exported into this shell");

const WASM_PATH = "../z-tenant-onboarding/target/wasm32-wasip2/release/z_tenant_onboarding.wasm";
const CONTRACT_TAIL = "onboarding";
const CONTRACT_VERSION = "0.1.0";

// ── connect ─────────────────────────────────────────────────────────────────
// trustAnchor opt-out: the cluster's manifest is missing fields this SDK
// requires, so verified anchoring is impossible right now. See connect.ts and
// findings/BUGS.md BUG-10. Never silent.
console.warn("!!! TEE attestation verification DISABLED — cluster manifest broken (BUG-10)\n");

const address = eth_get_address(T3N_API_KEY);
const t3n = new T3nClient({
  trustAnchor: { unsafe_trust_server: true } as any,
  wasmComponent: await loadWasmComponent(),
  handlers: { EthSign: metamask_sign(address, undefined, T3N_API_KEY) },
});
await t3n.handshake();
const tenantDid = (await t3n.authenticate(createEthAuthInput(address))).value;
console.log("tenant DID:", tenantDid);

const tenant = new TenantClient({ t3n, baseUrl: getNodeUrl(), tenantDid } as any);
await (tenant as any).tenant.me();
console.log("TenantClient ready");

// ── register the contract ───────────────────────────────────────────────────
const wasm = await readFile(WASM_PATH);
console.log(`\nregistering ${CONTRACT_TAIL} v${CONTRACT_VERSION} (${wasm.length} bytes)…`);
const reg: any = await (tenant as any).contracts.register({
  tail: CONTRACT_TAIL,
  version: CONTRACT_VERSION,
  wasm,
});
const contractId = reg.contract_id;
const tenantId = tenantDid.slice("did:t3n:".length);
console.log(`registered z:${tenantId}:${CONTRACT_TAIL} as contract id ${contractId}`);

// ── maps ────────────────────────────────────────────────────────────────────
// readers must be set explicitly: the KV governor defaults to deny, and a
// missing `readers` shows up later as AccessDenied on the contract's own read.
for (const tail of ["onboarding-providers", "secrets"]) {
  try {
    await (tenant as any).maps.create({
      tail,
      visibility: "private",
      writers: { only: [contractId] },
      readers: { only: [contractId] },
    });
    console.log(`map created: z:${tenantId}:${tail}`);
  } catch (e: any) {
    const msg = String(e?.message ?? e);
    if (msg.includes("MapAlreadyExists")) console.log(`map exists:  z:${tenantId}:${tail}`);
    else throw e;
  }
}

// ── seed one demo vendor ────────────────────────────────────────────────────
// httpbin echoes the request body back, which makes it the clearest possible
// proof of the privacy property: if the enclave substitutes the markers, the
// echo shows real values; if it does not, the echo shows the literal
// "{{profile.first_name}}" text. Either way we learn the truth.
const demoVendor = {
  name: "Demo Benefits (httpbin echo)",
  url: "https://httpbin.org/post",
  method: "POST",
  headers: [["Accept", "application/json"]],
  body: {
    employee: {
      given_name: "{{profile.first_name}}",
      family_name: "{{profile.last_name}}",
      born_on: "{{profile.date_of_birth}}",
      email: "{{profile.verified_contacts.email.value}}",
    },
    plan_id: "{{field.plan_id}}",
    start_date: "{{field.start_date}}",
  },
};

await (tenant as any).executeControl("map-entry-set", {
  map_name: (tenant as any).canonicalName("onboarding-providers"),
  key: "demo-benefits",
  value: JSON.stringify(demoVendor),
});
console.log("seeded vendor 'demo-benefits' into z:<tid>:onboarding-providers");

console.log("\n--- egress observed ---");
for (const [h, n] of [...hosts].sort((a, b) => b[1] - a[1])) {
  console.log(`  ${String(n).padStart(3)}x  ${h}`);
}
console.log(`\ncontract_id=${contractId}`);
