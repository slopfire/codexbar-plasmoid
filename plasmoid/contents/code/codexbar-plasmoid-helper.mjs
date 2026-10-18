#!/usr/bin/env node
import { execFileSync, spawn } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  classifyFailureMessage,
  failureFromPayload,
  fetchWithResilience,
  runWithRetries,
} from "./codexbar-resilience.mjs";

const rawArgs = process.argv.slice(2);
const args = parseArgs(rawArgs);
// KDE's plasmoid process inherits the user's session env from systemd --user
// (which sources $XDG_CONFIG_HOME/environment.d/*.conf), but env vars th e
// user only set in their shell rc files (e.g. ~/.zshrc) do not make it
// across. Re-read the standard env.d directory plus a plasmoid-local
// ~/.codexbar/.env so API keys and other exports still reach the spawned
// CLIs. Values that were already on process.env when the helper started
// take precedence so explicit overrides still win.
loadEnvironmentFromFiles();
const timeoutMs = Math.max(5, Number(args.timeout || 45)) * 1000;
// Reliability knobs from the widget settings. Each has the same default the
// KConfigXT schema ships, so a helper invoked without them behaves identically.
const retryAttempts = boundedNumber(args["retry-attempts"], 2, 1, 4);
const cooldownCapMs = boundedNumber(args["cooldown-cap-seconds"], 180, 30, 3600) * 1000;
const costWorkerTimeoutMs = boundedNumber(args["cost-timeout"], 300, 60, 1800) * 1000;
// Subprocess budget. The detached cost worker raises this because a full
// Codex cost scan takes 60-120s on a warm machine and the widget's request
// timeout is deliberately short.
let execTimeoutMs = timeoutMs;

function boundedNumber(raw, fallback, min, max) {
  const value = Number(raw);
  const chosen = Number.isFinite(value) && value > 0 ? value : fallback;
  return Math.min(max, Math.max(min, Math.round(chosen)));
}
const cliPath = args.cli || process.env.CODEXBAR_CLI || "codexbar";
const nativeCliPath = args.nativeCli || process.env.CODEXBAR_NATIVE_CLI || resolveNativeCliPath();
const autoUpdate = args.autoUpdate === "true" || args["auto-update"] === "true";
const updateTag = clean(args.tag) || "latest";
const managedCliBinary = managedBinary();
const sharedCacheSeconds = Math.max(0, Number(args.cacheSeconds || args["cache-seconds"] || 0));
const costCacheArg = args.costCacheSeconds ?? args["cost-cache-seconds"];
const costCacheSeconds = Math.max(0, Number(
  costCacheArg === undefined ? sharedCacheSeconds : costCacheArg,
));
const forceRefresh = args.force === "true";
const requestStartedAt = Date.now();

function injectSqliteEnv(targetDir) {
  for (const envVar of ["NIX_LD_LIBRARY_PATH", "LD_LIBRARY_PATH"]) {
    const existing = process.env[envVar] || "";
    const paths = existing.split(":").filter(Boolean);
    if (!paths.includes(targetDir)) {
      process.env[envVar] = [targetDir, ...paths].join(":");
    }
  }
}

function loadEnvironmentFromFiles() {
  // systemd --user sources KEY=VAL entries from $XDG_CONFIG_HOME/environment.d
  // (default ~/.config/environment.d). KDE inherits that into the user session,
  // but the plasmoid process sometimes loses them — re-read the directory
  // so API keys set with `systemctl --user set-environment` or by hand survive
  // a plasmoid restart. Values that were already on process.env when the
  // helper started win so explicit overrides still take precedence; later
  // files override earlier ones (matching systemd semantics).
  const preExisting = new Set(Object.keys(process.env));
  const applied = new Map();
  const envDir = path.join(
    process.env.XDG_CONFIG_HOME && clean(process.env.XDG_CONFIG_HOME)
      ? process.env.XDG_CONFIG_HOME
      : path.join(os.homedir(), ".config"),
    "environment.d",
  );
  const entries = [];
  if (fs.existsSync(envDir) && fs.statSync(envDir).isDirectory()) {
    try {
      for (const name of fs.readdirSync(envDir)) {
        if (name.endsWith(".conf")) {
          entries.push(path.join(envDir, name));
        }
      }
    } catch {
      // unreadable env dir: skip without blocking the helper
    }
  }
  entries.sort();
  // ~/.codexbar/.env is the plasmoid-local dotenv; convenient for keys the
  // user does not want to leak into a system-wide environment.d. It loads
  // last so its values win over system-wide defaults.
  const dotenvPath = path.join(os.homedir(), ".codexbar", ".env");
  if (fs.existsSync(dotenvPath)) {
    entries.push(dotenvPath);
  }
  for (const file of entries) {
    for (const [key, value] of parseEnvFile(file)) {
      if (preExisting.has(key)) {
        continue;
      }
      applied.set(key, value);
    }
  }
  for (const [key, value] of applied) {
    process.env[key] = value;
  }
}

function parseEnvFile(filePath) {
  const pairs = [];
  let raw;
  try {
    raw = fs.readFileSync(filePath, "utf8");
  } catch {
    return pairs;
  }
  for (const line of raw.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) {
      continue;
    }
    // Skip shell-only prefixes (`export FOO=bar`) and command substitutions
    // we cannot safely expand inside a Node process.
    const stripped = trimmed.replace(/^export\s+/, "");
    const eqIndex = stripped.indexOf("=");
    if (eqIndex <= 0) {
      continue;
    }
    const key = stripped.slice(0, eqIndex).trim();
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) {
      continue;
    }
    let value = stripped.slice(eqIndex + 1).trim();
    // Strip matching single or double quotes.
    if (
      value.length >= 2 &&
      ((value.startsWith('"') && value.endsWith('"')) ||
        (value.startsWith("'") && value.endsWith("'")))
    ) {
      value = value.slice(1, -1);
    }
    pairs.push([key, value]);
  }
  return pairs;
}

const managedDir = path.dirname(managedCliBinary);
if (fs.existsSync(path.join(managedDir, "libsqlite3.so.0"))) {
  injectSqliteEnv(managedDir);
}
const provider = clean(args.provider) || "all";
const source = clean(args.source) || "auto";
const localProviderConfigs = parseProviderConfigs(providerConfigInput());
const syncProviders = args.syncProviders === "true" || args["sync-providers"] === "true";
const providerConfigs = syncProviders ? loadSharedProviderConfigs(localProviderConfigs) : localProviderConfigs;
const includeCost = args.cost !== "false";
const includeStatus = args.status !== "false";
const showCredits = args.credits !== "false";
const anonymizeEmails = args.anonymizeEmails !== "false" && args["anonymize-emails"] !== "false";
const kdeProviderConfig = loadKdeProviderConfig();

// Sync-server settings. When a server URL is configured, every successful
// refresh publishes this device's locally fetched entries (never credentials,
// never remote-sourced entries that came back from the server) and provider
// rows with the "remote" source read the server's merged result instead of
// fetching with local credentials.
const syncUrl = clean(args["sync-url"]) || "";
const syncToken = clean(args["sync-token"]) || "";
const syncDevice =
  sanitizeSyncDevice(clean(args["sync-device"]) || suggestedDeviceName()) || "plasmoid";
// "Sync all from server": render the server's merged cross-device state for
// every provider and skip local fetching entirely, so a credential-less
// device (or one that should not poll providers) still shows usage. Work is
// short-circuited in main(); these functions stay available for the normal
// per-provider remote-source path.
const syncAllFromServer = args["sync-all"] === "true" || args.syncAll === "true";

const nativeProviders = new Set(["antigravity", "codebuddy", "commandcode", "cursor", "devin", "grok", "opencode", "opencodego"]);

// Upstream codexbar cost scans Claude local logs. Codex cost is scanned
// natively in Rust instead: upstream `codexbar cost` walks Codex history in
// Swift and SIGSEGVs on Linux. The helper falls back to one upstream
// `codexbar cost --provider codex` attempt only when the native scan comes
// back missing or all-errors. Native cost also covers OpenCode SQLite,
// Cursor dashboard events, Grok local session usage, and Command Code
// session transcripts.
// Antigravity / Devin only expose quota percentages (no absolute token history).
const CODEXBAR_COST_PROVIDERS = new Set(["claude"]);
const NATIVE_COST_PROVIDERS = new Set(["opencode", "opencodego", "cursor", "grok", "commandcode", "codex"]);

const linuxAutoFallbacks = {
  codex: "cli",
  claude: "cli",
  clinepass: "api",
  // CodeBuddy has no Linux-capable upstream source. The native fetcher reads the
  // bearer token the CLI stores under ~/.local/share/CodeBuddyExtension.
  codebuddy: "native",
  // Command Code's web source needs a logged-in commandcode.ai browser session;
  // the native fetcher reuses the `cmd login` key in ~/.commandcode/auth.json.
  commandcode: "native",
  cursor: "native",
  opencode: "native",
  opencodego: "native",
  antigravity: "native",
  devin: "native",
  augment: "cli",
  factory: "cli",
  // Grok agent-stdio billing RPC is broken (-32601). The native fetcher reads
  // every session: ~/.grok/auth.json (grok login) plus browser sso cookies
  // (Chrome/Zen/Firefox). Plan is only SuperGrok when /rest/subscriptions is active.
  grok: "native",
  jetbrains: "cli",
  kilo: "api",
  kiro: "cli",
  windsurf: "cli",
  openai: "api",
  azureopenai: "api",
  gemini: "api",
  copilot: "api",
  minimax: "api",
  alibaba: "api",
  bedrock: "api",
  deepgram: "api",
  deepseek: "api",
  doubao: "api",
  groq: "api",
  kimik2: "api",
  llmproxy: "api",
  moonshot: "api",
  ollama: "api",
  openrouter: "api",
  synthetic: "api",
  venice: "api",
  zai: "api",
  vertexai: "oauth",
  demo: "local",
};

// A resolved source that cannot answer must not leave the card blank. Only a
// transient failure (busy RPC socket, 429) or an unsupported source moves on to
// the next entry; auth and configuration errors stay put because a different
// source would fail the same way. Codex's `cli` source talks to the Codex
// app-server JSON-RPC bridge, which times out while a session is busy, while
// `oauth` reads the same limits from the ChatGPT backend independently.
const sourceFallbacks = {
  codex: ["oauth"],
};

function currentCliPath() {
  return resolveEffectiveCliPath(cliPath, autoUpdate, managedCliBinary);
}

function shouldAutoUpdate() {
  if (!autoUpdate) {
    return false;
  }
  // Only manage the binary when the user has not pointed us at a specific
  // installation. An absolute or relative path means they want that binary.
  const requested = clean(cliPath);
  return requested === "" || requested === "codexbar" || requested === managedCliBinary;
}

async function main() {
  if (args.costWorker === "true" || args["cost-worker"] === "true") {
    runCostWorker();
    return;
  }

  if (args.syncTest === "true" || args["sync-test"] === "true") {
    process.stdout.write(`${JSON.stringify(await testSyncServer())}\n`);
    return;
  }

  if (syncAllFromServer) {
    // Server-state mode owns the whole snapshot: no CLI update, no local
    // fetches, no publish (there is nothing local to publish).
    const snapshot = await buildServerOnlySnapshot();
    process.stdout.write(`${JSON.stringify(snapshot)}\n`);
    return;
  }

  let updateResult = null;
  if (shouldAutoUpdate()) {
    try {
      const updaterModule = await import(new URL("./codexbar-cli-updater.mjs", import.meta.url).href);
      updateResult = await updaterModule.updateIfNeeded({ targetDir: path.dirname(managedCliBinary), tag: updateTag });
    } catch (error) {
      updateResult = { ok: false, updated: false, error: shortError(error) };
    }
  }

  try {
    const usage = runUsage();
    let cost = [];
    let costError = null;
    if (includeCost) {
      const costResult = runCost();
      cost = costResult.items;
      costError = costResult.costError;
    }
    const snapshot = normalizeSnapshot(usage, cost, costError);
    await mergeRemoteEntriesIntoSnapshot(snapshot);
    snapshot.cliUpdate = updateResult;
    if (syncUrl) {
      snapshot.publish = await publishSnapshot(snapshot);
    }
    process.stdout.write(`${JSON.stringify(snapshot)}\n`);
  } catch (error) {
    const snapshot = errorSnapshot(error);
    snapshot.cliUpdate = updateResult;
    process.stdout.write(`${JSON.stringify(snapshot)}\n`);
    process.exitCode = 0;
  }
}

main();

function parseArgs(rawArgs) {
  const parsed = {};
  for (let index = 0; index < rawArgs.length; index += 1) {
    const token = rawArgs[index];
    if (!token.startsWith("--")) {
      continue;
    }
    const key = token.slice(2);
    const next = rawArgs[index + 1];
    if (next === undefined || next.startsWith("--")) {
      parsed[key] = "true";
    } else {
      parsed[key] = next;
      index += 1;
    }
  }
  return parsed;
}

function runUsage() {
  const configs = effectiveProviderConfigs();
  if (configs.length > 0) {
    const items = configs
      .filter((config) => !usesRemoteSource(config))
      .flatMap((config) => asArray(runUsageForConfig(config)));
    // codex-cli omits free rate-limit reset credits; oauth carries them. Merge when missing.
    return enrichCodexLimitResetCredits(items);
  }
  const legacy = {
    provider,
    source,
    account: clean(args.account),
    accountIndex: Number(args.accountIndex || 0),
    allAccounts: args.allAccounts === "true",
  };
  if (usesRemoteSource(legacy)) {
    return [];
  }
  // codex-cli omits free rate-limit reset credits; oauth carries them. Merge when missing.
  return enrichCodexLimitResetCredits(asArray(runUsageForConfig(legacy)));
}

/**
 * Codex `cli` / local sources report weekly limits but not Limit Reset Credits.
 * The oauth usage payload includes usage.codexResetCredits — pull that in when
 * the primary source left it out so the widget matches `codexbar … --pretty`.
 */
function enrichCodexLimitResetCredits(items) {
  return asArray(items).map((item) => {
    if (normalizeProviderId(item?.provider) !== "codex") {
      return item;
    }
    if (!item?.usage || item.usage.codexResetCredits) {
      return item;
    }
    const currentSource = clean(item.source).toLowerCase();
    if (currentSource === "oauth") {
      return item;
    }
    try {
      // Codex oauth rejects --account ("does not support token accounts"); always
      // fetch the default oauth session and match by email when multiple return.
      const commandArgs = [
        "usage",
        "--format",
        "json",
        "--json-only",
        "--provider",
        "codex",
        "--source",
        "oauth",
      ];
      if (includeStatus) {
        commandArgs.push("--status");
      }
      const command = currentCliPath();
      const oauthPayload = sharedFetch("usage", {
        command,
        commandArgs,
        provider: "codex",
        apiKeyHash: "",
        account: "",
      }, () => fetchUsageWithRetry(command, commandArgs, "codex", "", ""));
      const oauthItems = asArray(oauthPayload).filter((candidate) => (
        normalizeProviderId(candidate?.provider) === "codex" && !candidate?.error
      ));
      const accountLower = account.toLowerCase();
      const match = (accountLower
        ? oauthItems.find((candidate) => {
          const candidateAccount = clean(
            candidate.account
            || candidate.usage?.accountEmail
            || candidate.usage?.identity?.accountEmail,
          ).toLowerCase();
          return candidateAccount === accountLower;
        })
        : null) || oauthItems[0];
      const resetCredits = match?.usage?.codexResetCredits;
      if (!resetCredits) {
        return item;
      }
      return {
        ...item,
        usage: {
          ...item.usage,
          codexResetCredits: resetCredits,
        },
      };
    } catch {
      return item;
    }
  });
}

function runUsageForConfig(config) {
  // Built-in sample provider for tray/card UI testing — no CLI or network.
  if (normalizeProviderId(config.provider) === "demo") {
    return [buildDemoUsagePayload(config)];
  }

  // Walk the configured source, then any provider fallback, so a busy RPC
  // socket or an unsupported source does not leave the card empty. A fresh
  // success wins; a stale cache hit still tries the next source, but stays as
  // the answer if the fallback also fails, because stale limits beat an error.
  const chain = usageSourceChain(config);
  let lastResult = null;
  let staleResult = null;
  for (let index = 0; index < chain.length; index += 1) {
    const result = runUsageForSource(config, chain[index]);
    const failure = failureFromPayload(result);
    const servedStale = !failure && asArray(result).some((item) => item?.stale === true);
    if (!failure && !servedStale) {
      return result;
    }
    if (servedStale && !staleResult) {
      staleResult = result;
    }
    lastResult = result;
    if (!chain[index + 1]) {
      break;
    }
    if (failure && !isFallbackWorthTrying(failure)) {
      break;
    }
  }
  return staleResult || lastResult;
}

function isFallbackWorthTrying(failure) {
  return failure.transient || failure.kind === "unsupported";
}

function usageSourceChain(config) {
  const primary = resolveSource(config.provider, config.source);
  const chain = [primary];
  for (const candidate of sourceFallbacks[normalizeProviderId(config.provider)] || []) {
    if (candidate && !chain.includes(candidate)) {
      chain.push(candidate);
    }
  }
  return chain;
}

function runUsageForSource(config, source) {
  const commandArgs = [
    "usage",
    "--format",
    "json",
    "--json-only",
    "--provider",
    config.provider,
    "--source",
    source,
  ];

  if (includeStatus) {
    commandArgs.push("--status");
  }
  if (!showCredits) {
    commandArgs.push("--no-credits");
  }
  // Devin selects its organization through the account flag. OAuth sources
  // reject token-account flags outright, so a fallback source skips them.
  const skipAccountFlags = normalizeProviderId(config.provider) === "devin" || source === "oauth";
  if (!skipAccountFlags) {
    if (config.allAccounts === true) {
      commandArgs.push("--all-accounts");
    } else if (clean(config.account)) {
      commandArgs.push("--account", clean(config.account));
    } else if (Number(config.accountIndex || 0) > 0) {
      commandArgs.push("--account-index", String(Number(config.accountIndex)));
    }
  }

  const command = commandForConfig(config, source);
  try {
    return sharedFetch("usage", {
      command,
      commandArgs,
      provider: normalizeProviderId(config.provider),
      apiKeyHash: hashSecret(config.apiKey),
      account: clean(config.account),
    }, () => {
      try {
        return fetchUsageWithRetry(command, commandArgs, config.provider, config.apiKey, clean(config.account));
      } catch (error) {
        return [usageErrorEntry(config, source, command, error)];
      }
    });
  } catch (error) {
    return [usageErrorEntry(config, source, command, error)];
  }
}

/**
 * One CLI invocation, retried once when it fails transiently. A timeout burns
 * the whole attempt budget, so only a call that failed early — a busy Codex
 * app-server socket, a 429, a reset connection — is re-run.
 */
function fetchUsageWithRetry(command, commandArgs, providerId, apiKey, account) {
  const outcome = runWithRetries(
    () => runJSON(command, commandArgs, providerId, apiKey, account),
    { attempts: retryAttempts, timeoutMs: execTimeoutMs, startedAt: Date.now() },
  );
  if (outcome.error) {
    throw outcome.error;
  }
  return outcome.value;
}

function usageErrorEntry(config, source, command, error) {
  const failure = classifyFailureMessage(shortError(error, command));
  return {
    provider: normalizeProviderId(config.provider),
    source,
    error: { kind: failure.kind, message: failure.message },
  };
}

/**
 * Demo provider: fixed local sample data for widget UI checks.
 * Put comma-separated remaining percents in the account field, e.g. "1,11,35,72".
 * Defaults to low/mid values so tray bar fill and rounding are easy to verify.
 */
function buildDemoUsagePayload(config) {
  const now = new Date();
  const percents = parseDemoPercents(clean(config.account));
  // Keep every reset inside its window so the demo exercises both the
  // time-left marker and the helper's fallback pace calculation.
  const timings = [
    { windowMinutes: 300, resetInMinutes: 60 },
    { windowMinutes: 300, resetInMinutes: 180 },
    { windowMinutes: 10080, resetInMinutes: 2880 },
    { windowMinutes: 43200, resetInMinutes: 17280 },
  ];
  const usageRows = percents.map((percentLeft, index) => {
    const title = demoRowTitle(index, percentLeft);
    const timing = timings[index] || timings[timings.length - 1];
    return {
      id: `demo-${index + 1}`,
      title,
      percentLeft,
      windowMinutes: timing.windowMinutes,
      resetsAt: new Date(now.getTime() + timing.resetInMinutes * 60000).toISOString(),
    };
  });

  const dailyBreakdown = [];
  for (let offset = 29; offset >= 0; offset -= 1) {
    const day = new Date(now);
    day.setHours(0, 0, 0, 0);
    day.setDate(day.getDate() - offset);
    const year = day.getFullYear();
    const month = String(day.getMonth() + 1).padStart(2, "0");
    const dayNum = String(day.getDate()).padStart(2, "0");
    const dayKey = `${year}-${month}-${dayNum}`;
    const totalTokens = 8000 + (29 - offset) * 1200 + (offset % 5) * 400;
    const costUSD = Number((0.15 + (29 - offset) * 0.08 + (offset % 3) * 0.04).toFixed(2));
    dailyBreakdown.push({
      day: dayKey,
      totalTokens,
      totalCreditsUsed: costUSD,
      modelBreakdowns: [
        { name: "demo-model-a", costUSD: Number((costUSD * 0.65).toFixed(2)), totalTokens: Math.round(totalTokens * 0.6) },
        { name: "demo-model-b", costUSD: Number((costUSD * 0.35).toFixed(2)), totalTokens: Math.round(totalTokens * 0.4) },
      ],
    });
  }

  const accountLabel = clean(config.account) || "1,11,35,72";
  return {
    provider: "demo",
    source: "local",
    version: "demo",
    account: accountLabel,
    status: includeStatus
      ? { indicator: "none", description: "Demo / sample data" }
      : null,
    credits: showCredits
      ? { remaining: 42.5, updatedAt: now.toISOString() }
      : undefined,
    usage: {
      updatedAt: now.toISOString(),
      accountEmail: "demo@local",
      loginMethod: "demo",
      usageRows,
      providerCost: {
        used: 12.34,
        currencyCode: "USD",
        period: "Period",
      },
    },
    openaiDashboard: {
      codeReviewRemainingPercent: 91,
      dailyBreakdown,
    },
  };
}

function parseDemoPercents(raw) {
  // Default set spans hairline → low → mid-low → healthy for tray fill tests.
  const defaults = [1, 11, 35, 72];
  if (!raw) {
    return defaults;
  }
  // Allow "1,11,35" or "1 11 35" or "1%;11%;35%".
  const parts = raw
    .split(/[,;\s]+/)
    .map((token) => Number(String(token).replace(/%/g, "").trim()))
    .filter((value) => Number.isFinite(value));
  if (parts.length === 0) {
    return defaults;
  }
  return parts.slice(0, 4).map((value) => Math.max(0, Math.min(100, value)));
}

function demoRowTitle(index, percentLeft) {
  const names = ["Low", "Session", "Weekly", "Extra"];
  const base = names[index] || `Bar ${index + 1}`;
  const rounded = Math.round(Number(percentLeft));
  return `${base} ${rounded}%`;
}

function runCost(options = {}) {
  const configs = (effectiveProviderConfigs().length > 0
    ? effectiveProviderConfigs()
    : [{ provider, includeCost: true }])
    .filter((config) => config.includeCost !== false && !usesRemoteSource(config));

  if (configs.length === 0) {
    return { items: [], costError: null };
  }

  const items = [];
  const costErrors = [];
  for (const target of costTargets(configs)) {
    collectCostResults(items, costErrors, fetchCostWithCommand(target, options));
  }
  return { items, costError: costErrors.length > 0 ? costErrors.join(" | ") : null };
}

function collectCostResults(items, costErrors, payload) {
  for (const item of asArray(payload)) {
    if (item && typeof item === "object" && !item.error) {
      items.push(item);
      continue;
    }
    const message = item && typeof item === "object" && item.error
      ? (clean(item.error.message) || "CodexBar cost command failed")
      : "";
    if (message && !costErrors.includes(message)) {
      costErrors.push(message);
    }
  }
}

/**
 * The cost commands the current configuration needs. Claude keeps its upstream
 * scan; Codex is scanned by the bundled Rust binary (with one upstream
 * `--provider codex` attempt as fallback); every other Linux-native provider
 * has its own fast local scan.
 */
function costTargets(configs) {
  const targets = [];
  const seenProviders = new Set();

  const codexbarProviders = uniqueProviderIds(configs.filter((config) =>
    CODEXBAR_COST_PROVIDERS.has(normalizeProviderId(config.provider))
  ));
  if (codexbarProviders.length > 0) {
    targets.push({
      command: currentCliPath(),
      providerId: codexbarProviders.length === 1 ? codexbarProviders[0] : "all",
      backend: "codexbar",
    });
    for (const id of codexbarProviders) {
      seenProviders.add(id);
    }
  }

  for (const providerId of uniqueProviderIds(configs.filter((config) =>
    NATIVE_COST_PROVIDERS.has(normalizeProviderId(config.provider))
  ))) {
    targets.push({ command: nativeCliPath, providerId, backend: "native" });
    seenProviders.add(providerId);
  }

  // When auto-discovery requested a bare "all" without an explicit list, still
  // try native costs so local spend shows even if usage cookies fail.
  if (provider === "all" && configs.every((config) => !config._fromConfigs)) {
    for (const providerId of NATIVE_COST_PROVIDERS) {
      if (!seenProviders.has(providerId)) {
        targets.push({ command: nativeCliPath, providerId, backend: "native" });
      }
    }
  }

  return targets;
}

function uniqueProviderIds(configs) {
  const ids = [];
  const seen = new Set();
  for (const config of configs) {
    const id = normalizeProviderId(config.provider);
    if (!id || seen.has(id)) {
      continue;
    }
    seen.add(id);
    ids.push(id);
  }
  return ids;
}

/**
 * Cost is a background scanner, never a render dependency.
 *
 * `codexbar cost` walks the whole Codex history and takes 60-120s — past the
 * widget's request timeout — so an inline scan either blocks the refresh or
 * gets killed and reported as an error. The render path only reads the cache
 * and asks a detached worker to refresh it; the next refresh shows the new
 * figures. `options.synchronous` is the worker's path, which does the scan.
 */
function fetchCostWithCommand(target, options = {}) {
  const { command, providerId, backend } = target;
  const commandArgs = [
    "cost",
    "--format",
    "json",
    "--json-only",
    "--provider",
    providerId,
  ];
  // A manual refresh must bypass the CLI's own scan debounce. The flag is kept
  // out of the shared cache identity passed to sharedFetch below, so the
  // refreshed result overwrites the slot every widget polls instead of landing
  // in a refresh-only slot.
  const runArgs = forceRefresh && backend === "codexbar"
    ? [...commandArgs, "--refresh"]
    : commandArgs;
  const producer = () => {
    if (backend === "native" && normalizeProviderId(providerId) === "codex") {
      return fetchNativeCodexCostWithUpstreamFallback(command, commandArgs);
    }
    return runCostCommand(command, runArgs, providerId, backend);
  };
  try {
    const payload = sharedFetch("cost", {
      command,
      commandArgs,
      provider: providerId,
      backend,
    }, producer, costCacheSeconds, options.synchronous
      ? {}
      : { revalidate: { spawn: spawnCostWorker } });
    return asArray(payload);
  } catch (error) {
    return [{
      provider: "cost",
      error: { message: shortError(error, command) },
    }];
  }
}

/**
 * One cost scan against a single backend. Failures come back as data
 * (`[{provider: "cost", error}]`), never thrown, so usage entries survive a
 * cost failure.
 */
function runCostCommand(command, runArgs, providerId, backend) {
  try {
    const extraEnv = backend === "codexbar" ? { SWIFT_TESTING: "1" } : {};
    const outcome = runWithRetries(
      () => runJSON(command, runArgs, providerId, "", "", extraEnv),
      { attempts: retryAttempts, timeoutMs: execTimeoutMs, startedAt: Date.now() },
    );
    if (outcome.error) {
      throw outcome.error;
    }
    return outcome.value;
  } catch (error) {
    return [{
      provider: "cost",
      error: { message: shortError(error, command) },
    }];
  }
}

function hasUsableCostItem(payload) {
  return asArray(payload).some((item) => item && typeof item === "object" && !item.error);
}

/**
 * Codex cost scans with the bundled Rust binary first: upstream
 * `codexbar cost` walks Codex history in Swift and SIGSEGVs on Linux. When the
 * native scan comes back missing or all-errors (an old bundled binary without
 * the Codex scanner, or a scan failure), make ONE upstream
 * `codexbar cost --provider codex` attempt — today's behavior, including
 * SIGSEGV surfacing — rather than going blank. Whatever the fallback returns
 * (usable items or its error payload) is the Codex cost result.
 */
function fetchNativeCodexCostWithUpstreamFallback(nativeCommand, nativeCommandArgs) {
  const nativeItems = asArray(runCostCommand(nativeCommand, nativeCommandArgs, "codex", "native"));
  if (hasUsableCostItem(nativeItems)) {
    return nativeItems;
  }
  const upstreamCommandArgs = [
    "cost",
    "--format",
    "json",
    "--json-only",
    "--provider",
    "codex",
  ];
  const upstreamRunArgs = forceRefresh
    ? [...upstreamCommandArgs, "--refresh"]
    : upstreamCommandArgs;
  return runCostCommand(currentCliPath(), upstreamRunArgs, "codex", "codexbar");
}

const COST_WORKER_MIN_INTERVAL_MS = 5 * 60 * 1000;

function costScanMarkerPath() {
  return path.join(sharedProviderCacheDir(), "cost-scan.requested");
}

/**
 * Spawn at most one detached cost scan. The marker is the interlock: every
 * widget that reads a stale cost cache calls this, but only the first one wins,
 * and the worker clears the marker when it finishes.
 */
function spawnCostWorker() {
  const cacheDir = sharedProviderCacheDir();
  fs.mkdirSync(cacheDir, { recursive: true, mode: 0o700 });
  const marker = costScanMarkerPath();
  try {
    const stat = fs.statSync(marker);
    if (Date.now() - stat.mtimeMs < COST_WORKER_MIN_INTERVAL_MS) {
      return;
    }
  } catch {
    // No scan requested recently; fall through and request one.
  }
  fs.writeFileSync(marker, `${process.pid}\n${Date.now()}\n`, { mode: 0o600 });
  try {
    const child = spawn(process.execPath, [
      fileURLToPath(import.meta.url),
      ...rawArgs,
      "--cost-worker",
      "true",
    ], { detached: true, stdio: "ignore", env: process.env });
    child.unref();
  } catch {
    try { fs.unlinkSync(marker); } catch {}
  }
}

/**
 * Detached cost pass: the full scan with a long timeout, writing the shared
 * cost caches the widget reads. Clears the scan marker on the way out.
 */
function runCostWorker() {
  execTimeoutMs = Math.max(timeoutMs, costWorkerTimeoutMs);
  try {
    runCost({ synchronous: true });
  } finally {
    try { fs.unlinkSync(costScanMarkerPath()); } catch {}
  }
}

/**
 * Shared per-provider cache. Fresh successful results are reused for
 * `cacheSeconds`; a failed refresh keeps the last good payload on screen and
 * opens a short cooldown instead of replaying the error for the whole window.
 * `cacheSeconds <= 0` (sharing disabled) bypasses the cache entirely, which
 * also drops the stale-good safety net by design.
 */
function sharedFetch(namespace, identity, producer, cacheSeconds = sharedCacheSeconds, options = {}) {
  if (cacheSeconds <= 0) {
    return producer();
  }

  const cacheDir = sharedProviderCacheDir();
  fs.mkdirSync(cacheDir, { recursive: true, mode: 0o700 });
  const cacheKey = crypto.createHash("sha256")
    .update(JSON.stringify(identity))
    .digest("hex");
  const cachePath = path.join(cacheDir, `${namespace}-${cacheKey}.json`);

  return fetchWithResilience({
    cachePath,
    producer,
    cacheSeconds,
    timeoutMs,
    cooldownCapMs,
    forceRefresh,
    requestStartedAt,
    ...options,
  });
}

function sharedProviderCacheDir() {
  const cacheHome = clean(process.env.XDG_CACHE_HOME) || path.join(os.homedir(), ".cache");
  return path.join(cacheHome, "codexbar-plasmoid", "provider-cache");
}

function hashSecret(value) {
  const secret = clean(value);
  return secret ? crypto.createHash("sha256").update(secret).digest("hex") : "";
}

// ---------------------------------------------------------------------------
// Sync server: publish this device's local fetches, and serve merged entries
// for provider rows configured with the "remote" source.
// ---------------------------------------------------------------------------

function sanitizeSyncDevice(raw) {
  const trimmed = clean(raw);
  return /^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/.test(trimmed) ? trimmed : "";
}

function suggestedDeviceName() {
  // Hostnames carry no secrets, but they can contain dots and other
  // characters the server's device-id validation rejects; strip down here.
  return (os.hostname() || "").split(".")[0] || "";
}

function buildSyncUrl() {
  if (!syncUrl) {
    return null;
  }
  try {
    const url = new URL(syncUrl.includes("://") ? syncUrl : `http://${syncUrl}`);
    url.pathname = url.pathname.replace(/\/+$/, "");
    return url.toString().replace(/\/+$/, "");
  } catch {
    return null;
  }
}

/**
 * One sync-server HTTP exchange. Bounded by the request timeout; resolves a
 * normalized result instead of throwing so publish/read failures degrade to
 * per-provider error cards (never to JSON breakage).
 */
async function syncHttpRequest(method, urlString, token, timeoutMs, requestBody = null) {
  let url;
  try {
    url = new URL(urlString);
  } catch {
    return { ok: false, status: 0, text: "", error: `sync server URL is not valid: ${urlString}` };
  }
  const transport = url.protocol === "https:" ? await import("node:https") : await import("node:http");
  return new Promise((resolve) => {
    let settled = false;
    const finish = (result) => {
      if (!settled) {
        settled = true;
        resolve(result);
      }
    };
    const hasBody = typeof requestBody === "string" && requestBody.length > 0;
    const headers = { Accept: "application/json" };
    if (token) {
      headers.Authorization = `Bearer ${token}`;
    }
    if (hasBody) {
      headers["Content-Type"] = "application/json";
      headers["Content-Length"] = String(Buffer.byteLength(requestBody, "utf8"));
    }
    const request = transport.request(
      {
        hostname: url.hostname,
        port: url.port || (url.protocol === "https:" ? 443 : 80),
        path: `${url.pathname}${url.search}`,
        method,
        headers,
      },
      (response) => {
        const chunks = [];
        response.on("data", (chunk) => chunks.push(chunk));
        response.on("end", () => {
          const status = Number(response.statusCode || 0);
          finish({ ok: status >= 200 && status < 300, status, text: chunks.join("") });
        });
      },
    );
    request.setTimeout(timeoutMs, () => {
      request.destroy();
      finish({ ok: false, status: 0, text: "", error: "sync server request timeout" });
    });
    request.on("error", (error) => {
      finish({ ok: false, status: 0, text: "", error: shortError(error) });
    });
    if (hasBody) {
      request.end(requestBody);
    } else {
      request.end();
    }
  });
}

/** Connection check for the settings UI: health (no auth) + device list (auth). */
async function testSyncServer() {
  const base = buildSyncUrl();
  if (!base) {
    return { ok: false, error: "Sync server URL missing" };
  }
  const timeout = Math.min(timeoutMs, 15000);
  const health = await syncHttpRequest("GET", `${base}/health`, "", timeout);
  if (!health.ok) {
    return { ok: false, error: health.error || `Server unreachable (status ${health.status})` };
  }
  let version = null;
  try {
    version = JSON.parse(health.text)?.version ?? null;
  } catch {
    version = null;
  }
  const devices = await syncHttpRequest("GET", `${base}/v1/devices`, syncToken, timeout);
  if (!devices.ok) {
    const hint = devices.status === 401
      ? "Server reachable, but the token was rejected"
      : (devices.error || `Device list failed (status ${devices.status})`);
    return { ok: false, version, device: syncDevice, error: hint };
  }
  let deviceCount = null;
  try {
    deviceCount = devices.text ? (JSON.parse(devices.text)?.devices?.length ?? null) : null;
  } catch {
    deviceCount = null;
  }
  return { ok: true, version, device: syncDevice, deviceCount };
}

/** Best-effort publish; failures never affect the data the widget renders. */
async function publishSnapshot(snapshot) {
  const base = buildSyncUrl();
  if (!base) {
    return { ok: false, error: "sync server URL missing" };
  }
  // Never publish server-sourced entries back and never publish failures:
  // a bad fetch must not clobber this device's last good snapshot.
  const publishEntries = asArray(snapshot.entries).filter((entry) =>
    entry && !entry.error && localPublishProviders().has(normalizeProviderId(entry.provider)),
  );
  if (publishEntries.length === 0) {
    return { ok: true, skipped: true };
  }
  const payload = { ...snapshot, entries: publishEntries };
  const target = `${base}/v1/snapshot/${encodeURIComponent(syncDevice)}`;
  const response = await syncHttpRequest(
    "PUT",
    target,
    syncToken,
    Math.min(timeoutMs, 20000),
    JSON.stringify(payload),
  );
  if (response.ok) {
    return { ok: true };
  }
  return {
    ok: false,
    error: response.error || `sync server publish failed with status ${response.status}`,
  };
}

function usesRemoteSource(config) {
  return resolveSource(config.provider, config.source) === "remote";
}

/**
 * Providers this device fetches locally right now. Evaluated lazily because
 * resolveSource needs linuxAutoFallbacks, which is initialized later in this
 * module than the parse-time constants.
 */
function localPublishProviders() {
  return new Set(
    effectiveProviderConfigs()
      .filter((config) => !usesRemoteSource(config))
      .map((config) => normalizeProviderId(config.provider)),
  );
}

async function fetchRemoteEntries() {
  const providers = new Set();
  const remoteConfigs = effectiveProviderConfigs().filter((config) => usesRemoteSource(config));
  const uniqueConfigs = [];
  for (const config of remoteConfigs) {
    const providerId = normalizeProviderId(config.provider);
    if (providers.has(providerId)) {
      continue;
    }
    providers.add(providerId);
    uniqueConfigs.push(config);
  }
  if (uniqueConfigs.length === 0) {
    return [];
  }
  const base = buildSyncUrl();
  const entries = [];
  for (const config of uniqueConfigs) {
    entries.push(...(await remoteEntriesForProvider(config, base)));
  }
  return entries;
}

/**
 * Server-state mode ("Sync all from server"): one snapshot built entirely from
 * the server's merged cross-device view. `GET /v1/usage` without a provider
 * filter returns every merged entry, so the widget can show providers this
 * device never fetched and never touched a credential.
 */
async function buildServerOnlySnapshot() {
  const entries = await fetchAllRemoteEntries();
  assignStableEntryIds(entries);
  return {
    ok: true,
    generatedAt: new Date().toISOString(),
    requestedProvider: "all",
    entries,
    costError: null,
    syncAllFromServer: true,
  };
}

async function fetchAllRemoteEntries() {
  const base = buildSyncUrl();
  if (!base) {
    return syncAllErrorEntries("Sync server URL missing");
  }
  const response = await syncHttpRequest(
    "GET",
    `${base}/v1/usage`,
    syncToken,
    Math.min(timeoutMs, 20000),
  );
  if (!response.ok) {
    return syncAllErrorEntries(
      response.error || `sync server request failed with status ${response.status}`,
    );
  }
  let parsed = null;
  try {
    parsed = JSON.parse(response.text);
  } catch {
    return syncAllErrorEntries("Sync server returned invalid JSON");
  }
  const entries = asArray(parsed?.entries)
    .map((entry) => (entry && typeof entry === "object" ? normalizeRemoteEntry(entry) : null))
    .filter((entry) => entry && !entry.error && clean(entry.provider));
  if (entries.length === 0) {
    return syncAllErrorEntries("Nothing published on the sync server yet");
  }
  return entries;
}

/**
 * Explain an empty server-state view with one card per configured provider (or
 * a single Codex card when nothing is configured) instead of a blank widget.
 */
function syncAllErrorEntries(message) {
  const providers = [];
  for (const config of effectiveProviderConfigs()) {
    if (config.enabled === false) {
      continue;
    }
    const id = normalizeProviderId(config.provider);
    if (id && id !== "demo" && !providers.includes(id)) {
      providers.push(id);
    }
  }
  if (providers.length === 0) {
    providers.push("codex");
  }
  return providers.map((id) => remoteUsageErrorEntry(id, message));
}

async function mergeRemoteEntriesIntoSnapshot(snapshot) {
  const remoteEntries = await fetchRemoteEntries();
  if (remoteEntries.length === 0) {
    return;
  }
  // A provider row with the remote source governs its provider entirely,
  // so duplicate local cards for the same provider are dropped.
  const remoteProviders = new Set(
    remoteEntries.map((entry) => normalizeProviderId(entry.provider)),
  );
  snapshot.entries = [
    ...asArray(snapshot.entries).filter(
      (entry) => !remoteProviders.has(normalizeProviderId(entry.provider)),
    ),
    ...remoteEntries,
  ];
  assignStableEntryIds(snapshot.entries);
}

async function remoteEntriesForProvider(config, base) {
  const providerId = normalizeProviderId(config.provider);
  if (!base) {
    return [remoteUsageErrorEntry(providerId, "Sync server URL missing")];
  }
  const target = `${base}/v1/usage?provider=${encodeURIComponent(providerId)}`;
  try {
    const response = await syncHttpRequest("GET", target, syncToken, Math.min(timeoutMs, 20000));
    if (!response.ok) {
      throw new Error(
        response.error || `sync server request failed with status ${response.status}`,
      );
    }
    const parsed = JSON.parse(response.text);
    const entries = asArray(parsed.entries)
      .map((entry) => (entry && typeof entry === "object" ? normalizeRemoteEntry(entry) : null))
      .filter(
        (entry) =>
          entry && !entry.error && normalizeProviderId(entry.provider) === providerId,
      );
    if (entries.length === 0) {
      return [
        remoteUsageErrorEntry(
          providerId,
          "Nothing published on the sync server for this provider yet",
        ),
      ];
    }
    const wantedAccount = clean(config.account).toLowerCase();
    if (wantedAccount) {
      const matching = entries.filter(
        (entry) => clean(entry.account).toLowerCase() === wantedAccount,
      );
      // Lenient matching: the server may hold entries under another
      // publisher's anonymized label; keeping everything beats showing
      // nothing.
      if (matching.length > 0) {
        return matching;
      }
    }
    return entries;
  } catch (error) {
    return [remoteUsageErrorEntry(providerId, shortError(error))];
  }
}

/**
 * The server returns already-normalized entries; fill any missing field so
 * the merged shape matches what normalizeProvider emits for local rows.
 */
function normalizeRemoteEntry(entry) {
  const defaults = {
    provider: entry.provider || "unknown",
    account: entry.account ?? null,
    organization: entry.organization ?? null,
    plan: entry.plan ?? null,
    source: entry.source || "remote",
    siteUrl: entry.siteUrl ?? null,
    version: entry.version ?? null,
    updatedAt: entry.updatedAt || new Date().toISOString(),
    status: entry.status ?? null,
    error: entry.error ?? null,
    rows: asArray(entry.rows),
    creditsRemaining: entry.creditsRemaining ?? null,
    limitResetCredits: entry.limitResetCredits ?? null,
    codeReviewRemainingPercent: entry.codeReviewRemainingPercent ?? null,
    tokenUsage: entry.tokenUsage ?? null,
    dailyUsage: asArray(entry.dailyUsage),
    historySource: entry.historySource ?? entry.tokenUsage?.historySource ?? null,
  };
  return { ...defaults, ...entry };
}

function remoteUsageErrorEntry(providerId, message) {
  return {
    provider: providerId,
    account: null,
    organization: null,
    plan: null,
    source: "remote",
    siteUrl: null,
    version: null,
    updatedAt: new Date().toISOString(),
    status: null,
    error: { message },
    rows: [],
    creditsRemaining: null,
    limitResetCredits: null,
    codeReviewRemainingPercent: null,
    tokenUsage: null,
    dailyUsage: [],
  };
}

function effectiveProviderConfigs() {
  if (providerConfigs.length > 0) {
    return providerConfigs;
  }
  if (provider === "all" && source === "auto") {
    return discoverInstalledAgents();
  }
  return [];
}

function discoverInstalledAgents() {
  const home = os.homedir();
  const configHome = clean(process.env.XDG_CONFIG_HOME) || path.join(home, ".config");
  const candidates = [
    { provider: "codex", commands: ["codex"], paths: [path.join(home, ".codex")] },
    { provider: "claude", commands: ["claude"], paths: [path.join(home, ".claude")] },
    { provider: "codebuddy", commands: ["codebuddy"], paths: [path.join(home, ".codebuddy"), path.join(home, ".local", "share", "CodeBuddyExtension")] },
    { provider: "commandcode", commands: ["cmd", "command-code"], paths: [path.join(home, ".commandcode")] },
    { provider: "cursor", commands: ["cursor"], paths: [path.join(configHome, "Cursor"), path.join(home, ".cursor")] },
    { provider: "antigravity", commands: ["antigravity", "agy"], paths: [path.join(configHome, "Antigravity"), path.join(configHome, "antigravity"), path.join(configHome, "antigravity-usage")] },
    { provider: "augment", commands: ["augment"], paths: [path.join(home, ".augment")] },
    { provider: "factory", commands: ["droid"], paths: [path.join(home, ".factory")] },
    { provider: "jetbrains", commands: [], paths: [path.join(configHome, "JetBrains")] },
    { provider: "kiro", commands: ["kiro"], paths: [path.join(home, ".kiro"), path.join(configHome, "Kiro")] },
    { provider: "grok", commands: ["grok"], paths: [path.join(home, ".grok")] },
    { provider: "opencode", commands: ["opencode"], paths: [path.join(configHome, "opencode")] },
    { provider: "windsurf", commands: ["windsurf"], paths: [path.join(home, ".codeium", "windsurf"), path.join(configHome, "Windsurf")] },
  ];
  const discovered = candidates.filter((candidate) =>
    candidate.commands.some(commandExists) || candidate.paths.some((candidatePath) => fs.existsSync(candidatePath))
  );
  const agents = discovered.length > 0 ? discovered : [candidates[0]];
  return agents.map((candidate) => ({
    provider: candidate.provider,
    source: "auto",
    account: "",
    accountIndex: 0,
    allAccounts: false,
  }));
}

function commandExists(command) {
  const pathEntries = clean(process.env.PATH).split(path.delimiter).filter(Boolean);
  return pathEntries.some((directory) => {
    const candidate = path.join(directory, command);
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      return true;
    } catch {
      return false;
    }
  });
}

function runJSON(command, commandArgs, providerId = "", apiKey = "", account = "", extraEnv = {}) {
  const invocation = resolveCommandInvocation(command);
  let stdout = "";
  try {
    stdout = execFileSync(invocation.command, [...invocation.prefix, ...commandArgs], {
      encoding: "utf8",
      env: { ...cliEnvForProvider(providerId, apiKey, account), ...(extraEnv || {}) },
      stdio: ["ignore", "pipe", "pipe"],
      timeout: execTimeoutMs,
      windowsHide: true,
    });
  } catch (error) {
    const partial = parseJSONOrNull(error?.stdout?.toString?.());
    if (partial !== null) {
      return partial;
    }
    throw error;
  }
  const parsed = parseJSONOrNull(stdout);
  return parsed === null ? [] : parsed;
}

/** Parse CLI stdout defensively; malformed output becomes "no data". */
function parseJSONOrNull(text) {
  const trimmed = clean(text);
  if (!trimmed) {
    return null;
  }
  try {
    return JSON.parse(trimmed);
  } catch {
    return null;
  }
}

function normalizeSnapshot(usagePayload, costPayload, costError = null) {
  const costByProvider = new Map();
  for (const item of asArray(costPayload)) {
    if (!item || typeof item.provider !== "string" || item.error) {
      continue;
    }
    // Prefer the first successful cost payload per provider id.
    if (!costByProvider.has(item.provider)) {
      costByProvider.set(item.provider, item);
    }
  }

  const costEnabled = costEnabledProviders();

  const entries = asArray(usagePayload).map((item) => {
    const providerId = item.provider || "unknown";
    const cost = costEnabled.has(normalizeProviderId(providerId))
      ? costByProvider.get(providerId)
      : null;
    return normalizeProvider(item, cost);
  });

  // For each provider, keep only working accounts. If every account failed,
  // retain a single error entry so the user can see why nothing loaded.
  const visibleByProvider = new Map();
  for (const entry of entries) {
    const bucket = visibleByProvider.get(entry.provider) || { successes: [], firstError: null };
    if (entry.error) {
      if (!bucket.firstError) bucket.firstError = entry;
    } else {
      bucket.successes.push(entry);
    }
    visibleByProvider.set(entry.provider, bucket);
  }
  const filteredEntries = [];
  for (const bucket of visibleByProvider.values()) {
    if (bucket.successes.length > 0) {
      filteredEntries.push(...bucket.successes);
    } else if (bucket.firstError) {
      filteredEntries.push(bucket.firstError);
    }
  }

  // Surface local cost even when usage failed (or was never configured).
  // Prefer replacing a pure error card with cost-backed stats over leaving
  // the user with only a red failure for providers that still have spend.
  const successProviders = new Set(
    filteredEntries.filter((entry) => !entry.error).map((entry) => entry.provider),
  );
  const presentProviders = new Set(filteredEntries.map((entry) => entry.provider));
  for (const [providerId, cost] of costByProvider) {
    if (providerId === "cost" || providerId === "cli") {
      continue;
    }
    if (!costEnabled.has(normalizeProviderId(providerId))) {
      continue;
    }
    if (!presentProviders.has(providerId)) {
      filteredEntries.push(normalizeProvider({ provider: providerId, source: "local" }, cost));
      presentProviders.add(providerId);
      successProviders.add(providerId);
      continue;
    }
    if (!successProviders.has(providerId)) {
      const index = filteredEntries.findIndex((entry) => entry.provider === providerId && entry.error);
      if (index >= 0) {
        // The card falls back to local spend because the limits fetch failed.
        // Carry the reason so it can say so instead of looking like a normal
        // cost card with no bars.
        filteredEntries[index] = normalizeProvider({
          provider: providerId,
          source: "local",
          usageUnavailable: filteredEntries[index].error || null,
        }, cost);
        successProviders.add(providerId);
      }
    }
  }

  // Assign stable ids after filtering so selection survives Plasma restarts.
  // Never key off payload order — that changes when providers fail/succeed.
  assignStableEntryIds(filteredEntries);

  return {
    ok: true,
    generatedAt: new Date().toISOString(),
    requestedProvider: provider,
    entries: filteredEntries,
    costError: clean(costError) || null,
  };
}

function costEnabledProviders() {
  const configs = effectiveProviderConfigs();
  if (configs.length === 0) {
    // No explicit list: allow every cost backend we know about.
    return new Set([...CODEXBAR_COST_PROVIDERS, ...NATIVE_COST_PROVIDERS]);
  }
  const enabled = new Set();
  for (const config of configs) {
    if (config.includeCost === false) {
      continue;
    }
    enabled.add(normalizeProviderId(config.provider));
  }
  return enabled;
}

/**
 * Build a stable entry id for the provider switcher / selection restore.
 * Prefer account (anonymized when enabled); otherwise provider+source.
 * Collisions among multi-account providers get a numeric disambiguator.
 */
function assignStableEntryIds(entries) {
  const baseCounts = new Map();
  for (const entry of entries) {
    const base = stableEntryIdBase(entry);
    const count = baseCounts.get(base) || 0;
    baseCounts.set(base, count + 1);
    entry.id = count === 0 ? base : `${base}#${count}`;
  }
}

function stableEntryIdBase(entry) {
  const providerId = clean(entry?.provider) || "unknown";
  const account = clean(entry?.account);
  if (account) {
    return `${providerId}:${account}`;
  }
  const source = clean(entry?.source);
  if (source) {
    return `${providerId}:${source}`;
  }
  return providerId;
}


function normalizeProvider(item, cost) {
  const providerId = item.provider || "unknown";
  const usage = item.usage || {};
  const dashboard = item.openaiDashboard || {};
  const identity = usage.identity || {};
  const source = item.source || "unknown";
  const rows = usageRows(providerId, usage, source, item.pace || {});
  const dailyUsage = annotateLimitResets(dailyUsagePoints(dashboard, cost), rows);
  const rawAccount = item.account || usage.accountEmail || identity.accountEmail || null;
  const account = anonymizeEmails ? anonymizeIdentity(rawAccount) : rawAccount;
  const rawOrganization = usage.accountOrganization || identity.accountOrganization || null;
  const organization = anonymizeEmails ? anonymizeIdentity(rawOrganization) : rawOrganization;

  // Credits can arrive in several shapes depending on CLI / plugin version:
  // - item.credits.remaining / usage.openRouterUsage.balance (legacy)
  // - usage.primary.resetDescription like "$8.56 remaining"
  // - usage.details[{title:"Credits", rows:[{label:"Remaining", value:"$8.56"}]}]
  // - usage.loginMethod / identity.loginMethod like "Balance: $8.56" (0.48+ plugins)
  let creditsRemaining = numberOrNull(item.credits?.remaining ?? usage.openRouterUsage?.balance);
  if (creditsRemaining === null) {
    creditsRemaining = creditsFromUsageDetails(usage);
  }
  if (creditsRemaining === null && source === "api" && usage.primary?.resetDescription) {
    creditsRemaining = parseBalanceFromDescription(usage.primary.resetDescription);
  }
  if (creditsRemaining === null) {
    creditsRemaining = parseBalanceFromDescription(usage.loginMethod || identity.loginMethod);
  }

  const itemSiteUrl = typeof item.siteUrl === "string" ? item.siteUrl : null;
  const tokenUsage = buildTokenUsage(cost, usage);
  const limitResetCredits = normalizeLimitResetCredits(usage);  // A stale entry is last known good data whose refresh failed. The card keeps
  // rendering it and shows a soft retry note instead of a red error; the failed
  // attempt is kept only for the tooltip. Cost refreshes in the background by
  // design, so a stale cost cache must not mark fresh usage limits as stale.
  const stale = item.stale === true;
  return {
    provider: providerId,
    account,
    organization,
    plan: usage.loginMethod || identity.loginMethod || null,
    source,
    siteUrl: itemSiteUrl || configuredProviderSiteUrl(providerId),
    version: item.version || null,
    updatedAt: usage.updatedAt || item.credits?.updatedAt || cost?.updatedAt || new Date().toISOString(),
    status: item.status || null,
    error: item.error || null,
    stale,
    staleSince: stale ? (item.staleSince || cost?.staleSince || null) : null,
    lastAttemptError: stale ? (item.lastAttemptError || cost?.lastAttemptError || null) : null,
    usageUnavailable: item.usageUnavailable || null,
    rows,
    creditsRemaining,
    limitResetCredits,
    codeReviewRemainingPercent: numberOrNull(dashboard.codeReviewRemainingPercent),
    tokenUsage,
    dailyUsage,
    // Entry-level fallback for the sync-server merge when individual days
    // predate fingerprints (dashboard dailyBreakdown implies cloud).
    historySource: tokenUsage?.historySource
      || (Array.isArray(dashboard.dailyBreakdown) && dashboard.dailyBreakdown.length > 0 ? "cloud" : null),
  };
}

/**
 * Codex (and similar) grant free rate-limit reset credits, exposed on usage as
 * codexResetCredits. Normalize for the widget as limitResetCredits.
 */
function normalizeLimitResetCredits(usage) {
  const raw = usage?.codexResetCredits || usage?.limitResetCredits || null;
  if (!raw || typeof raw !== "object") {
    return null;
  }
  const items = asArray(raw.credits).map((credit) => {
    const status = clean(credit?.status) || "unknown";
    return {
      id: clean(credit?.id) || null,
      title: clean(credit?.title) || "Reset",
      description: clean(credit?.description) || null,
      status,
      resetType: clean(credit?.reset_type || credit?.resetType) || null,
      grantedAt: credit?.granted_at || credit?.grantedAt || null,
      expiresAt: credit?.expires_at || credit?.expiresAt || null,
    };
  });
  const availableItems = items.filter((item) => String(item.status).toLowerCase() === "available");
  const availableCount = integerOrNull(raw.availableCount) ?? availableItems.length;
  let nextExpiresAt = null;
  for (const item of (availableItems.length > 0 ? availableItems : items)) {
    if (!item.expiresAt) {
      continue;
    }
    const ms = Date.parse(item.expiresAt);
    if (!Number.isFinite(ms)) {
      continue;
    }
    if (nextExpiresAt === null || ms < Date.parse(nextExpiresAt)) {
      nextExpiresAt = item.expiresAt;
    }
  }
  return {
    availableCount,
    nextExpiresAt,
    items,
    updatedAt: raw.updatedAt || raw.updated_at || null,
  };
}

function buildTokenUsage(cost, usage) {
  if (cost && !cost.error) {
    const sessionCostUSD = numberOrNull(cost.sessionCostUSD);
    const sessionTokens = integerOrNull(cost.sessionTokens);
    const last30DaysCostUSD = numberOrNull(cost.last30DaysCostUSD);
    const last30DaysTokens = integerOrNull(cost.last30DaysTokens);
    if (
      sessionCostUSD !== null
      || sessionTokens !== null
      || last30DaysCostUSD !== null
      || last30DaysTokens !== null
    ) {
      return {
        sessionCostUSD,
        sessionTokens,
        last30DaysCostUSD,
        last30DaysTokens,
        provenance: clean(cost.provenance || cost.totals?.provenance) || null,
        historyCoverageIsEstablished: typeof cost.historyCoverageIsEstablished === "boolean"
          ? cost.historyCoverageIsEstablished : null,
        currencyCode: cost.currencyCode || "USD",
        sessionLabel: cost.sessionLabel || "Today",
        last30DaysLabel: cost.last30DaysLabel || "30d",
        // Local log scans sum across devices; account-global dashboards
        // (Cursor) take freshest-wins on the server. The native cost scan
        // reports source "dashboard" for the latter, "local" otherwise.
        historySource: clean(cost.source) === "dashboard" ? "cloud" : "local",
      };
    }
  }

  // Cursor (and similar) expose a period spend on the usage payload when the
  // dedicated cost scanner has nothing local to aggregate.
  const providerCost = usage?.providerCost || usage?.provider_cost || null;
  if (providerCost) {
    const used = numberOrNull(providerCost.used);
    if (used !== null) {
      return {
        sessionCostUSD: null,
        sessionTokens: null,
        last30DaysCostUSD: used,
        last30DaysTokens: null,
        currencyCode: providerCost.currencyCode || providerCost.currency_code || "USD",
        sessionLabel: "Today",
        last30DaysLabel: providerCost.period || "Period",
      };
    }
  }

  return null;
}

function configuredProviderSiteUrl(providerId) {
  if (normalizeProviderId(providerId) !== "llmproxy") {
    return null;
  }
  const providerConfig = kdeProviderConfig.llmproxy || {};
  const rawUrl = clean(process.env.LLM_PROXY_BASE_URL) || clean(providerConfig.enterpriseHost);
  if (!rawUrl) {
    return null;
  }
  try {
    const withScheme = rawUrl.includes("://") ? rawUrl : `https://${rawUrl}`;
    const url = new URL(withScheme);
    const isLoopbackHttp = url.protocol === "http:"
      && (url.hostname === "127.0.0.1" || url.hostname === "localhost" || url.hostname === "[::1]");
    if (url.protocol !== "https:" && !isLoopbackHttp) {
      return null;
    }
    url.pathname = url.pathname.replace(/\/(?:v1\/quota-stats|v1)\/?$/, "") || "/";
    url.search = "";
    url.hash = "";
    return url.toString();
  } catch {
    return null;
  }
}

/**
 * CodexBar's per-window pace report: whether the remaining budget lasts until
 * the window resets at the current burn rate. Null when the CLI has none.
 */
function normalizePace(pace) {
  if (!pace || typeof pace !== "object") {
    return null;
  }
  const willLastToReset = typeof pace.willLastToReset === "boolean" ? pace.willLastToReset : null;
  const deltaPercent = numberOrNull(pace.deltaPercent);
  if (willLastToReset === null && deltaPercent === null) {
    return null;
  }
  return {
    willLastToReset,
    // Negative = budget in reserve versus the expected burn; positive = deficit.
    deltaPercent,
    expectedUsedPercent: numberOrNull(pace.expectedUsedPercent),
    etaSeconds: numberOrNull(pace.etaSeconds),
    // CodexBar's own prose, rendered verbatim by the widget. Locally computed
    // pace leaves this null so QML builds a translated line from the fields.
    summary: typeof pace.summary === "string" ? pace.summary : null,
  };
}

/**
 * Fallback pace when the CLI reports none for a window but we know its length
 * and reset time: compare budget used against the fraction of the window
 * elapsed, as CodexBar does for primary/secondary. Returns null when the data
 * cannot support a projection (no reset time, no window length, or a reset at
 * or beyond the full window length).
 */
function computePace(percentLeft, resetsAt, windowMinutes, now = Date.now()) {
  if (percentLeft === null || !resetsAt || !(windowMinutes > 0)) {
    return null;
  }
  const resetMs = new Date(resetsAt).getTime();
  if (!Number.isFinite(resetMs)) {
    return null;
  }
  const windowMs = windowMinutes * 60000;
  const elapsed = 1 - (resetMs - now) / windowMs;
  // A reset at or beyond the full window length means no window time has
  // elapsed, so there is no burn rate to project from. Report nothing rather
  // than a verdict the data cannot support.
  if (!(elapsed > 0)) {
    return null;
  }
  const elapsedFraction = Math.min(1, elapsed);
  const used = Math.max(0, Math.min(1, 1 - percentLeft / 100));
  const expectedUsedPercent = Math.round(elapsedFraction * 100);
  const deltaPercent = Math.round((used - elapsedFraction) * 100);
  const willLastToReset = used <= elapsedFraction;
  let etaSeconds = null;
  if (!willLastToReset && used > 0) {
    // Time until empty at the current average burn rate.
    etaSeconds = Math.max(0, Math.round(((1 - used) / (used / elapsedFraction)) * windowMs / 1000));
  }
  // No prose here: the widget builds the pace line with i18n() from these
  // fields, and CodexBar's own summary (when present) wins instead.
  return {
    willLastToReset,
    deltaPercent,
    expectedUsedPercent,
    etaSeconds,
    summary: null,
  };
}

/**
 * Scoped rate windows the CLI reports beyond the standard three, e.g. Claude's
 * per-model weekly limits ("Fable only"). Shape: { id, title, window }.
 *
 * Row ids are namespaced as `extra:<id>`: a row id keys tray-bar selection and
 * the per-window CLI pace lookup, so an unnamespaced scoped window could shadow
 * primary/secondary/tertiary and inherit their pace report. A window the CLI
 * reports twice is collapsed; the same id with different data keeps both rows
 * as `extra:<id>#n`.
 */
function extraRateWindowEntries(usage) {
  const entries = [];
  const seenIds = new Set();
  const seenWindows = new Set();
  for (const extra of Array.isArray(usage.extraRateWindows) ? usage.extraRateWindows : []) {
    if (!extra || typeof extra !== "object" || !extra.window) {
      continue;
    }
    const rawId = clean(extra.id) || clean(extra.title);
    if (!rawId) {
      continue;
    }
    const window = extra.window;
    const fingerprint = JSON.stringify([
      rawId,
      window.usedPercent,
      window.remainingPercent,
      window.resetsAt,
      window.windowMinutes,
    ]);
    if (seenWindows.has(fingerprint)) {
      continue;
    }
    seenWindows.add(fingerprint);
    let id = `extra:${rawId}`;
    for (let n = 2; seenIds.has(id); n += 1) {
      id = `extra:${rawId}#${n}`;
    }
    seenIds.add(id);
    entries.push([id, clean(extra.title) || rawId, window]);
  }
  return entries;
}

/** Remaining percent, preferring the CLI's explicit value over `100 - used`. */
function percentLeftFromWindow(window) {
  const remaining = numberOrNull(window?.remainingPercent);
  if (remaining !== null) {
    return remaining;
  }
  const used = numberOrNull(window?.usedPercent);
  return used === null ? null : Math.max(0, Math.min(100, 100 - used));
}

/** One usage bar row built from a CodexBar usage window. */
function windowUsageRow(id, title, window, paceReport) {
  const percentLeft = percentLeftFromWindow(window);
  const resetsAt = window?.resetsAt || null;
  const windowMinutes = numberOrNull(window?.windowMinutes);
  return {
    id,
    title,
    percentLeft,
    resetsAt,
    windowMinutes,
    pace: normalizePace(paceReport) || computePace(percentLeft, resetsAt, windowMinutes),
  };
}

function usageRows(providerId, usage, source, pace = {}) {
  // Scoped windows are rate windows the CLI reported explicitly, so they are
  // never treated as an API balance placeholder.
  const scoped = extraRateWindowEntries(usage)
    .map(([id, title, window]) => windowUsageRow(id, title, window, null))
    .filter((row) => row.percentLeft !== null);
  if (Array.isArray(usage.usageRows)) {
    const native = usage.usageRows.map((row) => ({
      id: String(row.id || row.title || "usage"),
      title: String(row.title || "Usage"),
      percentLeft: numberOrNull(row.percentLeft),
      resetsAt: row.resetsAt || null,
      windowMinutes: numberOrNull(row.windowMinutes),
      pace: normalizePace(row.pace)
        || computePace(numberOrNull(row.percentLeft), row.resetsAt || null, numberOrNull(row.windowMinutes)),
    })).filter((row) => row.percentLeft !== null);
    return native.concat(scoped);
  }

  const labels = providerLabels(providerId);
  const windows = [
    ["primary", labels.session, usage.primary],
    ["secondary", labels.weekly, usage.secondary],
    ["tertiary", labels.tertiary, usage.tertiary],
  ];

  const standard = windows.map(([id, title, window]) => {
    const row = windowUsageRow(id, title, window, pace?.[id]);
    // API balance placeholders have no reset time or window duration. A real
    // window can omit its reset time, such as ClinePass's monthly quota.
    if (source === "api" && !row.resetsAt && !(row.windowMinutes > 0) && row.percentLeft !== null) {
      return null;
    }
    return row;
  }).filter((row) => row !== null && row.percentLeft !== null);

  return standard.concat(scoped);
}

function parseBalanceFromDescription(description) {
  if (typeof description !== "string") {
    return null;
  }
  // Accept "$8.56 …", "Balance: $8.56", "Remaining $8.56", or a bare amount.
  const match = description.match(/\$\s*([\d,]+(?:\.\d+)?)/) || description.match(/^([\d,]+(?:\.\d+)?)\s*$/);
  if (!match) {
    return null;
  }
  return numberOrNull(match[1].replace(/,/g, ""));
}

/**
 * CodexBar 0.48+ JS provider plugins (OpenRouter, OpenAI, …) put credit
 * balances in usage.details instead of usage.primary / openRouterUsage.
 */
function creditsFromUsageDetails(usage) {
  for (const section of asArray(usage?.details)) {
    const title = clean(section?.title).toLowerCase();
    if (title && title !== "credits" && title !== "balance" && !title.includes("credit")) {
      continue;
    }
    for (const row of asArray(section?.rows)) {
      const label = clean(row?.label).toLowerCase();
      if (label === "remaining" || label === "balance" || label === "credits remaining") {
        const parsed = parseBalanceFromDescription(typeof row?.value === "string" ? row.value : String(row?.value ?? ""));
        if (parsed !== null) {
          return parsed;
        }
      }
    }
  }
  return null;
}

function dailyUsagePoints(dashboard, cost) {
  const byDay = new Map();
  let hasSource = false;

  if (Array.isArray(dashboard.dailyBreakdown) && dashboard.dailyBreakdown.length > 0) {
    hasSource = true;
    for (const day of dashboard.dailyBreakdown) {
      mergeDailyPoint(byDay, day.day, {
        totalTokens: integerOrNull(day.totalTokens),
        costUSD: numberOrNull(day.totalCreditsUsed),
        models: normalizeModelBreakdowns(day.modelBreakdowns || day.models),
        // Usage dashboards are account-global: every device sees the same
        // numbers, so the server takes freshest-wins instead of summing.
        historySource: "cloud",
        historyFingerprint: clean(day.historyFingerprint || day.fingerprint) || null,
      });
    }
  } else if (cost && !cost.error && (Array.isArray(cost.daily) || cost.last30DaysCostUSD != null || cost.sessionCostUSD != null)) {
    hasSource = true;
    const costHistorySource = clean(cost.source) === "dashboard" ? "cloud" : "local";
    for (const day of asArray(cost.daily)) {
      mergeDailyPoint(byDay, day.date, {
        totalTokens: integerOrNull(day.totalTokens),
        costUSD: numberOrNull(day.totalCost),
        models: normalizeModelBreakdowns(day.modelBreakdowns || day.models),
        historySource: costHistorySource,
        historyFingerprint: clean(day.historyFingerprint || day.fingerprint) || null,
      });
    }
  }

  if (!hasSource) {
    return [];
  }

  // Always emit a continuous local-calendar window of the last 30 days so the
  // chart shows real calendar time (missing days are zero / flat bars).
  return padLast30Days(byDay);
}

function mergeDailyPoint(byDay, dayKey, next) {
  const key = clean(dayKey);
  if (!key) {
    return;
  }
  const existing = byDay.get(key) || {
    dayKey: key,
    totalTokens: 0,
    costUSD: 0,
    models: [],
    historySource: next.historySource || null,
    historyFingerprint: next.historyFingerprint || null,
  };
  const tokens = next.totalTokens;
  const cost = next.costUSD;
  if (tokens !== null) {
    existing.totalTokens = (existing.totalTokens || 0) + tokens;
  }
  if (cost !== null) {
    existing.costUSD = (existing.costUSD || 0) + cost;
  }
  existing.models = mergeModelBreakdowns(existing.models, next.models || []);
  // Same-device merge of two origins (dashboard + local scan): fingerprints
  // only survive when both sides agree; sources survive when they agree.
  if ((existing.historyFingerprint || null) !== (next.historyFingerprint || null)) {
    existing.historyFingerprint = null;
  }
  if ((existing.historySource || null) !== (next.historySource || null)) {
    existing.historySource = null;
  }
  byDay.set(key, existing);
}

function normalizeModelBreakdowns(raw) {
  if (!Array.isArray(raw)) {
    return [];
  }
  return raw
    .map((item) => {
      const name = clean(item?.modelName || item?.model || item?.name || item?.id);
      if (!name) {
        return null;
      }
      return {
        name,
        costUSD: numberOrNull(item.cost ?? item.costUSD ?? item.totalCost) ?? 0,
        totalTokens: integerOrNull(item.totalTokens ?? item.tokens) ?? 0,
      };
    })
    .filter(Boolean);
}

function mergeModelBreakdowns(left, right) {
  const byName = new Map();
  for (const item of [...(left || []), ...(right || [])]) {
    const name = clean(item?.name);
    if (!name) {
      continue;
    }
    const existing = byName.get(name) || { name, costUSD: 0, totalTokens: 0 };
    existing.costUSD += Number(item.costUSD) || 0;
    existing.totalTokens += Number(item.totalTokens) || 0;
    byName.set(name, existing);
  }
  return [...byName.values()]
    .map((item) => ({
      name: item.name,
      costUSD: Math.round(item.costUSD * 1_000_000) / 1_000_000,
      totalTokens: item.totalTokens,
    }))
    .sort((a, b) => (b.costUSD - a.costUSD) || (b.totalTokens - a.totalTokens) || a.name.localeCompare(b.name));
}

function padLast30Days(byDay) {
  const days = [];
  const today = startOfLocalDay(new Date());
  for (let offset = 29; offset >= 0; offset -= 1) {
    const date = new Date(today);
    date.setDate(today.getDate() - offset);
    const dayKey = formatLocalDayKey(date);
    const existing = byDay.get(dayKey);
    days.push({
      dayKey,
      totalTokens: existing?.totalTokens ?? 0,
      costUSD: existing?.costUSD ?? 0,
      models: existing?.models || [],
      limitResets: [],
      ...(existing?.historyFingerprint ? { historyFingerprint: existing.historyFingerprint } : {}),
      ...(existing?.historySource ? { historySource: existing.historySource } : {}),
    });
  }
  return days;
}

function startOfLocalDay(date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

function formatLocalDayKey(date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function dayKeyFromTimestamp(value) {
  if (!value) {
    return null;
  }
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) {
    return null;
  }
  return formatLocalDayKey(date);
}

/**
 * Attach unused rate-limit resets for the chart hover popup.
 *
 * - If a window's resetsAt falls on a day inside the 30-day window, mark that day.
 * - Always attach upcoming unused resets onto *today* as well, so future resets
 *   (e.g. next week) still surface when hovering the current day.
 * Only windows with percentLeft > 0 count as "unused" limit resets.
 */
function annotateLimitResets(dailyUsage, rows) {
  if (!Array.isArray(dailyUsage) || dailyUsage.length === 0 || !Array.isArray(rows)) {
    return dailyUsage;
  }
  const byDay = new Map(dailyUsage.map((day) => [day.dayKey, day]));
  const todayKey = formatLocalDayKey(startOfLocalDay(new Date()));
  const today = byDay.get(todayKey) || dailyUsage[dailyUsage.length - 1];

  for (const row of rows) {
    const percentLeft = numberOrNull(row?.percentLeft);
    const resetsAt = row?.resetsAt || null;
    if (percentLeft === null || percentLeft <= 0 || !resetsAt) {
      continue;
    }
    const entry = {
      title: String(row.title || "Limit"),
      percentLeft,
      resetsAt,
    };
    const dayKey = dayKeyFromTimestamp(resetsAt);
    const onDay = dayKey ? byDay.get(dayKey) : null;
    if (onDay) {
      pushLimitReset(onDay, entry);
    }
    // Surface upcoming unused resets on today even when the reset day is outside
    // the trailing 30-day window (or after the series ends).
    if (today && (!onDay || onDay.dayKey !== today.dayKey)) {
      const resetMs = Date.parse(resetsAt);
      if (Number.isFinite(resetMs) && resetMs >= Date.now() - 60_000) {
        pushLimitReset(today, entry);
      }
    }
  }
  return dailyUsage;
}

function pushLimitReset(day, entry) {
  if (!Array.isArray(day.limitResets)) {
    day.limitResets = [];
  }
  const already = day.limitResets.some((item) => (
    item.title === entry.title && item.resetsAt === entry.resetsAt
  ));
  if (!already) {
    day.limitResets.push(entry);
  }
}

function providerLabels(providerId) {
  switch (providerId) {
    case "claude":
      return { session: "Session", weekly: "Weekly", tertiary: "Opus" };
    case "clinepass":
      // Upstream ClinePass reports a 5-hour window, a weekly window, and a
      // monthly window (primaryBindingQuotaLanes: secondary + tertiary).
      return { session: "5-hour", weekly: "Weekly", tertiary: "Monthly" };
    case "codex":
      return { session: "Session", weekly: "Weekly", tertiary: "Long window" };
    case "kilo":
      return { session: "Credits", weekly: "Monthly", tertiary: "Extra" };
    case "cursor":
      return { session: "Total", weekly: "Auto + Composer", tertiary: "API" };
    case "antigravity":
      // Current native responses provide titled usageRows. Older generic CLI
      // windows do not identify models, so avoid inventing model names here.
      return { session: "Limit 1", weekly: "Limit 2", tertiary: "Limit 3" };
    case "opencode":
      return { session: "Rolling Usage", weekly: "Weekly Usage", tertiary: "Extra" };
    case "opencodego":
      return { session: "Rolling Usage", weekly: "Weekly Usage", tertiary: "Monthly Usage" };
    case "devin":
      return { session: "Daily", weekly: "Weekly", tertiary: "Extra" };
    case "grok":
      // xAI UI labels this "Weekly SuperGrok Limit"; primary window is the weekly pool.
      return { session: "Weekly", weekly: "Weekly", tertiary: "Extra" };
    case "codebuddy":
      // Every CodeBuddy allowance is a monthly credit cycle and the native
      // fetcher titles each row after its package, so these labels are only a
      // fallback for generic responses.
      return { session: "Monthly", weekly: "Weekly", tertiary: "Extra" };
    case "commandcode":
      // /alpha/billing/credits reports a rolling 5-hour and a weekly credit
      // window; the native helper rebuilds the plan's monthly allowance as the
      // tertiary window.
      return { session: "5-hour", weekly: "Weekly", tertiary: "Monthly" };
    case "demo":
      return { session: "Low", weekly: "Session", tertiary: "Weekly" };
    default:
      return { session: "Session", weekly: "Weekly", tertiary: "Extra" };
  }
}

function parseProviderConfigs(raw) {
  let decoded = [];
  try {
    decoded = JSON.parse(clean(raw) || "[]");
  } catch {
    decoded = [];
  }
  if (!Array.isArray(decoded)) {
    return [];
  }
  return decoded
    .filter((item) => item && item.enabled !== false)
    .map((item) => ({
      provider: clean(item.provider) || "codex",
      source: clean(item.source) || "auto",
      account: clean(item.account),
      accountIndex: Number(item.accountIndex || 0),
      allAccounts: item.allAccounts === true,
      apiKey: clean(item.apiKey),
      // Default on: users opt out per provider when they do not want spend stats.
      includeCost: item.includeCost !== false,
      _fromConfigs: true,
    }));
}

function providerConfigInput() {
  const appletId = clean(args.appletId || args["applet-id"]);
  if (!appletId) {
    return args.providers;
  }
  const stored = readAppletConfigValue(appletId, "providerConfigs");
  return stored === null ? args.providers : stored;
}

function readAppletConfigValue(appletId, key) {
  if (!/^[A-Za-z0-9._-]+$/.test(appletId)) {
    return null;
  }
  const configHome = clean(process.env.XDG_CONFIG_HOME) || path.join(os.homedir(), ".config");
  const configFileName = "plasma-org.kde.plasma.desktop-appletsrc";
  const configPath = path.join(configHome, configFileName);
  let raw;
  try {
    raw = fs.readFileSync(configPath, "utf8");
  } catch {
    return null;
  }

  const escapedId = appletId.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const sectionPattern = new RegExp(
    `^\\[Containments\\]\\[([^\\]]+)\\]\\[Applets\\]\\[${escapedId}\\]\\[Configuration\\]\\[General\\]$`,
    "m",
  );
  const section = sectionPattern.exec(raw);
  if (!section) {
    return null;
  }

  try {
    return execFileSync("kreadconfig6", [
      "--file", configFileName,
      "--group", "Containments",
      "--group", section[1],
      "--group", "Applets",
      "--group", appletId,
      "--group", "Configuration",
      "--group", "General",
      "--key", key,
    ], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      timeout: 5000,
    }).trim();
  } catch {
    return readKConfigValueFromSection(raw, section, key);
  }
}

function readKConfigValueFromSection(raw, section, key) {
  const sectionStart = section.index + section[0].length;
  const remainder = raw.slice(sectionStart);
  const nextSection = remainder.search(/^\[/m);
  const body = nextSection === -1 ? remainder : remainder.slice(0, nextSection);
  const prefix = `${key}=`;
  const line = body.split(/\r?\n/).find((item) => item.startsWith(prefix));
  return line === undefined ? null : decodeKConfigValue(line.slice(prefix.length));
}

function decodeKConfigValue(raw) {
  let decoded = "";
  for (let index = 0; index < raw.length; index += 1) {
    if (raw[index] !== "\\" || index + 1 >= raw.length) {
      decoded += raw[index];
      continue;
    }
    index += 1;
    const escaped = raw[index];
    if (escaped === "n") decoded += "\n";
    else if (escaped === "r") decoded += "\r";
    else if (escaped === "t") decoded += "\t";
    else decoded += escaped;
  }
  return decoded;
}

function loadSharedProviderConfigs(fallback) {
  const configHome = clean(process.env.XDG_CONFIG_HOME) || path.join(os.homedir(), ".config");
  const candidate = path.join(configHome, "codexbar-plasmoid", "shared-providers.json");
  try {
    if (fs.existsSync(candidate)) {
      const parsed = parseProviderConfigs(fs.readFileSync(candidate, "utf8"));
      if (parsed.length > 0) {
        return parsed;
      }
    }
  } catch {
    // A malformed shared file should not prevent a widget refresh.
  }
  return fallback;
}

function resolveSource(providerId, requestedSource) {
  const sourceMode = clean(requestedSource) || "auto";
  const normalized = normalizeProviderId(providerId);

  // Grok SuperGrok weekly limits need the native helper on Linux:
  // - codexbar `cli` hits a broken agent-stdio billing RPC ("Method not found")
  // - `oauth` is not a supported Grok source upstream
  // - `web` often yields cost-only local fallbacks without rate-limit windows
  // Without this remap, a saved source of "cli" shows spend charts but no
  // Weekly SuperGrok Limit bar (the plasmoid cost path still works).
  if (
    normalized === "grok"
    && process.platform !== "darwin"
    && (sourceMode === "auto" || sourceMode === "cli" || sourceMode === "oauth" || sourceMode === "web")
  ) {
    return "native";
  }

  if (process.platform === "darwin" || sourceMode !== "auto") {
    return sourceMode;
  }
  const fallback = linuxAutoFallbacks[normalized];
  return fallback || sourceMode;
}

function usesNativeCli(providerId, requestedSource) {
  const resolved = resolveSource(providerId, requestedSource);
  return (
    (resolved === "native" || resolved === "native-auth") &&
    nativeProviders.has(normalizeProviderId(providerId))
  );
}

function commandForConfig(config, requestedSource) {
  if (usesNativeCli(config.provider, requestedSource ?? config.source)) {
    return nativeCliPath;
  }
  return currentCliPath();
}

function resolveCommandInvocation(command) {
  return { command, prefix: [] };
}

function managedBinary() {
  return path.join(os.homedir(), ".local", "share", "codexbar-plasmoid", "bin", "codexbar");
}

function resolveEffectiveCliPath(requestedCli, enabledAutoUpdate, managed) {
  // If the user supplied an absolute or explicit relative path, honor it.
  if (requestedCli && requestedCli !== "codexbar") {
    return requestedCli;
  }
  // When auto-update is enabled, prefer the managed binary if it exists.
  if (enabledAutoUpdate && fs.existsSync(managed)) {
    return managed;
  }
  return requestedCli;
}

function resolveNativeCliPath() {
  const codeDir = path.dirname(fileURLToPath(import.meta.url));
  const candidates = [
    path.join(codeDir, "codexbar-plasmoid"),
    path.resolve(codeDir, "../../../native-cli/target/release/codexbar-plasmoid"),
  ];
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }
  return "codexbar-plasmoid";
}

/**
 * Builds the environment for one CLI subprocess.
 *
 * Swift cost scans run through Foundation on Linux; if no test-environment
 * key is present, CostUsageCustomPricing.isRunningTests falls through to
 * Bundle.allBundles and that CoreFoundation enumeration SIGSEGVs. Cost
 * callers pass SWIFT_TESTING=1 through runJSON's extraEnv so isRunningTests
 * returns before enumerating bundles. SWIFT_TESTING is intentionally used
 * instead of XCTestConfigurationFilePath: ProviderHTTPClient only recognizes
 * the XCTest* keys, and setting one of those would switch cost requests to
 * URLSession.shared. While SWIFT_TESTING is active, CodexBar skips the user
 * custom-pricing.json overlays; bundled list prices still apply.
 */
function cliEnvForProvider(providerId, apiKey, account = "") {
  const env = { ...process.env };
  const normalized = normalizeProviderId(providerId);
  const envName = providerApiKeyEnvName(providerId);
  const resolvedApiKey = clean(apiKey) || providerApiKey(providerId);
  if (envName && resolvedApiKey && !clean(env[envName])) {
    env[envName] = resolvedApiKey;
  }
  if (normalized === "devin" && clean(account) && !clean(env.DEVIN_ORGANIZATION) && !clean(env.DEVIN_ORG)) {
    env.DEVIN_ORGANIZATION = clean(account);
  }
  return env;
}

function providerApiKey(providerId) {
  const normalized = normalizeProviderId(providerId);
  const provider = kdeProviderConfig[normalized] || kdeProviderConfig[clean(providerId).toLowerCase()];
  return clean(provider?.apiKey);
}

function providerApiKeyEnvName(providerId) {
  switch (normalizeProviderId(providerId)) {
    case "azureopenai":
      return "AZURE_OPENAI_API_KEY";
    case "alibaba":
      return "ALIBABA_API_KEY";
    case "alibabatokenplan":
      return "ALIBABA_API_KEY";
    case "clinepass":
      return "CLINE_API_KEY";
    case "copilot":
      return "GITHUB_TOKEN";
    case "deepseek":
      return "DEEPSEEK_API_KEY";
    case "doubao":
      return "DOUBAO_API_KEY";
    case "gemini":
      return "GEMINI_API_KEY";
    case "groq":
      return "GROQ_API_KEY";
    case "kilo":
      return "KILO_API_KEY";
    case "kimik2":
      return "MOONSHOT_API_KEY";
    case "llmproxy":
      return "LLMPROXY_API_KEY";
    case "minimax":
      return "MINIMAX_API_KEY";
    case "moonshot":
      return "MOONSHOT_API_KEY";
    case "openai":
      return "OPENAI_API_KEY";
    case "openrouter":
      return "OPENROUTER_API_KEY";
    case "synthetic":
      return "SYNTHETIC_API_KEY";
    case "venice":
      return "VENICE_API_KEY";
    case "zai":
      return "ZAI_API_KEY";
    case "devin":
      return "DEVIN_BEARER_TOKEN";
    default:
      return "";
  }
}

function loadKdeProviderConfig() {
  const candidate = path.join(os.homedir(), ".codexbar", "config.json");
  try {
    if (fs.existsSync(candidate)) {
      const parsed = JSON.parse(fs.readFileSync(candidate, "utf8"));
      const providers = parsed?.providers;
      if (Array.isArray(providers)) {
        const normalized = {};
        for (const config of providers) {
          const providerId = clean(config?.id) || clean(config?.provider);
          if (providerId) {
            normalized[normalizeProviderId(providerId)] = config;
          }
        }
        return normalized;
      }
      if (providers && typeof providers === "object" && !Array.isArray(providers)) {
        const normalized = {};
        for (const [providerId, config] of Object.entries(providers)) {
          normalized[normalizeProviderId(providerId)] = config;
        }
        return normalized;
      }
    }
  } catch {
    // Missing or malformed optional config files should not block widget updates.
  }
  return {};
}

function normalizeProviderId(providerId) {
  const normalized = clean(providerId).toLowerCase().replace(/[-_]/g, "");
  const aliases = {
    azureopenai: "azureopenai",
    alibabacodingplan: "alibaba",
    alibabatokenplan: "alibabatokenplan",
    abacusai: "abacus",
    groqcloud: "groq",
    opencodego: "opencodego",
  };
  return aliases[normalized] || normalized;
}

function asArray(value) {
  if (Array.isArray(value)) {
    return value;
  }
  if (value && typeof value === "object") {
    return [value];
  }
  return [];
}

function clean(value) {
  return typeof value === "string" ? value.trim() : "";
}

function numberOrNull(value) {
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
}

function integerOrNull(value) {
  const number = Number(value);
  if (!Number.isFinite(number)) {
    return null;
  }
  return Number.isInteger(number) ? number : Math.round(number);
}

function errorSnapshot(error) {
  return {
    ok: false,
    generatedAt: new Date().toISOString(),
    requestedProvider: provider,
    entries: [],
    error: shortError(error),
  };
}

function shortError(error, command = currentCliPath()) {
  if (error?.signal === "SIGSEGV") {
    return "CodexBar CLI crashed with SIGSEGV during the command";
  }
  const stderr = clean(error?.stderr?.toString?.());
  if (stderr) {
    return stderr.split("\n").slice(-4).join("\n");
  }
  if (error?.code === "ENOENT") {
    return `CLI not found: ${command}`;
  }
  if (error?.signal === "SIGTERM" || error?.code === "ETIMEDOUT") {
    return `CodexBar CLI timed out after ${Math.round(timeoutMs / 1000)} seconds`;
  }
  return error?.message || String(error);
}

function anonymizeIdentity(value) {
  if (typeof value !== "string") {
    return value;
  }
  const trimmed = value.trim();
  if (!trimmed) {
    return value;
  }
  if (trimmed.includes("@")) {
    return anonymizeEmailAddress(trimmed);
  }
  if (looksLikeOpaqueId(trimmed)) {
    return anonymizeOpaqueId(trimmed);
  }
  return value;
}

function anonymizeEmailAddress(email) {
  if (typeof email !== "string" || !email.includes("@")) {
    return email;
  }
  const parts = email.split("@");
  if (parts.length !== 2) {
    return email;
  }
  const local = parts[0];
  const domain = parts[1];
  if (local.length <= 2) {
    return `${local[0]}*@${domain}`;
  }
  return `${local[0]}${"*".repeat(local.length - 2)}${local[local.length - 1]}@${domain}`;
}

function looksLikeOpaqueId(value) {
  if (isUuidLike(value)) {
    return true;
  }
  const alnum = value.replace(/[^A-Za-z0-9]/g, "");
  if (alnum.length < 16) {
    return false;
  }
  const hexCount = (alnum.match(/[0-9A-Fa-f]/g) || []).length;
  return hexCount * 2 >= alnum.length;
}

function isUuidLike(value) {
  const parts = value.split("-");
  if (parts.length !== 5) {
    return false;
  }
  const expected = [8, 4, 4, 4, 12];
  return parts.every((part, index) =>
    part.length === expected[index] && /^[0-9A-Fa-f]+$/.test(part)
  );
}

function anonymizeOpaqueId(value) {
  if (isUuidLike(value)) {
    const parts = value.split("-");
    return `${parts[0].slice(0, 4)}****-****-****-****-********${parts[4].slice(-4)}`;
  }
  if (value.length <= 8) {
    return `${value.slice(0, 1)}${"*".repeat(Math.max(0, value.length - 1))}`;
  }
  return `${value.slice(0, 4)}${"*".repeat(value.length - 8)}${value.slice(-4)}`;
}
