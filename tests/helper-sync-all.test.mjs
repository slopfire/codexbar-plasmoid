// End-to-end test for "Sync all from server" mode.
//
// With --sync-all true the helper must render the server's merged
// cross-device state for every provider and skip local fetching entirely:
// no CLI invocation, no cost scan, and no publish (nothing local exists to
// publish). This is the thin-device path — a widget with no provider
// credentials still shows usage.
//
// Run: node --test tests/helper-sync-all.test.mjs

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const helperPath = path.join(repoRoot, "plasmoid/contents/code/codexbar-plasmoid-helper.mjs");

const SERVER_ENTRIES = [
  {
    provider: "codex",
    account: "server@example.com",
    source: "remote",
    updatedAt: "2026-01-02T00:00:00Z",
    rows: [{ id: "primary", title: "Session", percentLeft: 42, windowMinutes: 300 }],
    dailyUsage: [{ date: "2026-01-01", totalTokens: 1000, costUSD: 0.5, models: [] }],
  },
  {
    provider: "claude",
    account: "server@example.com",
    source: "remote",
    updatedAt: "2026-01-02T00:00:00Z",
    rows: [{ id: "primary", title: "Session", percentLeft: 61, windowMinutes: 300 }],
  },
];

/**
 * A minimal sync server that records requests and answers /v1/usage with the
 * supplied entries. PUT /v1/snapshot/* is recorded so a publish would be
 * visible in the test.
 */
async function startServer({ token = "secret", entries = SERVER_ENTRIES } = {}) {
  const requests = [];
  const server = http.createServer((req, res) => {
    const authorized = req.headers.authorization === `Bearer ${token}`;
    requests.push({ method: req.method, url: req.url, authorized });
    if (req.method === "GET" && req.url.startsWith("/v1/usage")) {
      if (!authorized) {
        res.writeHead(401, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ ok: false, error: "unauthorized" }));
        return;
      }
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ ok: true, entries, requestedProvider: "all" }));
      return;
    }
    res.writeHead(404, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ ok: false }));
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  return {
    baseUrl: `http://127.0.0.1:${port}`,
    requests,
    close: () => new Promise((resolve) => server.close(resolve)),
  };
}

function makeFixture() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "codexbar-sync-all-"));
  const home = path.join(dir, "home");
  fs.mkdirSync(home, { recursive: true });
  const marker = path.join(dir, "cli-called");
  const cli = path.join(dir, "codexbar");
  fs.writeFileSync(
    cli,
    `#!/usr/bin/env bash\nprintf 'called\\n' >> "${marker}"\nprintf '[]\\n'\n`,
    { mode: 0o755 },
  );
  return { dir, home, marker, cli };
}

function runHelper(fixture, extraArgs) {
  const env = {
    ...process.env,
    HOME: fixture.home,
    XDG_CACHE_HOME: path.join(fixture.home, ".cache"),
    XDG_CONFIG_HOME: path.join(fixture.home, ".config"),
    PATH: process.env.PATH || "",
  };
  // spawn (not execFileSync): the parent keeps its event loop free so the
  // in-process mock sync server can answer the helper's HTTP request.
  return new Promise((resolve, reject) => {
    const child = spawn(
      process.execPath,
      [
        helperPath,
        "--cli",
        fixture.cli,
        "--nativeCli",
        fixture.cli,
        "--provider",
        "all",
        "--source",
        "auto",
        "--timeout",
        "5",
        ...extraArgs,
      ],
      { env },
    );
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => { stdout += chunk; });
    child.stderr.on("data", (chunk) => { stderr += chunk; });
    child.on("error", reject);
    child.on("close", (code) => {
      if (code !== 0) {
        reject(new Error(`helper exited ${code}: ${stderr}`));
        return;
      }
      try {
        resolve(JSON.parse(stdout));
      } catch (error) {
        reject(new Error(`helper JSON parse failed: ${error.message}: ${stdout.slice(0, 300)}`));
      }
    });
  });
}

test("sync-all renders every server provider and never touches the CLI", async () => {
  const fixture = makeFixture();
  const server = await startServer();
  try {
    const snapshot = await runHelper(fixture, [
      "--sync-all",
      "true",
      "--sync-url",
      server.baseUrl,
      "--sync-token",
      "secret",
    ]);

    assert.equal(snapshot.ok, true);
    const providers = snapshot.entries.map((entry) => entry.provider).sort();
    assert.deepEqual(providers, ["claude", "codex"]);

    const codex = snapshot.entries.find((entry) => entry.provider === "codex");
    assert.equal(codex.rows[0].percentLeft, 42);
    assert.equal(codex.account, "server@example.com");

    // The server answered over GET /v1/usage with no provider filter.
    const usageRequests = server.requests.filter((request) => request.url.startsWith("/v1/usage"));
    assert.equal(usageRequests.length, 1);
    assert.equal(usageRequests[0].method, "GET");
    assert.equal(usageRequests[0].url, "/v1/usage");
    assert.equal(usageRequests[0].authorized, true);

    // No publish: server-state mode owns no local entries.
    assert.equal(server.requests.filter((request) => request.method === "PUT").length, 0);

    // No local fetching: the CLI was never invoked.
    assert.equal(fs.existsSync(fixture.marker), false);
  } finally {
    await server.close();
  }
});

test("sync-all treats a missing server URL as an error card, not a local fetch", async () => {
  const fixture = makeFixture();
  const snapshot = await runHelper(fixture, ["--sync-all", "true"]);

  assert.equal(snapshot.ok, true);
  assert.ok(snapshot.entries.length > 0);
  for (const entry of snapshot.entries) {
    assert.match(entry.error.message, /Sync server URL missing/);
  }
  assert.equal(fs.existsSync(fixture.marker), false);
});

test("sync-all reports an unpublished server instead of an empty widget", async () => {
  const fixture = makeFixture();
  const server = await startServer({ entries: [] });
  try {
    const snapshot = await runHelper(fixture, [
      "--sync-all",
      "true",
      "--sync-url",
      server.baseUrl,
      "--sync-token",
      "secret",
    ]);
    assert.equal(snapshot.ok, true);
    assert.ok(snapshot.entries.length > 0);
    assert.match(snapshot.entries[0].error.message, /Nothing published/);
  } finally {
    await server.close();
  }
});
