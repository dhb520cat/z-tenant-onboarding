/**
 * Self-grant the contract's egress, then run a real enrolment.
 *
 * Egress is authorised by the *calling user's* grant, not by the contract, so
 * nothing outbound works until this runs. Here the tenant stands in for the
 * employee (a self-grant: grantee === own DID).
 *
 * The vendor is httpbin.org/post, which echoes the request body back. That
 * makes the privacy property directly observable:
 *
 *   • if the enclave substitutes the markers → the echo shows real values
 *   • if it does not                          → the echo shows "{{profile.…}}"
 *
 * Either outcome is a finding, which is the point of testing it this way.
 *
 * Run:  set -a && . ../.secrets/t3n.env && set +a && npx tsx grant-and-enroll.ts
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
const VENDOR_HOST = "httpbin.org";

console.warn("!!! TEE attestation verification DISABLED — cluster manifest broken (BUG-10)\n");

const address = eth_get_address(T3N_API_KEY);
const t3n: any = new T3nClient({
  trustAnchor: { unsafe_trust_server: true } as any,
  wasmComponent: await loadWasmComponent(),
  handlers: { EthSign: metamask_sign(address, undefined, T3N_API_KEY) },
});
await t3n.handshake();
const tenantDid = (await t3n.authenticate(createEthAuthInput(address))).value;
const tenantId = tenantDid.slice("did:t3n:".length);
const contractName = `z:${tenantId}:${TAIL}`;
const tenant: any = new TenantClient({ t3n, baseUrl: getNodeUrl(), tenantDid } as any);
await tenant.tenant.me();

// ── grant ───────────────────────────────────────────────────────────────────
// updateMemberDelegation is the read-merge-write form; the raw
// member-delegation-update call replaces the entire policy and would drop any
// other grants this DID holds.
console.log("=".repeat(70));
console.log(`self-grant: ${contractName} → allowed_hosts: [${VENDOR_HOST}]`);
console.log("=".repeat(70));
try {
  const res = await t3n.updateMemberDelegation({
    grantee: tenantDid,
    contract_id: contractName,
    functions: ["list-providers", "enroll"],
    scopes: [],
    version_req: VERSION,
    allowed_hosts: [VENDOR_HOST],
  });
  console.log("grant written. preserved rows:", res?.preservedRows?.length ?? 0, "\n");
} catch (e: any) {
  console.log("grant FAILED:", String(e?.message ?? e).slice(0, 400), "\n");
}

// ── enrol ───────────────────────────────────────────────────────────────────
console.log("=".repeat(70));
console.log("enroll — real dispatch through http-with-placeholders");
console.log("=".repeat(70));
try {
  const out: any = await tenant.contracts.execute(TAIL, {
    version: VERSION,
    functionName: "enroll",
    input: {
      provider_id: "demo-benefits",
      fields: { plan_id: "GOLD-2026", start_date: "2026-10-01T09:00:00Z" },
    },
  });
  console.log(JSON.stringify(out, null, 2));
} catch (e: any) {
  console.log("FAILED:", String(e?.message ?? e).slice(0, 600));
}

// ── logs ────────────────────────────────────────────────────────────────────
console.log("\n" + "=".repeat(70));
console.log("contract logs");
console.log("=".repeat(70));
try {
  const logs = await tenant.contracts.logs(TAIL, { limit: 10 });
  for (const e of logs?.entries ?? []) console.log(`  [${e.level}] ${e.message}`);
} catch (e: any) {
  console.log("(unavailable)");
}
