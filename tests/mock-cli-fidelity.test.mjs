// The mock CLI in scripts/setup-mock-cli.sh must serialize the same field
// names the real producers use, or mock-driven chart checks pass on data the
// widget never sees. Two real shapes are pinned here:
//
//   - `openaiDashboard` is a top-level sibling of `usage` (CodexBar
//     ProviderPayload), and its days are
//     { day, services: [{ service, creditsUsed }], totalCreditsUsed }.
//   - `cost.daily` entries are { date, totalCost, totalTokens, modelBreakdowns }
//     with model rows { modelName, cost, totalTokens }.
//
// Both were wrong once: the dashboard sat under `usage` behind `date`/`costUSD`
// keys and the cost rows used `costUSD`/`name`, so the mock's history chart
// rendered zero-cost days while every check stayed green.
//
// Run: node --test tests/mock-cli-fidelity.test.mjs

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const helperPath = path.join(repoRoot, "plasmoid/contents/code/codexbar-plasmoid-helper.mjs");
const setupMockPath = path.join(repoRoot, "scripts/setup-mock-cli.sh");

function installMock() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "codexbar-mockcli-"));
  const home = path.join(dir, "home");
  const cache = path.join(dir, "cache");
  const mockDir = path.join(dir, "mock");
  fs.mkdirSync(home, { recursive: true });
  fs.mkdirSync(cache, { recursive: true });
  execFileSync("bash", [setupMockPath], {
    encoding: "utf8",
    env: { ...process.env, CODEXBAR_MOCK_DIR: mockDir },
  });
  return { dir, home, cache, cli: path.join(mockDir, "codexbar") };
}

function runHelper(mock, provider) {
  // No cache flags: both cache windows default to 0, so no detached worker is
  // spawned and the run is deterministic.
  const stdout = execFileSync(
    process.execPath,
    [
      helperPath,
      "--cli",
      mock.cli,
      "--nativeCli",
      mock.cli,
      "--provider",
      provider,
      "--source",
      "cli",
      "--timeout",
      "5",
      "--retry-attempts",
      "1",
    ],
    {
      encoding: "utf8",
      env: {
        ...process.env,
        HOME: mock.home,
        XDG_CACHE_HOME: mock.cache,
        XDG_CONFIG_HOME: path.join(mock.home, ".config"),
        CODEX_HOME: path.join(mock.home, ".codex"),
      },
      timeout: 30_000,
    },
  );
  return JSON.parse(stdout);
}

function entryFor(snapshot, provider) {
  const entry = (snapshot.entries || []).find((item) => item.provider === provider);
  assert.ok(entry, `no ${provider} entry in the snapshot`);
  assert.equal(entry.error, null, `${provider} entry carries an error`);
  return entry;
}

function costDays(entry) {
  return (entry.dailyUsage || []).filter((day) => Number(day.costUSD) > 0);
}

test("the mock's openaiDashboard reaches the helper as cloud cost history", () => {
  const mock = installMock();
  const entry = entryFor(runHelper(mock, "codex"), "codex");

  // codeReviewRemainingPercent lives on the dashboard snapshot only; seeing it
  // means `openaiDashboard` was found at the provider-entry top level.
  assert.equal(entry.codeReviewRemainingPercent, 91);

  const days = costDays(entry);
  assert.equal(days.length, 2, "the two dashboard days must survive as cost days");
  assert.deepEqual(days.map((day) => day.costUSD), [1.1, 2.45]);
  for (const day of days) {
    assert.equal(day.historySource, "cloud");
  }
});

test("the mock's cost.daily rows reach the helper as local cost history", () => {
  const mock = installMock();
  const entry = entryFor(runHelper(mock, "claude"), "claude");

  const days = costDays(entry);
  assert.equal(days.length, 1, "the single cost day must carry its totalCost");
  assert.equal(days[0].costUSD, 0.8);
  assert.equal(days[0].historySource, "local");
  assert.deepEqual(days[0].models, [
    { name: "claude-mock", costUSD: 0.8, totalTokens: 40000 },
  ]);
});
