/**
 * T3N ADK — authenticated connection, with an egress probe.
 *
 * The published SDK bundle is obfuscated, so its outbound hosts cannot be
 * determined statically. We wrap global fetch BEFORE importing the SDK and
 * log every distinct host it contacts.
 */
const seen = new Map<string, number>();
const origFetch = globalThis.fetch;
globalThis.fetch = async (input: any, init?: any) => {
  const raw = typeof input === "string" ? input : (input?.url ?? String(input));
  try {
    const u = new URL(raw);
    const key = `${u.protocol}//${u.host}`;
    seen.set(key, (seen.get(key) ?? 0) + 1);
  } catch { /* non-URL input */ }
  return origFetch(input as any, init);
};

// Dynamic import so the hook is installed before any SDK module code runs.
const {
  T3nClient,
  setEnvironment,
  loadWasmComponent,
  fetchTrustedManifest,
  eth_get_address,
  metamask_sign,
  createEthAuthInput,
} = await import("@terminal3/t3n-sdk");

setEnvironment("testnet");

const T3N_API_KEY = process.env.T3N_API_KEY;
if (!T3N_API_KEY) throw new Error("T3N_API_KEY not exported into this shell");

const wasmComponent = await loadWasmComponent();
const address = eth_get_address(T3N_API_KEY);
console.log("Derived address:", address.slice(0, 8) + "…" + address.slice(-4));

const t3n = new T3nClient({
  trustAnchor: await fetchTrustedManifest("testnet"),
  wasmComponent,
  handlers: { EthSign: metamask_sign(address, undefined, T3N_API_KEY) },
});

await t3n.handshake();
const did = await t3n.authenticate(createEthAuthInput(address));
const tenantDid = did.value;
console.log("Connected as:", tenantDid);

console.log("\n--- egress observed ---");
for (const [host, n] of [...seen].sort((a, b) => b[1] - a[1])) {
  console.log(`  ${n.toString().padStart(3)}x  ${host}`);
}
