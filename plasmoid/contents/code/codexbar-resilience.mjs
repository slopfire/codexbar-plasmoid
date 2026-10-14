// Resilience layer for the CodexBar plasmoid helper.
//
// Usage and cost fetches go through shell commands that fail transiently: the
// Codex app-server JSON-RPC bridge times out while a session is busy
// (`account/rateLimits/read`), a dashboard answers 429 to a burst of reads, or
// a local cost scan exceeds its timeout. The old shared cache stored whatever
// came back, so one failure was replayed for the whole cache window and the
// widget dropped from live limits to a red error card (or to a cost-only
// fallback).
//
// This module turns the cache into a two-tier store:
//
//   <key>.json         last known good payload, kept past its freshness window
//   <key>.state.json   failure count, last error, and cooldown deadline
//
// Rules:
//   - a fresh good payload wins and is returned untouched;
//   - a failed refresh keeps the good payload on screen, tagged `stale`, and
//     opens a cooldown so the next refresh does not hammer a provider that just
//     asked us to back off;
//   - a failure with no good payload is negative-cached for the cooldown only
//     (seconds to minutes, never the full cost/usage window);
//   - a success clears the failure state.
//
// Everything here is pure except `fetchWithResilience`, which owns the cache
// files. Keeping the classification and backoff pure lets the node test suite
// pin the behaviour down without spawning a CLI.

import fs from "node:fs";
import path from "node:path";

export const CACHE_VERSION = 2;
export const DEFAULT_COOLDOWN_BASE_MS = 30 * 1000;
// Three minutes is the widget default: long enough to skip a rate-limited
// refresh, short enough that a recovered provider is back within a couple of
// polls at the default 150s interval. The General settings page can raise it.
export const DEFAULT_COOLDOWN_CAP_MS = 3 * 60 * 1000;
export const DEFAULT_FENCE_MS = 5000;

const UNSUPPORTED_PATTERNS = [
  /not supported/i,
  /unsupported/i,
  /method not found/i,
  /-32601/,
  /only supported on macos/i,
  /does not support/i,
  /not implemented/i,
  /unknown method/i,
  /no such (?:method|command)/i,
];

const AUTH_PATTERNS = [
  /\b401\b/,
  /\b403\b/,
  /unauthorized/i,
  /invalid or expired/i,
  /not logged in/i,
  /login required/i,
  /no .{0,24}session found/i,
  /missing .{0,24}(?:cookie|token|api key|credential|auth)/i,
  /credentials/i,
  /api key/i,
];

const TRANSIENT_PATTERNS = [
  /timed out/i,
  /\btimeout\b/i,
  /ETIMEDOUT/,
  /ECONNRESET/,
  /ECONNREFUSED/,
  /EAI_AGAIN/,
  /\bEPIPE\b/,
  /socket hang ?up/i,
  /connection reset/i,
  /network error/i,
  /temporar/i,
  /rate.?limit/i,
  /\b429\b/,
  /too many requests/i,
  /quota exceeded/i,
  /\b50[0234]\b/,
  /service unavailable/i,
  /bad gateway/i,
  /overloaded/i,
  /try again/i,
  /not running/i,
  /unavailable/i,
  /\bbusy\b/i,
];

const RETRY_HINT_PATTERNS = [
  /retry[-\s]?after[^0-9]{0,12}(\d+(?:\.\d+)?)\s*(ms|milliseconds?|s|sec|seconds?|m|min|minutes?)?/i,
  /try again(?: in| after)?[^0-9]{0,12}(\d+(?:\.\d+)?)\s*(ms|milliseconds?|s|sec|seconds?|m|min|minutes?)?/i,
];

function toText(value) {
  if (typeof value === "string") {
    return value.trim();
  }
  if (value === null || value === undefined) {
    return "";
  }
  return String(value).trim();
}

/**
 * Read the "try again in N" hint a provider or CLI put in its message.
 * Returns milliseconds, or null when the message carries no hint. A bare
 * number follows the HTTP `Retry-After` convention and means seconds.
 */
export function parseRetryAfterMs(message) {
  const text = toText(message);
  if (!text) {
    return null;
  }
  for (const pattern of RETRY_HINT_PATTERNS) {
    const match = pattern.exec(text);
    if (!match) {
      continue;
    }
    const amount = Number(match[1]);
    if (!Number.isFinite(amount) || amount <= 0) {
      continue;
    }
    const unit = (match[2] || "s").toLowerCase();
    if (unit.startsWith("ms") || unit.startsWith("milli")) {
      return Math.round(amount);
    }
    if (unit.startsWith("m")) {
      return Math.round(amount * 60_000);
    }
    return Math.round(amount * 1000);
  }
  return null;
}

/**
 * Bucket a failure message. `transient` failures are worth retrying and worth
 * a short cooldown; `unsupported` failures mean the selected source cannot
 * answer and a fallback source should be tried; `auth` failures need the user.
 */
export function classifyFailureMessage(message) {
  const text = toText(message);
  if (!text) {
    return { kind: "permanent", transient: false, retryAfterMs: null, message: "" };
  }
  const retryAfterMs = parseRetryAfterMs(text);
  let kind = "permanent";
  if (UNSUPPORTED_PATTERNS.some((pattern) => pattern.test(text))) {
    kind = "unsupported";
  } else if (AUTH_PATTERNS.some((pattern) => pattern.test(text))) {
    kind = "auth";
  } else if (TRANSIENT_PATTERNS.some((pattern) => pattern.test(text)) || retryAfterMs !== null) {
    kind = "transient";
  }
  return { kind, transient: kind === "transient", retryAfterMs, message: text };
}

/**
 * Inspect a CLI payload and report whether it represents a failure.
 *
 * The plasmoid's CLIs report failures as data, not exit codes: a payload is an
 * array where every item carries an `error`. An empty array is a valid "no
 * accounts" answer, a non-object payload is left alone, and a mixed array
 * counts as success because at least one account answered.
 */
export function failureFromPayload(value) {
  const items = Array.isArray(value) ? value : [value];
  const objects = items.filter((item) => item && typeof item === "object");
  if (objects.length === 0) {
    return null;
  }
  const errored = objects.filter((item) => item.error);
  if (errored.length !== objects.length) {
    return null;
  }
  const first = errored[0].error;
  const message = toText(first?.message) || toText(first?.description) || toText(first);
  return classifyFailureMessage(message);
}

/**
 * Tag a last-known-good payload as stale so QML can keep rendering it with a
 * soft "retrying" note instead of a red error. The stored copy is never
 * mutated; annotation happens at read time only.
 */
export function annotateStaleValue(value, info = {}) {
  const items = Array.isArray(value) ? value : [value];
  return items.map((item) => {
    if (!item || typeof item !== "object") {
      return item;
    }
    return {
      ...item,
      stale: true,
      staleSince: info.since ?? null,
      lastAttemptError: info.error
        ? { message: toText(info.error.message), kind: info.error.kind || null }
        : null,
    };
  });
}

/** Backoff for the Nth consecutive failure, honouring a Retry-After hint. */
export function cooldownMsFor(failures, failure = null, options = {}) {
  const cap = options.capMs ?? DEFAULT_COOLDOWN_CAP_MS;
  // A cap below the base would make every retry wait the base interval anyway,
  // so fold the base down to keep "cap" a real ceiling.
  const base = Math.min(options.baseMs ?? DEFAULT_COOLDOWN_BASE_MS, cap);
  const hinted = failure?.retryAfterMs;
  if (Number.isFinite(hinted) && hinted > 0) {
    return Math.min(cap, Math.max(base, hinted));
  }
  const attempt = Math.max(0, Number(failures) - 1);
  const backoff = base * 2 ** Math.min(attempt, 6);
  return Math.min(cap, backoff);
}

/** Spread retry/cooldown timing so several widgets do not fire in lockstep. */
export function jitterMs(milliseconds, ratio = 0.25) {
  const clamped = Math.max(0, Number(milliseconds) || 0);
  const spread = clamped * Math.max(0, Math.min(1, ratio));
  if (spread === 0) {
    return Math.round(clamped);
  }
  return Math.round(clamped - spread + Math.random() * spread * 2);
}

function writeJsonAtomic(filePath, value) {
  const temporaryPath = `${filePath}.${process.pid}.${Date.now()}.tmp`;
  try {
    fs.writeFileSync(temporaryPath, JSON.stringify(value), { mode: 0o600 });
    fs.renameSync(temporaryPath, filePath);
  } finally {
    try {
      fs.unlinkSync(temporaryPath);
    } catch {
      // The rename already consumed it, or another writer owns it.
    }
  }
}

function readGood(cachePath) {
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync(cachePath, "utf8"));
  } catch {
    return null;
  }
  if (
    parsed
    && typeof parsed === "object"
    && !Array.isArray(parsed)
    && parsed.cacheVersion === CACHE_VERSION
  ) {
    return { value: parsed.value, storedAt: Number(parsed.storedAt) || 0 };
  }
  // Legacy cache entry: the file held the raw payload and its mtime was the
  // timestamp. Payloads that are already failures are dropped rather than
  // promoted to "good" data so a poisoned cache heals on first read.
  if (failureFromPayload(parsed)) {
    return null;
  }
  let storedAt = 0;
  try {
    storedAt = fs.statSync(cachePath).mtimeMs;
  } catch {
    return null;
  }
  return { value: parsed, storedAt };
}

function writeGood(cachePath, value, storedAt) {
  writeJsonAtomic(cachePath, { cacheVersion: CACHE_VERSION, storedAt, value });
}

function readState(statePath) {
  try {
    const parsed = JSON.parse(fs.readFileSync(statePath, "utf8"));
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      return parsed;
    }
  } catch {
    // A missing or half-written state file simply means "no failures yet".
  }
  return { failures: 0, lastAttemptAt: 0, lastError: null, lastFailureValue: null, cooldownUntilMs: 0 };
}

function removeStaleLock(lockPath, timeoutMs, now) {
  try {
    const stat = fs.statSync(lockPath);
    if (now() - stat.mtimeMs > timeoutMs + 10_000) {
      fs.unlinkSync(lockPath);
    }
  } catch {
    // The lock disappeared between checks; the next loop reads the cache.
  }
}

function errorText(error) {
  const stderr = toText(error?.stderr?.toString?.());
  if (stderr) {
    return stderr.split("\n").slice(-4).join("\n");
  }
  if (error?.code === "ENOENT") {
    return "CLI not found";
  }
  if (error?.signal === "SIGTERM" || error?.code === "ETIMEDOUT") {
    return "Command timed out";
  }
  return toText(error?.message) || String(error);
}

function produceAndRecord(deps) {
  const {
    valuePath,
    statePath,
    producer,
    maxStaleMs,
    cooldownBaseMs,
    cooldownCapMs,
    forceRefresh,
    now,
  } = deps;

  const good = readGood(valuePath);
  const state = readState(statePath);
  // `maxStaleMs` is deliberately NOT consulted here. It bounds how long a
  // *failure* may keep replaying a payload nobody is revalidating; it must not
  // gate the failure path itself. The cached payload is the last thing that
  // worked, and on screen (tagged `stale`) it is far more useful than the error
  // entry that replaced live bars with a red card or a bare cost panel.
  const hasGood = !!good;
  const coolingDown = !forceRefresh && Number(state.cooldownUntilMs || 0) > now();

  if (coolingDown) {
    if (hasGood) {
      return annotateStaleValue(good.value, { since: good.storedAt, error: state.lastError });
    }
    if (state.lastFailureValue !== null && state.lastFailureValue !== undefined) {
      return state.lastFailureValue;
    }
  }
  let value = null;
  let thrown = null;
  try {
    value = producer();
  } catch (error) {
    thrown = error;
  }

  const failure = thrown
    ? classifyFailureMessage(errorText(thrown))
    : failureFromPayload(value);

  if (!failure) {
    writeGood(valuePath, value, now());
    writeJsonAtomic(statePath, {
      failures: 0,
      lastAttemptAt: now(),
      lastError: null,
      lastFailureValue: null,
      cooldownUntilMs: 0,
    });
    return value;
  }

  const failures = Number(state.failures || 0) + 1;
  const cooldownMs = cooldownMsFor(failures, failure, { baseMs: cooldownBaseMs, capMs: cooldownCapMs });
  writeJsonAtomic(statePath, {
    failures,
    lastAttemptAt: now(),
    lastError: { message: failure.message, kind: failure.kind },
    lastFailureValue: value,
    cooldownUntilMs: now() + cooldownMs,
  });

  // The producer failed. Anything we cached is the last payload that worked, so
  // keep serving it (tagged stale) rather than handing back an error entry that
  // would replace live bars with a red card or a bare cost panel. A manual
  // refresh (--force) lands here too: it wants fresh data, but not at the price
  // of blanking limits that were on screen a moment ago.
  if (hasGood) {
    return annotateStaleValue(good.value, { since: good.storedAt, error: failure });
  }

  // Nothing good was ever cached: the payload the producer returned is the only
  // thing we know, so hand it back and let the caller decide what to render.
  if (thrown) {
    throw thrown;
  }
  return value;
}

/**
 * Stale-while-revalidate: return whatever good payload the cache holds and let
 * a detached worker refresh it. Used for expensive scans (a full Codex cost
 * walk takes 60-120s) where waiting inline would block the whole render.
 * `revalidate.spawn` owns its own interlock so several widgets do not all spawn
 * a worker; a missing payload falls back to `emptyValue`.
 */
function serveWithBackgroundRefresh(deps) {
  const {
    cachePath,
    statePath,
    freshEnough,
    revalidate,
    emptyValue,
    maxStaleMs,
    forceRefresh,
    now,
  } = deps;
  const good = readGood(cachePath);
  if (good && freshEnough(good.storedAt)) {
    return good.value;
  }

  const state = readState(statePath);
  const coolingDown = !forceRefresh && Number(state.cooldownUntilMs || 0) > now();
  if (!coolingDown) {
    try {
      revalidate.spawn();
    } catch {
      // A background refresh is best effort; the render must not fail with it.
    }
  }

  // Serve the last good payload even when it has aged past `maxStaleMs`. The
  // alternative — `emptyValue` — is a blank card: no history bars, no token
  // stats. An old payload tagged `stale` keeps the widget informative and the
  // card note ("Last updated … — retrying") tells the user it is not current.
  // `emptyValue` stays as the fallback for "nothing was ever cached".
  if (good) {
    return annotateStaleValue(good.value, { since: good.storedAt, error: state.lastError });
  }
  return emptyValue;
}

/**
 * Fetch through the two-tier cache.
 *
 * `producer` returns the CLI payload (or throws). The returned value is either
 * the producer's success value, a stale copy of the last success annotated
 * with `stale`/`lastAttemptError`, or the raw failure when nothing good was
 * ever cached.
 */
export function fetchWithResilience(options) {
  const {
    cachePath,
    producer,
    cacheSeconds = 0,
    timeoutMs = 45_000,
    forceRefresh = false,
    requestStartedAt = Date.now(),
    maxStaleMs = 24 * 60 * 60 * 1000,
    cooldownBaseMs = DEFAULT_COOLDOWN_BASE_MS,
    cooldownCapMs = DEFAULT_COOLDOWN_CAP_MS,
    now = () => Date.now(),
    sleep = (milliseconds) => {
      const sleeper = new Int32Array(new SharedArrayBuffer(4));
      Atomics.wait(sleeper, 0, 0, milliseconds);
    },
    fenceMs = DEFAULT_FENCE_MS,
    revalidate = null,
    emptyValue = [],
  } = options;

  const statePath = `${cachePath}.state.json`;
  const lockPath = `${cachePath}.lock`;
  fs.mkdirSync(path.dirname(cachePath), { recursive: true, mode: 0o700 });

  const freshEnough = (storedAt) => (forceRefresh
    ? storedAt >= requestStartedAt
    : now() - storedAt <= cacheSeconds * 1000);

  if (revalidate) {
    return serveWithBackgroundRefresh({
      cachePath,
      statePath,
      freshEnough,
      revalidate,
      emptyValue,
      maxStaleMs,
      forceRefresh,
      now,
    });
  }

  const takeFresh = () => {
    const good = readGood(cachePath);
    return good && freshEnough(good.storedAt) ? good.value : null;
  };

  const record = () => produceAndRecord({
    valuePath: cachePath,
    statePath,
    producer,
    maxStaleMs,
    cooldownBaseMs,
    cooldownCapMs,
    forceRefresh,
    now,
  });

  const waitDeadline = now() + timeoutMs + fenceMs;
  while (true) {
    const fresh = takeFresh();
    if (fresh !== null) {
      return fresh;
    }

    let lockFd = null;
    try {
      lockFd = fs.openSync(lockPath, "wx", 0o600);
      fs.writeFileSync(lockFd, `${process.pid}\n${now()}\n`);

      // Another helper may have populated the cache between our read and lock.
      const afterLock = takeFresh();
      if (afterLock !== null) {
        return afterLock;
      }
      return record();
    } catch (error) {
      if (error?.code !== "EEXIST") {
        throw error;
      }
      removeStaleLock(lockPath, timeoutMs, now);
      if (now() >= waitDeadline) {
        // A broken peer must not prevent this widget from refreshing forever.
        return record();
      }
      sleep(50);
    } finally {
      if (lockFd !== null) {
        try {
          fs.closeSync(lockFd);
        } catch {
          // Already closed.
        }
        try {
          fs.unlinkSync(lockPath);
        } catch {
          // Already removed.
        }
      }
    }
  }
}

/**
 * Run a CLI producer, retrying transient failures only.
 *
 * A timeout consumes the whole per-attempt budget, so the retry only fires
 * when the first attempt failed early (a busy RPC socket, a 429, a reset
 * connection). Retries are jittered and only start while the request budget
 * still has room for another attempt.
 */
export function runWithRetries(producer, options = {}) {
  const {
    attempts = 2,
    timeoutMs = 45_000,
    startedAt = Date.now(),
    minRemainingMs = 3000,
    baseDelayMs = 400,
    now = () => Date.now(),
    sleep = (milliseconds) => {
      const sleeper = new Int32Array(new SharedArrayBuffer(4));
      Atomics.wait(sleeper, 0, 0, milliseconds);
    },
  } = options;

  let lastFailure = null;
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    let value;
    let thrown = null;
    try {
      value = producer();
    } catch (error) {
      thrown = error;
    }
    const failure = thrown
      ? classifyFailureMessage(errorText(thrown))
      : failureFromPayload(value);
    if (!failure) {
      return { value, error: null, failure: null, attempts: attempt };
    }
    if (!failure.transient) {
      // Non-transient failures keep their original shape for the caller so the
      // existing error entry is built exactly as it was before.
      return thrown
        ? { value: null, error: thrown, failure, attempts: attempt }
        : { value, error: null, failure, attempts: attempt };
    }

    lastFailure = { value, error: thrown, failure, attempts: attempt };
    const remaining = timeoutMs - (now() - startedAt);
    if (attempt >= attempts || remaining < minRemainingMs) {
      break;
    }
    sleep(jitterMs(baseDelayMs * attempt));
  }
  return lastFailure;
}
