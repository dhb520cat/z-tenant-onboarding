/**
 * Re-point the maps' ACLs at the current contract id.
 *
 * Registering a new version of a contract mints a NEW numeric contract_id.
 * Map ACLs are keyed on that id, so after a re-register the contract can no
 * longer read the maps it created — the failure surfaces at call time as:
 *
 *   kv_store.get on 'z:<tid>:onboarding-providers' read denied:
 *   access denied: TenantContract(...)
 *
 * which points at the KV layer, not at the re-registration that caused it.
 * See findings/BUGS.md BUG-13.
 *
 * Both ids are kept in the ACL so calls pinned to the previous version keep
 * working during a rollout.
 *
 * Run:  set -a && . ../.secrets/t3n.env && set +a && npx tsx fix-map-acl.ts <id>...
 */

const {
  T3nClient, TenantClient, setEnvironment, getNodeUrl, loadWasmComponent,
  eth_get_address, metamask_sign, createEthAuthInput,
} = await import("@terminal3/t3n-sdk");

setEnvironment("testnet");

const T3N_API_KEY = process.env.T3N_API_KEY;
if (!T3N_API_KEY) throw new Error("T3N_API_KEY is not exported into this shell");

const ids = process.argv.slice(2).map(Number).filter((n) => Number.isFinite(n));
if (ids.length === 0) throw new Error("usage: npx tsx fix-map-acl.ts <contractId>...");

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

for (const tail of ["onboarding-providers", "secrets"]) {
  await tenant.maps.update(tail, {
    writers: { only: ids },
    readers: { only: ids },
  });
  console.log(`ACL updated: ${tail} → contracts [${ids.join(", ")}]`);
}
