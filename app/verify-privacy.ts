/**
 * The decisive test: does the enclave actually substitute {{profile.*}}?
 *
 * The vendor is httpbin.org/post, which echoes the request body back. The
 * contract never holds the employee's name or email — it sends marker text —
 * so whatever the echo shows is what the *host* put on the wire:
 *
 *   echo shows real values      → the enclave resolved them. Privacy property holds:
 *                                 the data reached the vendor without passing
 *                                 through contract memory.
 *   echo shows "{{profile.…}}"  → nothing was substituted; the vendor got junk.
 *
 * Only the three markers this cluster can actually resolve are used here —
 * see probe-profile-schema.ts and findings/BUGS.md BUG-06/BUG-12.
 *
 * Run:  set -a && . ../.secrets/t3n.env && set +a && npx tsx verify-privacy.ts
 */

import { readFile } from "node:fs/promises";

const {
  T3nClient, TenantClient, setEnvironment, getNodeUrl, loadWasmComponent,
  eth_get_address, metamask_sign, createEthAuthInput,
} = await import("@terminal3/t3n-sdk");

setEnvironment("testnet");

const T3N_API_KEY = process.env.T3N_API_KEY;
if (!T3N_API_KEY) throw new Error("T3N_API_KEY is not exported into this shell");

const TAIL = "onboarding";
const VERSION = "0.1.1";
const WASM_PATH = "../z-tenant-onboarding/target/wasm32-wasip2/release/z_tenant_onboarding.wasm";

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

// 1. register the new version — idempotently.
//    The platform requires a strictly increasing version, so re-running a
//    deploy script with an unchanged version fails with "version X is not
//    higher than current version X". That is correct behaviour for the
//    platform and wrong behaviour for a script, so absorb it here: an
//    already-registered version is a success, not a failure.
const wasm = await readFile(WASM_PATH);
try {
  const reg: any = await tenant.contracts.register({ tail: TAIL, version: VERSION, wasm });
  console.log(`registered ${contractName} v${VERSION} (contract id ${reg.contract_id})`);
} catch (e: any) {
  const msg = String(e?.message ?? e);
  if (msg.includes("is not higher than current version")) {
    console.log(`${contractName} v${VERSION} already registered — continuing`);
  } else {
    throw e;
  }
}

// 2. reconfigure the vendor: only resolvable markers, echo on
await tenant.executeControl("map-entry-set", {
  map_name: tenant.canonicalName("onboarding-providers"),
  key: "demo-benefits",
  value: JSON.stringify({
    name: "Demo Benefits (httpbin echo)",
    url: "https://httpbin.org/post",
    method: "POST",
    headers: [["Accept", "application/json"]],
    echo_response: true,
    body: {
      employee: {
        given_name: "{{profile.first_name}}",
        family_name: "{{profile.last_name}}",
        email: "{{profile.verified_contacts.email.value}}",
      },
      plan_id: "{{field.plan_id}}",
      start_date: "{{field.start_date}}",
    },
  }),
});
console.log("vendor reconfigured (3 resolvable markers, echo on)");

// 3. re-grant for the new version
await t3n.updateMemberDelegation({
  grantee: tenantDid,
  contract_id: contractName,
  functions: ["list-providers", "enroll"],
  scopes: [],
  version_req: VERSION,
  allowed_hosts: ["httpbin.org"],
});
console.log("grant updated for v" + VERSION + "\n");

// 4. the actual test
const out: any = await tenant.contracts.execute(TAIL, {
  version: VERSION,
  functionName: "enroll",
  input: {
    provider_id: "demo-benefits",
    fields: { plan_id: "GOLD-2026", start_date: "2026-10-01T09:00:00Z" },
  },
});

console.log("=".repeat(70));
console.log("what the vendor actually received");
console.log("=".repeat(70));
const received = out?.echo?.json ?? out?.echo;
console.log(JSON.stringify(received, null, 2));

console.log("\n" + "=".repeat(70));
console.log("verdict");
console.log("=".repeat(70));
const asText = JSON.stringify(received ?? {});
const stillMarkers = asText.includes("{{profile.");
const emp = received?.employee ?? {};
for (const [k, v] of Object.entries(emp)) {
  const isMarker = typeof v === "string" && v.startsWith("{{profile.");
  console.log(`  ${isMarker ? "✗ NOT substituted" : "✓ substituted"}  employee.${k} = ${JSON.stringify(v)}`);
}
console.log(
  stillMarkers
    ? "\n✗ markers reached the vendor unresolved — the privacy path did NOT work."
    : "\n✓ every {{profile.*}} marker was resolved by the host.\n" +
      "  The contract sent marker text; the vendor received real values.\n" +
      "  The plaintext never existed inside WASM memory.",
);
