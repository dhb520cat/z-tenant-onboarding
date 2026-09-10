/**
 * Exercise the deployed z-tenant-onboarding contract.
 *
 * Three checks, in increasing order of what they prove:
 *   1. list-providers  — the contract loads, runs in the TEE, and reads its
 *                        own KV map. No user context or egress needed.
 *   2. enroll (guard)  — the PII guard rejects personal data in `fields`
 *                        before any network work happens.
 *   3. enroll (real)   — the full placeholder path. Needs a user context and
 *                        an egress grant; we report exactly what comes back.
 *
 * Run:  set -a && . ../.secrets/t3n.env && set +a && npx tsx invoke.ts
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

console.warn("!!! TEE attestation verification DISABLED — cluster manifest broken (BUG-10)\n");

const address = eth_get_address(T3N_API_KEY);
const t3n = new T3nClient({
  trustAnchor: { unsafe_trust_server: true } as any,
  wasmComponent: await loadWasmComponent(),
  handlers: { EthSign: metamask_sign(address, undefined, T3N_API_KEY) },
});
await t3n.handshake();
const tenantDid = (await t3n.authenticate(createEthAuthInput(address))).value;
const tenant: any = new TenantClient({ t3n, baseUrl: getNodeUrl(), tenantDid } as any);
await tenant.tenant.me();

const show = (label: string, v: unknown) => {
  const s = typeof v === "string" ? v : JSON.stringify(v, null, 2);
  console.log(`${label}\n${s}\n`);
};

// ── 1. list-providers ───────────────────────────────────────────────────────
console.log("=".repeat(70));
console.log("1. list-providers — contract loads, runs, reads its own KV map");
console.log("=".repeat(70));
try {
  const out = await tenant.contracts.execute(TAIL, {
    version: VERSION,
    functionName: "list-providers",
    input: {},
  });
  show("PASS:", out);
} catch (e: any) {
  show("FAIL:", String(e?.message ?? e).slice(0, 400));
}

// ── 2. enroll — PII guard ───────────────────────────────────────────────────
console.log("=".repeat(70));
console.log("2. enroll — PII guard must reject personal data in `fields`");
console.log("=".repeat(70));
try {
  const out = await tenant.contracts.execute(TAIL, {
    version: VERSION,
    functionName: "enroll",
    input: {
      provider_id: "demo-benefits",
      // Deliberately smuggling PII through the one caller-writable channel.
      fields: { plan_id: "GOLD-2026", employee_email: "jane.smith@example.com" },
    },
  });
  show("UNEXPECTED — guard did not fire:", out);
} catch (e: any) {
  const msg = String(e?.message ?? e);
  const fired = msg.includes("personal data");
  show(fired ? "PASS: guard rejected it —" : "FAIL:", msg.slice(0, 400));
}

// ── 3. enroll — real placeholder dispatch ───────────────────────────────────
console.log("=".repeat(70));
console.log("3. enroll — full placeholder path (needs user context + egress grant)");
console.log("=".repeat(70));
try {
  const out = await tenant.contracts.execute(TAIL, {
    version: VERSION,
    functionName: "enroll",
    input: {
      provider_id: "demo-benefits",
      fields: { plan_id: "GOLD-2026", start_date: "2026-10-01T09:00:00Z" },
    },
  });
  show("RESULT:", out);
} catch (e: any) {
  show("RESULT (expected to need a grant):", String(e?.message ?? e).slice(0, 500));
}

// ── contract logs ───────────────────────────────────────────────────────────
console.log("=".repeat(70));
console.log("contract logs (emitted from inside the TEE)");
console.log("=".repeat(70));
try {
  const logs = await tenant.contracts.logs(TAIL, { limit: 20 });
  if (!logs?.entries?.length) {
    console.log("(none — contract logging is off by default: log_max_entries quota is 0)");
  } else {
    for (const e of logs.entries) console.log(`  [${e.level}] ${e.message}`);
  }
} catch (e: any) {
  console.log("(unavailable:", String(e?.message ?? e).slice(0, 160) + ")");
}
