/** Ask the SDK itself which URL each environment resolves to, and whether its manifest validates. */
const hits: string[] = [];
const origFetch = globalThis.fetch;
globalThis.fetch = async (input: any, init?: any) => {
  const raw = typeof input === "string" ? input : (input?.url ?? String(input));
  hits.push(raw);
  return origFetch(input as any, init);
};

const { setEnvironment, fetchTrustedManifest } = await import("@terminal3/t3n-sdk");

for (const env of ["sandbox", "testnet", "production"] as const) {
  hits.length = 0;
  setEnvironment(env);
  let verdict: string;
  try {
    const a: any = await fetchTrustedManifest(env);
    verdict = `OK  peers=${a.expected_peer_ids?.length} rtmr3=${a.rtmr3_allowlist?.length} rtmr1=${a.rtmr1_allowlist?.length} sev=${a.sev_snp_measurement_allowlist?.length}`;
  } catch (e: any) {
    verdict = "FAIL  " + String(e?.message ?? e).slice(0, 110);
  }
  console.log(`\n[${env}]`);
  console.log("  url:    ", hits[0] ?? "(no fetch observed)");
  console.log("  result: ", verdict);
}
