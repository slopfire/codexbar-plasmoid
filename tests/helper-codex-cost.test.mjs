// Codex cost goes to the bundled Rust binary first, with one upstream
// `codexbar cost --provider codex` attempt as fallback.
//
// Upstream `codexbar cost` walks Codex history in Swift and SIGSEGVs on Linux,
// so the helper must never send Codex history through the upstream batch scan:
// NATIVE_COST_PROVIDERS owns codex, CODEXBAR_COST_PROVIDERS is claude-only,
// and a missing-or-all-errors native result triggers exactly one upstream
// attempt rather than going blank. A cost failure (including an upstream
// SIGSEGV) surfaces as `costError` without dropping usage entries.
//
// Run: node --test tests/helper-codex-cost.test.mjs

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const helperPath = path.join(repoRoot, "plasmoid/contents/code/codexbar-plasmoid-helper.mjs");

const GOOD_USAGE = JSON.stringify([
  {
    provider: "codex",
    account: "codex-cost@example.com",
    source: "cli",
    version: "0.0.0-test",
    usage: {
      accountEmail: "codex-cost@example.com",
      loginMethod: "plus",
      updatedAt: "2026-01-01T00:00:00Z",
      primary: { usedPercent: 37, resetsAt: "2026-01-01T05:00:00Z", windowMinutes: 300 },
      codexResetCredits: { availableCount: 1, items: [], updatedAt: "2026-01-01T00:00:00Z" },
    },
  },
]);

const NATIVE_COST = JSON.stringify([
  {
    provider: "codex",
    source: "local",
    sessionCostUSD: 7.77,
    sessionTokens: 777000,
    last30DaysCostUSD: 70.7,
    last30DaysTokens: 7000000,
    currencyCode: "USD",
    sessionLabel: "Today",
    last30DaysLabel: "30d",
    updatedAt: "2026-01-01T00:00:00Z",
    provenance: "listPriceEstimate",
  },
]);

const UPSTREAM_COST = JSON.stringify([
  {
    provider: "codex",
    sessionCostUSD: 2.5,
    sessionTokens: 1000,
    last30DaysCostUSD: 20,
    last30DaysTokens: 5000,
    currencyCode: "USD",
    updatedAt: "2026-01-01T00:00:00Z",
  },
]);

const NATIVE_COST_ERROR = JSON.stringify([
  { provider: "cost", error: { message: "No Codex token usage found under /none." } },
]);

function makeFixture({ nativeCostBody, upstreamCostBody }) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "codexbar-codexcost-"));
  const home = path.join(dir, "home");
  const cache = path.join(dir, "cache");
  fs.mkdirSync(home, { recursive: true });
  fs.mkdirSync(cache, { recursive: true });

  const nativeCalls = path.join(dir, "native-calls");
  const upstreamCalls = path.join(dir, "upstream-calls");
  fs.writeFileSync(nativeCalls, "");
  fs.writeFileSync(upstreamCalls, "");

  // Fake Rust binary: answers `cost` with the scenario payload and logs every
  // invocation plus the $CODEX_HOME it saw (env passthrough check).
  const nativeCli = path.join(dir, "codexbar-plasmoid");
  fs.writeFileSync(
    nativeCli,
    `#!/usr/bin/env bash
set -euo pipefail
echo "cost $* CODEX_HOME=\${CODEX_HOME:-<unset>}" >> "${nativeCalls}"
${nativeCostBody}
`,
    { mode: 0o755 },
  );

  // Fake upstream CLI: answers `usage` with good Codex limits, `cost` with the
  // scenario body, and logs every cost invocation for call-count assertions.
  const cli = path.join(dir, "codexbar");
  fs.writeFileSync(
    cli,
    `#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == "usage" ]]; then
  printf '%s\\n' '${GOOD_USAGE}'
  exit 0
fi
if [[ "$1" == "cost" ]]; then
  echo "cost $*" >> "${upstreamCalls}"
  ${upstreamCostBody}
fi
printf '[]\\n'
`,
    { mode: 0o755 },
  );

  return { dir, home, cache, nativeCli, cli, nativeCalls, upstreamCalls };
}

function runHelper(fixture, provider = "codex") {
  // No cache flags: both cache windows default to 0, so sharedFetch runs the
  // cost producer synchronously inline and no detached worker is spawned.
  const env = {
    ...process.env,
    HOME: fixture.home,
    XDG_CACHE_HOME: fixture.cache,
    XDG_CONFIG_HOME: path.join(fixture.home, ".config"),
    CODEX_HOME: path.join(fixture.home, ".codex"),
    PATH: process.env.PATH || "",
  };
  const stdout = execFileSync(
    process.execPath,
    [
      helperPath,
      "--cli",
      fixture.cli,
      "--nativeCli",
      fixture.nativeCli,
      "--provider",
      provider,
      "--source",
      "cli",
      "--timeout",
      "5",
      "--retry-attempts",
      "1",
    ],
    { encoding: "utf8", env, timeout: 30_000 },
  );
  return JSON.parse(stdout);
}

function costCalls(callsFile) {
  const raw = fs.readFileSync(callsFile, "utf8").trim();
  if (!raw) {
    return [];
  }
  return raw.split("\n").filter((line) => line.startsWith("cost "));
}

function codexEntry(snapshot) {
  return (snapshot.entries || []).find((entry) => entry.provider === "codex");
}

test("codex cost is scanned by the native binary, upstream is never hit", () => {
  const fixture = makeFixture({
    nativeCostBody: `printf '%s\\n' '${NATIVE_COST}'`,
    // Upstream would answer too: the call counts below prove it is not asked.
    upstreamCostBody: `printf '%s\\n' '${UPSTREAM_COST}'; exit 0`,
  });

  const snapshot = runHelper(fixture);
  const entry = codexEntry(snapshot);
  assert.equal(entry.error, null);
  assert.equal(entry.tokenUsage.sessionCostUSD, 7.77);
  assert.equal(entry.tokenUsage.last30DaysCostUSD, 70.7);
  assert.equal(entry.tokenUsage.provenance, "listPriceEstimate");
  assert.equal(snapshot.costError, null);

  const native = costCalls(fixture.nativeCalls);
  assert.equal(native.length, 1, `native binary must answer the codex scan: ${native}`);
  assert.match(native[0], /--provider codex/);
  assert.match(
    native[0],
    new RegExp(`CODEX_HOME=${fixture.home.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}/\\.codex`),
    "$CODEX_HOME must reach the native scan",
  );
  assert.equal(costCalls(fixture.upstreamCalls).length, 0, "no upstream cost scan for codex");
});

test("an all-errors native scan falls back to exactly one upstream attempt", () => {
  const fixture = makeFixture({
    nativeCostBody: `printf '%s\\n' '${NATIVE_COST_ERROR}'`,
    upstreamCostBody: `printf '%s\\n' '${UPSTREAM_COST}'; exit 0`,
  });

  const snapshot = runHelper(fixture);
  const entry = codexEntry(snapshot);
  assert.equal(entry.error, null, "usage must survive the native failure");
  assert.equal(entry.tokenUsage.sessionCostUSD, 2.5, "upstream fallback cost applies");
  assert.equal(snapshot.costError, null);

  assert.equal(costCalls(fixture.nativeCalls).length, 1);
  assert.equal(costCalls(fixture.upstreamCalls).length, 1, "exactly one upstream fallback attempt");
  assert.match(costCalls(fixture.upstreamCalls)[0], /--provider codex/, "fallback scans codex only");
});

test("an old native binary with no codex scan (empty result) falls back upstream", () => {
  const fixture = makeFixture({
    nativeCostBody: "printf '[]\\n'",
    upstreamCostBody: `printf '%s\\n' '${UPSTREAM_COST}'; exit 0`,
  });

  const snapshot = runHelper(fixture);
  const entry = codexEntry(snapshot);
  assert.equal(entry.error, null);
  assert.equal(entry.tokenUsage.sessionCostUSD, 2.5, "upstream fallback cost applies");
  assert.equal(snapshot.costError, null);
  assert.equal(costCalls(fixture.upstreamCalls).length, 1, "exactly one upstream fallback attempt");
});

test("an upstream SIGSEGV on fallback is a costError, usage stays", () => {
  const fixture = makeFixture({
    nativeCostBody: `printf '%s\\n' '${NATIVE_COST_ERROR}'`,
    upstreamCostBody: "kill -SEGV $$",
  });

  const snapshot = runHelper(fixture);
  const entry = codexEntry(snapshot);
  assert.ok(entry, "the usage entry must not be dropped");
  assert.equal(entry.error, null);
  assert.equal(entry.rows.length, 1);
  assert.equal(entry.tokenUsage, null, "no cost data anywhere");
  assert.match(snapshot.costError, /SIGSEGV/);
  assert.equal(costCalls(fixture.upstreamCalls).length, 1, "exactly one upstream fallback attempt");
});

const CLAUDE_USAGE = JSON.stringify([
  {
    provider: "claude",
    account: "claude-cost@example.com",
    source: "cli",
    version: "0.0.0-test",
    usage: {
      accountEmail: "claude-cost@example.com",
      loginMethod: "pro",
      updatedAt: "2026-01-01T00:00:00Z",
      primary: { usedPercent: 40, resetsAt: "2026-01-01T05:00:00Z", windowMinutes: 300 },
    },
  },
]);

const CLAUDE_COST = JSON.stringify([
  {
    provider: "claude",
    sessionCostUSD: 0.8,
    sessionTokens: 40000,
    last30DaysCostUSD: 12.0,
    last30DaysTokens: 500000,
    currencyCode: "USD",
    updatedAt: "2026-01-01T00:00:00Z",
  },
]);

test("claude cost stays on its own upstream scan, never batched with codex", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "codexbar-claudecost-"));
  const home = path.join(dir, "home");
  const cache = path.join(dir, "cache");
  fs.mkdirSync(home, { recursive: true });
  fs.mkdirSync(cache, { recursive: true });
  const upstreamCalls = path.join(dir, "upstream-calls");
  fs.writeFileSync(upstreamCalls, "");

  const cli = path.join(dir, "codexbar");
  fs.writeFileSync(
    cli,
    `#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == "usage" ]]; then
  printf '%s\\n' '${CLAUDE_USAGE}'
  exit 0
fi
if [[ "$1" == "cost" ]]; then
  echo "cost $*" >> "${upstreamCalls}"
  printf '%s\\n' '${CLAUDE_COST}'
  exit 0
fi
printf '[]\\n'
`,
    { mode: 0o755 },
  );
  // The native binary must not exist here: claude cost is upstream-only, so a
  // missing native binary must not matter.
  const fixture = { dir, home, cache, nativeCli: path.join(dir, "no-such-native"), cli, upstreamCalls };

  const snapshot = runHelper(fixture, "claude");
  const entry = (snapshot.entries || []).find((e) => e.provider === "claude");
  assert.equal(entry.error, null);
  assert.equal(entry.tokenUsage.sessionCostUSD, 0.8);
  assert.equal(snapshot.costError, null);

  const calls = costCalls(upstreamCalls);
  assert.equal(calls.length, 1);
  assert.match(calls[0], /--provider claude/, "claude keeps its own upstream scan");
  assert.doesNotMatch(calls[0], /codex/, "codex history is never walked upstream");
});
