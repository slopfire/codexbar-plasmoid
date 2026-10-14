// End-to-end test for stale-serving through the real helper process.
//
// A mock CLI answers once, then starts failing with a Codex-style RPC timeout.
// The widget must keep rendering the previous limits with `stale: true` and no
// error, and the cooldown must stop the helper from re-hitting the CLI on the
// next poll. This is the behaviour that replaced "red error card" and the
// "cost-only fallback" for a provider that already had good data.
//
// Run: node --test tests/helper-stale-serving.test.mjs

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
    account: "stale-test@example.com",
    source: "codex-cli",
    version: "0.0.0-test",
    usage: {
      accountEmail: "stale-test@example.com",
      loginMethod: "plus",
      updatedAt: "2026-01-01T00:00:00Z",
      primary: { usedPercent: 37, resetsAt: "2026-01-01T05:00:00Z", windowMinutes: 300 },
      codexResetCredits: { availableCount: 1, items: [], updatedAt: "2026-01-01T00:00:00Z" },
    },
  },
]);

const FAILED_USAGE = JSON.stringify([
  { provider: "codex", source: "cli", error: { message: "Codex RPC timed out waiting for `account/rateLimits/read` reply." } },
]);

function makeFixture() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "codexbar-stale-"));
  const home = path.join(dir, "home");
  const cache = path.join(dir, "cache");
  fs.mkdirSync(home, { recursive: true });
  fs.mkdirSync(cache, { recursive: true });

  const failFlag = path.join(dir, "fail");
  const calls = path.join(dir, "calls");
  fs.writeFileSync(calls, "0");

  const cli = path.join(dir, "codexbar");
  fs.writeFileSync(
    cli,
    `#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == "usage" ]]; then
  count=$(cat "${calls}")
  echo $((count + 1)) > "${calls}"
  if [[ -f "${failFlag}" ]]; then
    printf '%s\\n' '${FAILED_USAGE}'
  else
    printf '%s\\n' '${GOOD_USAGE}'
  fi
  exit 0
fi
if [[ "$1" == "cost" ]]; then
  printf '[]\\n'
  exit 0
fi
printf '[]\\n'
`,
    { mode: 0o755 },
  );

  return { dir, home, cache, failFlag, calls, cli };
}

function runHelper(fixture, extraArgs) {
  const env = {
    ...process.env,
    HOME: fixture.home,
    XDG_CACHE_HOME: fixture.cache,
    XDG_CONFIG_HOME: path.join(fixture.home, ".config"),
    PATH: process.env.PATH || "",
  };
  const stdout = execFileSync(
    process.execPath,
    [
      helperPath,
      "--cli",
      fixture.cli,
      "--nativeCli",
      fixture.cli,
      "--provider",
      "codex",
      "--source",
      "cli",
      "--timeout",
      "5",
      "--cost",
      "false",
      ...extraArgs,
    ],
    { encoding: "utf8", env, timeout: 30_000 },
  );
  return JSON.parse(stdout);
}

function codexEntry(snapshot) {
  return (snapshot.entries || []).find((entry) => entry.provider === "codex");
}

function usageCalls(fixture) {
  return Number(fs.readFileSync(fixture.calls, "utf8").trim());
}

/** Replace the fixture CLI with one that always answers usage with `message`. */
function useUsageErrorCli(fixture, message) {
  const cli = path.join(fixture.dir, "codexbar-error");
  fs.writeFileSync(
    cli,
    `#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == "usage" ]]; then
  printf '%s\\n' '[{"provider":"codex","source":"cli","error":{"message":"${message}"}}]'
  exit 0
fi
printf '[]\\n'
`,
    { mode: 0o755 },
  );
  fixture.cli = cli;
}

/** Cooldown windows recorded in the usage state files, in milliseconds. */
function cooldownDeltas(cacheDir) {
  const dir = path.join(cacheDir, "codexbar-plasmoid", "provider-cache");
  let names;
  try {
    names = fs.readdirSync(dir).filter((name) => name.startsWith("usage-") && name.endsWith(".state.json"));
  } catch {
    return [];
  }
  const deltas = [];
  for (const name of names) {
    try {
      const state = JSON.parse(fs.readFileSync(path.join(dir, name), "utf8"));
      if (state.cooldownUntilMs > 0) {
        deltas.push(state.cooldownUntilMs - state.lastAttemptAt);
      }
    } catch {
      // A half-written state file is not a cooldown.
    }
  }
  return deltas;
}

async function waitFor(predicate, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) {
      return true;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  return predicate();
}

/** Cost payloads the detached worker has written into the shared cache. */
function cachedCostPayloads(cacheDir) {
  const dir = path.join(cacheDir, "codexbar-plasmoid", "provider-cache");
  let names;
  try {
    names = fs.readdirSync(dir).filter((name) => name.startsWith("cost-") && name.endsWith(".json"));
  } catch {
    return [];
  }
  const payloads = [];
  for (const name of names) {
    try {
      const parsed = JSON.parse(fs.readFileSync(path.join(dir, name), "utf8"));
      const value = parsed && parsed.cacheVersion ? parsed.value : parsed;
      if (Array.isArray(value) && value.length > 0) {
        payloads.push(value);
      }
    } catch {
      // A half-written cache file is never a payload.
    }
  }
  return payloads;
}

test("a transient failure keeps the previous limits on screen, tagged stale", async () => {
  const fixture = makeFixture();

  const healthy = runHelper(fixture, ["--cache-seconds", "1", "--cost-cache-seconds", "3600", "--force", "false"]);
  const healthyEntry = codexEntry(healthy);
  assert.equal(healthyEntry.error, null);
  assert.equal(healthyEntry.stale, false);
  assert.equal(healthyEntry.rows[0].percentLeft, 63);
  assert.equal(usageCalls(fixture), 1);

  // Let the good entry fall out of its freshness window, then fail.
  await new Promise((resolve) => setTimeout(resolve, 1100));
  fs.writeFileSync(fixture.failFlag, "1");

  const failed = runHelper(fixture, ["--cache-seconds", "1", "--cost-cache-seconds", "3600", "--force", "true"]);
  const failedEntry = codexEntry(failed);
  assert.equal(failedEntry.error, null, "stale data must not be reported as an error");
  assert.equal(failedEntry.stale, true);
  assert.equal(failedEntry.rows[0].percentLeft, 63, "the last known limits stay visible");
  assert.match(failedEntry.lastAttemptError.message, /timed out/);

  // The cooldown must stop the next automatic poll from hitting the CLI again.
  const callsAfterFailure = usageCalls(fixture);
  const cooled = runHelper(fixture, ["--cache-seconds", "1", "--cost-cache-seconds", "3600", "--force", "false"]);
  assert.equal(codexEntry(cooled).stale, true);
  assert.equal(usageCalls(fixture), callsAfterFailure, "cooldown must not re-hit the provider");
});

test("the configured backoff cap bounds a provider's Retry-After hint", () => {
  const args = [
    "--cache-seconds", "1", "--cost-cache-seconds", "3600", "--force", "false",
    "--retry-attempts", "1",
  ];

  const tight = makeFixture();
  useUsageErrorCli(tight, "429 Too Many Requests, retry after 600");
  runHelper(tight, [...args, "--cooldown-cap-seconds", "30"]);
  const clamped = cooldownDeltas(tight.cache);
  assert.ok(clamped.length > 0, "a failing refresh records a cooldown");
  assert.ok(clamped.every((delta) => delta === 30_000), `a 30s cap must clamp a 10min hint: ${clamped}`);

  const loose = makeFixture();
  useUsageErrorCli(loose, "429 Too Many Requests, retry after 600");
  runHelper(loose, [...args, "--cooldown-cap-seconds", "1800"]);
  const hinted = cooldownDeltas(loose.cache);
  assert.ok(hinted.length > 0);
  assert.ok(hinted.every((delta) => delta === 600_000), `a raised cap honours the hint: ${hinted}`);
});

test("a failure with no cached data still surfaces an actionable error", () => {
  const fixture = makeFixture();
  fs.writeFileSync(fixture.failFlag, "1");

  const snapshot = runHelper(fixture, ["--cache-seconds", "1", "--cost-cache-seconds", "3600", "--force", "false"]);
  const entry = codexEntry(snapshot);
  assert.equal(entry.stale, false);
  assert.ok(entry.error, "no cached payload means the error is what the user needs to see");
  assert.match(entry.error.message, /timed out/);
});

function costOnlyFixture(costBody) {
  const fixture = makeFixture();
  const cli = path.join(fixture.dir, "codexbar-costonly");
  fs.writeFileSync(
    cli,
    `#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == "usage" ]]; then
  printf '%s\\n' '${FAILED_USAGE}'
  exit 0
fi
if [[ "$1" == "cost" ]]; then
  ${costBody}
  printf '%s\\n' '[{"provider":"codex","sessionCostUSD":2.5,"sessionTokens":1000,"last30DaysCostUSD":20,"last30DaysTokens":5000,"provenance":"listPriceEstimate"}]'
  exit 0
fi
printf '[]\\n'
`,
    { mode: 0o755 },
  );
  fixture.cli = cli;
  return fixture;
}

test("local spend without limits explains itself instead of showing a bare card", async () => {
  const fixture = costOnlyFixture(":");
  const args = [
    "--cache-seconds", "1", "--cost-cache-seconds", "3600", "--force", "false", "--cost", "true",
  ];

  // The first render has no cost yet: the scan runs detached.
  runHelper(fixture, args);
  assert.ok(await waitFor(() => cachedCostPayloads(fixture.cache).length > 0, 20_000), "worker should populate the cache");

  const entry = codexEntry(runHelper(fixture, args));
  assert.equal(entry.error, null, "a cost-only card is not an error card");
  assert.ok(entry.usageUnavailable, "the card carries the reason limits are missing");
  assert.match(entry.usageUnavailable.message, /timed out/);
  assert.equal(entry.tokenUsage.sessionCostUSD, 2.5);
});

test("a slow cost scan never blocks the render", async () => {
  const fixture = costOnlyFixture("sleep 8");
  const args = [
    "--cache-seconds", "1", "--cost-cache-seconds", "3600", "--force", "false", "--cost", "true",
  ];

  const started = Date.now();
  const snapshot = runHelper(fixture, args);
  const elapsed = Date.now() - started;
  assert.ok(elapsed < 5000, `render path waited ${elapsed}ms for an 8s cost scan`);
  assert.equal(codexEntry(snapshot).tokenUsage, null, "cost appears on the next refresh, not this one");

  // The detached worker still finishes and lands in the shared cache.
  assert.ok(await waitFor(() => cachedCostPayloads(fixture.cache).length > 0, 30_000));
  const next = runHelper(fixture, args);
  assert.equal(codexEntry(next).tokenUsage.sessionCostUSD, 2.5);
});

test("a busy Codex CLI source falls back to the oauth source", () => {
  const fixture = makeFixture();
  // Fail only the app-server-bridge source; oauth reads the same limits from
  // the backend independently, so the card should stay green and fresh.
  const cli = path.join(fixture.dir, "codexbar-fallback");
  fs.writeFileSync(
    cli,
    `#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == "usage" ]]; then
  if [[ " $* " == *" --source cli "* ]]; then
    printf '%s\\n' '${FAILED_USAGE}'
  else
    printf '%s\\n' '${GOOD_USAGE}'
  fi
  exit 0
fi
printf '[]\\n'
`,
    { mode: 0o755 },
  );
  fixture.cli = cli;

  const snapshot = runHelper(fixture, ["--cache-seconds", "1", "--cost-cache-seconds", "3600", "--force", "false"]);
  const entry = codexEntry(snapshot);
  assert.equal(entry.error, null, "the fallback source answered");
  assert.equal(entry.stale, false);
  assert.equal(entry.rows[0].percentLeft, 63);
});
