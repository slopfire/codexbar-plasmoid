// Slot labels such as "Session" promise a window length. When the CLI reports a
// different length — Codex's free plan puts a 30-day allowance in the primary
// slot — the row is named after the length it actually has.
//
// Run: node --test tests/helper-window-titles.test.mjs

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const helperPath = path.join(repoRoot, "plasmoid/contents/code/codexbar-plasmoid-helper.mjs");

function codexUsage(primary, secondary) {
  return JSON.stringify([
    {
      provider: "codex",
      account: "titles@example.com",
      source: "cli",
      version: "0.0.0-test",
      usage: {
        accountEmail: "titles@example.com",
        loginMethod: "free",
        updatedAt: "2026-01-01T00:00:00Z",
        primary,
        ...(secondary ? { secondary } : {}),
      },
    },
  ]);
}

function codexTitles(usageBody) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "codexbar-titles-"));
  const home = path.join(dir, "home");
  fs.mkdirSync(home, { recursive: true });
  const cli = path.join(dir, "codexbar");
  fs.writeFileSync(
    cli,
    `#!/usr/bin/env bash
if [[ "$1" == "usage" ]]; then
  printf '%s\\n' '${usageBody}'
  exit 0
fi
printf '[]\\n'
`,
    { mode: 0o755 },
  );
  const nativeCli = path.join(dir, "codexbar-plasmoid");
  fs.writeFileSync(nativeCli, "#!/usr/bin/env bash\nprintf '[]\\n'\n", { mode: 0o755 });

  const stdout = execFileSync(
    process.execPath,
    [helperPath, "--cli", cli, "--nativeCli", nativeCli, "--provider", "codex",
      "--source", "cli", "--timeout", "5", "--retry-attempts", "1"],
    {
      encoding: "utf8",
      timeout: 30_000,
      env: {
        ...process.env,
        HOME: home,
        XDG_CACHE_HOME: path.join(dir, "cache"),
        XDG_CONFIG_HOME: path.join(home, ".config"),
        CODEX_HOME: path.join(home, ".codex"),
      },
    },
  );
  const entry = (JSON.parse(stdout).entries || []).find((item) => item.provider === "codex");
  return entry.rows.map((row) => row.title);
}

test("a 30-day primary window is Monthly, not Session", () => {
  const titles = codexTitles(codexUsage(
    { usedPercent: 1, resetsAt: "2026-01-31T00:00:00Z", windowMinutes: 43200 },
  ));
  assert.deepEqual(titles, ["Monthly"]);
});

test("paid-plan windows keep their slot labels", () => {
  const titles = codexTitles(codexUsage(
    { usedPercent: 37, resetsAt: "2026-01-01T05:00:00Z", windowMinutes: 300 },
    { usedPercent: 10, resetsAt: "2026-01-07T00:00:00Z", windowMinutes: 10080 },
  ));
  assert.deepEqual(titles, ["Session", "Weekly"]);
});

test("a window with no reported length keeps its slot label", () => {
  const titles = codexTitles(codexUsage(
    { usedPercent: 37, resetsAt: "2026-01-01T05:00:00Z" },
  ));
  assert.deepEqual(titles, ["Session"]);
});
