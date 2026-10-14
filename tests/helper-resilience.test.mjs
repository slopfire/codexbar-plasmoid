// Unit tests for the helper's resilience layer.
//
// The behaviour under test is what stops a transient provider failure (a busy
// Codex app-server RPC socket, a 429, a slow cost scan) from turning into a red
// error card or a cost-only fallback:
//
//   - transient/auth/unsupported/permanent classification,
//   - retry only where a retry can help,
//   - a two-tier cache that keeps last known good data and cools down,
//   - a failed scan never landing in the good slot for the full cache window.
//
// Run: node --test tests/helper-resilience.test.mjs

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  annotateStaleValue,
  classifyFailureMessage,
  cooldownMsFor,
  DEFAULT_COOLDOWN_CAP_MS,
  failureFromPayload,
  fetchWithResilience,
  jitterMs,
  parseRetryAfterMs,
  runWithRetries,
} from "../plasmoid/contents/code/codexbar-resilience.mjs";

function tempCachePath(label) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), `codexbar-${label}-`));
  return { dir, cachePath: path.join(dir, "usage-key.json") };
}

function clockFrom(start = Date.now()) {
  let current = start;
  return {
    now: () => current,
    advance: (milliseconds) => {
      current += milliseconds;
    },
  };
}

const noSleep = () => {};

test("classifyFailureMessage buckets the failures the widget actually sees", () => {
  assert.equal(
    classifyFailureMessage("Codex RPC timed out waiting for `account/rateLimits/read` reply.").kind,
    "transient",
  );
  assert.equal(classifyFailureMessage("HTTP 429 Too Many Requests").kind, "transient");
  assert.equal(classifyFailureMessage("OpenCode Go is unavailable. Try again later.").kind, "transient");
  assert.equal(classifyFailureMessage("Grok request failed: Method not found").kind, "unsupported");
  assert.equal(
    classifyFailureMessage("Error: selected source requires web support and is only supported on macOS.").kind,
    "unsupported",
  );
  assert.equal(classifyFailureMessage("OpenCode session cookie is invalid or expired.").kind, "auth");
  assert.equal(classifyFailureMessage("CLI not found: codexbar").kind, "permanent");
  assert.equal(classifyFailureMessage("no such table: message").kind, "permanent");
  assert.equal(classifyFailureMessage("").kind, "permanent");
});

test("parseRetryAfterMs reads Retry-After hints in seconds, ms and minutes", () => {
  assert.equal(parseRetryAfterMs("rate limited, retry after 30"), 30_000);
  assert.equal(parseRetryAfterMs("Retry-After: 250ms"), 250);
  assert.equal(parseRetryAfterMs("try again in 2 minutes"), 120_000);
  assert.equal(parseRetryAfterMs("Service busy"), null);
});

test("failureFromPayload only calls a fully errored payload a failure", () => {
  assert.equal(failureFromPayload([]), null);
  assert.equal(failureFromPayload([{ provider: "codex", usage: {} }]), null);
  assert.equal(
    failureFromPayload([{ provider: "codex", error: { message: "boom" } }, { provider: "claude" }]),
    null,
    "one working account means the payload is usable",
  );
  assert.equal(failureFromPayload([{ error: { message: "boom" } }]).kind, "permanent");
  assert.equal(
    failureFromPayload([{ error: { message: "request timed out" } }]).message,
    "request timed out",
  );
});

test("cooldown grows exponentially, caps, and honours Retry-After", () => {
  const options = { baseMs: 1000, capMs: 60_000 };
  assert.equal(cooldownMsFor(1, null, options), 1000);
  assert.equal(cooldownMsFor(2, null, options), 2000);
  assert.equal(cooldownMsFor(3, null, options), 4000);
  assert.equal(cooldownMsFor(20, null, options), 60_000);
  assert.equal(cooldownMsFor(1, { retryAfterMs: 45_000 }, options), 45_000);
  assert.equal(cooldownMsFor(1, { retryAfterMs: 5 }, options), 1000, "never below the base");
  // The widget default is three minutes, and a cap below the base still wins.
  assert.equal(DEFAULT_COOLDOWN_CAP_MS, 180_000);
  assert.equal(cooldownMsFor(5), 180_000);
  assert.equal(cooldownMsFor(5, null, { baseMs: 30_000, capMs: 10_000 }), 10_000);
});

test("jitterMs stays inside its band and never goes negative", () => {
  for (let index = 0; index < 50; index += 1) {
    const value = jitterMs(1000, 0.25);
    assert.ok(value >= 750 && value <= 1250, `jitter out of band: ${value}`);
  }
  assert.equal(jitterMs(0), 0);
});

test("annotateStaleValue tags a copy and leaves the cached payload alone", () => {
  const cached = [{ provider: "codex", usage: { primary: { usedPercent: 30 } } }];
  const annotated = annotateStaleValue(cached, {
    since: 1234,
    error: { message: "request timed out", kind: "transient" },
  });
  assert.equal(cached[0].stale, undefined, "cached payload must not be mutated");
  assert.equal(annotated[0].stale, true);
  assert.equal(annotated[0].staleSince, 1234);
  assert.deepEqual(annotated[0].lastAttemptError, { message: "request timed out", kind: "transient" });
  assert.deepEqual(annotated[0].usage, cached[0].usage);
});

test("fetchWithResilience reuses a fresh success without calling the producer", () => {
  const { cachePath } = tempCachePath("fresh");
  const clock = clockFrom();
  let calls = 0;
  const options = {
    cachePath,
    cacheSeconds: 300,
    now: clock.now,
    sleep: noSleep,
    producer: () => {
      calls += 1;
      return [{ provider: "codex", usage: {} }];
    },
  };

  const first = fetchWithResilience(options);
  assert.equal(calls, 1);
  const second = fetchWithResilience(options);
  assert.equal(calls, 1, "still inside the freshness window");
  assert.deepEqual(second, first);

  clock.advance(301_000);
  fetchWithResilience(options);
  assert.equal(calls, 2, "expired entry refreshes");
});

test("a failed refresh keeps the last good payload and opens a cooldown", () => {
  const { cachePath } = tempCachePath("stale");
  const clock = clockFrom();
  const good = [{ provider: "codex", usage: { primary: { usedPercent: 30 } } }];
  const bad = [{ provider: "codex", error: { message: "Codex RPC timed out" } }];
  let calls = 0;
  const options = {
    cachePath,
    cacheSeconds: 150,
    cooldownBaseMs: 1000,
    now: clock.now,
    sleep: noSleep,
    producer: () => {
      calls += 1;
      return calls === 1 ? good : bad;
    },
  };

  assert.deepEqual(fetchWithResilience(options), good);

  clock.advance(200_000);
  const stale = fetchWithResilience(options);
  assert.equal(calls, 2);
  assert.equal(stale[0].stale, true, "failure serves the previous good payload");
  assert.equal(stale[0].lastAttemptError.message, "Codex RPC timed out");
  assert.equal(stale[0].usage.primary.usedPercent, 30);

  clock.advance(500);
  const cooling = fetchWithResilience(options);
  assert.equal(calls, 2, "cooldown must not hit the provider again");
  assert.equal(cooling[0].stale, true);

  clock.advance(60_000);
  const retried = fetchWithResilience(options);
  assert.equal(calls, 3, "after the cooldown the provider is retried");
  assert.equal(retried[0].stale, true);
});

test("a failure with no good payload is negative-cached only for the cooldown", () => {
  const { cachePath } = tempCachePath("empty");
  const clock = clockFrom();
  const bad = [{ provider: "cost", error: { message: "CodexBar CLI timed out after 45 seconds" } }];
  let calls = 0;
  const options = {
    cachePath,
    cacheSeconds: 3600,
    cooldownBaseMs: 5000,
    now: clock.now,
    sleep: noSleep,
    producer: () => {
      calls += 1;
      return bad;
    },
  };

  const first = fetchWithResilience(options);
  assert.equal(calls, 1);
  assert.equal(first[0].error.message, "CodexBar CLI timed out after 45 seconds");

  const duringCooldown = fetchWithResilience(options);
  assert.equal(calls, 1, "the raw failure is replayed from state, not the hour-long good slot");
  assert.equal(duringCooldown[0].error.message, "CodexBar CLI timed out after 45 seconds");

  clock.advance(5001);
  fetchWithResilience(options);
  assert.equal(calls, 2, "a short cooldown retries long before the 1h cache window");
});

test("a success clears the cooldown so the next failure starts over", () => {
  const { cachePath } = tempCachePath("recover");
  const clock = clockFrom();
  let mode = "good";
  let calls = 0;
  const options = {
    cachePath,
    cacheSeconds: 100,
    cooldownBaseMs: 10_000,
    now: clock.now,
    sleep: noSleep,
    producer: () => {
      calls += 1;
      return mode === "good"
        ? [{ provider: "codex", usage: {} }]
        : [{ provider: "codex", error: { message: "429 too many requests" } }];
    },
  };

  fetchWithResilience(options);
  clock.advance(101_000);
  mode = "bad";
  assert.equal(fetchWithResilience(options)[0].stale, true);
  assert.equal(calls, 2);

  // Recover, then fail again: the new failure must start at the base cooldown.
  clock.advance(10_001);
  mode = "good";
  assert.equal(fetchWithResilience(options)[0].stale, undefined);
  clock.advance(101_000);
  mode = "bad";
  assert.equal(fetchWithResilience(options)[0].stale, true);
  const stateFile = `${cachePath}.state.json`;
  const state = JSON.parse(fs.readFileSync(stateFile, "utf8"));
  assert.equal(state.failures, 1, "the successful refresh reset the failure count");
});

test("legacy error cache entries are dropped instead of replayed", () => {
  const { cachePath } = tempCachePath("legacy");
  fs.writeFileSync(
    cachePath,
    JSON.stringify([{ provider: "cost", error: { message: "CodexBar CLI timed out after 45 seconds" } }]),
  );
  let calls = 0;
  const value = fetchWithResilience({
    cachePath,
    cacheSeconds: 3600,
    now: () => Date.now(),
    sleep: noSleep,
    producer: () => {
      calls += 1;
      return [{ provider: "cost", sessionCostUSD: 1.25 }];
    },
  });
  assert.equal(calls, 1, "a poisoned cache must not be served");
  assert.equal(value[0].sessionCostUSD, 1.25);
});

test("forceRefresh ignores freshness and cooldown", () => {
  const { cachePath } = tempCachePath("force");
  const clock = clockFrom();
  let calls = 0;
  const options = {
    cachePath,
    cacheSeconds: 3600,
    cooldownBaseMs: 60_000,
    now: clock.now,
    sleep: noSleep,
    producer: () => {
      calls += 1;
      return [{ provider: "codex", error: { message: "timed out" } }];
    },
  };
  fetchWithResilience(options);
  fetchWithResilience({ ...options, forceRefresh: true, requestStartedAt: clock.now() });
  assert.equal(calls, 2, "a manual refresh always asks the provider");
});

test("revalidate serves the cache and asks a worker to refresh it", () => {
  const { cachePath } = tempCachePath("revalidate");
  const clock = clockFrom();
  const good = [{ provider: "codex", sessionCostUSD: 2.5 }];
  let spawns = 0;
  const options = {
    cachePath,
    cacheSeconds: 3600,
    now: clock.now,
    sleep: noSleep,
    revalidate: { spawn: () => { spawns += 1; } },
    producer: () => good,
  };

  // Nothing cached yet: the render gets an empty value and a worker starts.
  assert.deepEqual(fetchWithResilience(options), []);
  assert.equal(spawns, 1);

  // A worker result lands in the cache and is served fresh without spawning.
  fetchWithResilience({ ...options, revalidate: null, now: clock.now, sleep: noSleep });
  assert.deepEqual(fetchWithResilience(options), good);
  assert.equal(spawns, 1, "a fresh cache needs no worker");

  // Once stale, the last good value stays on screen while a worker refreshes.
  clock.advance(3_601_000);
  const stale = fetchWithResilience(options);
  assert.equal(spawns, 2);
  assert.equal(stale[0].stale, true);
  assert.equal(stale[0].sessionCostUSD, 2.5);

  // A cooldown suppresses the worker but keeps serving the stale value.
  fs.writeFileSync(`${cachePath}.state.json`, JSON.stringify({
    failures: 2,
    lastAttemptAt: clock.now(),
    lastError: { message: "timed out", kind: "transient" },
    lastFailureValue: null,
    cooldownUntilMs: clock.now() + 60_000,
  }));
  clock.advance(1);
  const cooled = fetchWithResilience(options);
  assert.equal(spawns, 2, "cooldown must not keep spawning workers");
  assert.equal(cooled[0].stale, true);
  assert.equal(cooled[0].lastAttemptError.message, "timed out");
});

test("runWithRetries retries transient failures and stops on permanent ones", () => {
  let attempts = 0;
  const transientThenGood = runWithRetries(() => {
    attempts += 1;
    return attempts === 1
      ? [{ error: { message: "Codex RPC timed out waiting for `account/rateLimits/read` reply." } }]
      : [{ provider: "codex", usage: {} }];
  }, { attempts: 2, timeoutMs: 45_000, startedAt: Date.now(), sleep: noSleep });
  assert.equal(attempts, 2);
  assert.equal(transientThenGood.error, null);
  assert.equal(transientThenGood.value[0].usage !== undefined, true);

  let permanentAttempts = 0;
  const permanent = runWithRetries(() => {
    permanentAttempts += 1;
    return [{ error: { message: "CLI not found: codexbar" } }];
  }, { attempts: 3, timeoutMs: 45_000, startedAt: Date.now(), sleep: noSleep });
  assert.equal(permanentAttempts, 1, "a missing CLI is not retried");
  assert.equal(permanent.failure.kind, "permanent");

  let thrownAttempts = 0;
  const thrown = runWithRetries(() => {
    thrownAttempts += 1;
    const error = new Error("spawn failed");
    error.code = "ENOENT";
    throw error;
  }, { attempts: 2, timeoutMs: 45_000, startedAt: Date.now(), sleep: noSleep });
  assert.equal(thrownAttempts, 1);
  assert.equal(thrown.error.code, "ENOENT");
});

test("runWithRetries gives up when the request budget is spent", () => {
  let attempts = 0;
  let elapsed = 0;
  const result = runWithRetries(() => {
    attempts += 1;
    elapsed += 44_000;
    return [{ error: { message: "request timed out" } }];
  }, {
    attempts: 2,
    timeoutMs: 45_000,
    startedAt: 0,
    now: () => elapsed,
    sleep: noSleep,
  });
  assert.equal(attempts, 1, "no second attempt when the timeout already consumed the budget");
  assert.equal(result.failure.transient, true);
});
