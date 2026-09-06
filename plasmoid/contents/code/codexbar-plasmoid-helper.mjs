#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const args = parseArgs(process.argv.slice(2));
// KDE's plasmoid process inherits the user's session env from systemd --user
// (which sources $XDG_CONFIG_HOME/environment.d/*.conf), but env vars th e
// user only set in their shell rc files (e.g. ~/.zshrc) do not make it
// across. Re-read the standard env.d directory plus a plasmoid-local
// ~/.codexbar/.env so API keys and other exports still reach the spawned
// CLIs. Values that were already on process.env when the helper started
// take precedence so explicit overrides still win.
loadEnvironmentFromFiles();
const timeoutMs = Math.max(5, Number(args.timeout || 45)) * 1000;
const cliPath = args.cli || process.env.CODEXBAR_CLI || "codexbar";
const nativeCliPath = args.nativeCli || process.env.CODEXBAR_NATIVE_CLI || resolveNativeCliPath();
const autoUpdate = args.autoUpdate === "true" || args["auto-update"] === "true";
const updateTag = clean(args.tag) || "latest";
const managedCliBinary = managedBinary();
const sharedCacheSeconds = Math.max(0, Number(args.cacheSeconds || args["cache-seconds"] || 0));
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
const localProviderConfigs = parseProviderConfigs(args.providers);
const syncProviders = args.syncProviders === "true" || args["sync-providers"] === "true";
const providerConfigs = syncProviders ? loadSharedProviderConfigs(localProviderConfigs) : localProviderConfigs;
const includeCost = args.cost !== "false";
const includeStatus = args.status !== "false";
const showCredits = args.credits !== "false";
const anonymizeEmails = args.anonymizeEmails !== "false" && args["anonymize-emails"] !== "false";
const kdeProviderConfig = loadKdeProviderConfig();

const nativeProviders = new Set(["antigravity", "cursor", "devin", "grok", "opencode", "opencodego"]);

// Upstream codexbar cost only scans Claude/Codex local logs. Native cost covers
// OpenCode SQLite, Cursor dashboard events, and Grok local session usage.
// Antigravity / Devin only expose quota percentages (no absolute token history).
const CODEXBAR_COST_PROVIDERS = new Set(["codex", "claude"]);
const NATIVE_COST_PROVIDERS = new Set(["opencode", "opencodego", "cursor", "grok"]);

const linuxAutoFallbacks = {
  codex: "cli",
  claude: "cli",
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
    snapshot.cliUpdate = updateResult;
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
  const items = configs.length > 0
    ? configs.flatMap((config) => asArray(runUsageForConfig(config)))
    : asArray(runUsageForConfig({
      provider,
      source,
      account: clean(args.account),
      accountIndex: Number(args.accountIndex || 0),
      allAccounts: args.allAccounts === "true",
    }));
  // codex-cli omits free rate-limit reset credits; oauth carries them. Merge when missing.
  return enrichCodexLimitResetCredits(items);
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
      const account = clean(item.account || item.usage?.accountEmail || item.usage?.identity?.accountEmail);
      const command = currentCliPath();
      const oauthPayload = sharedFetch("usage", {
        command,
        commandArgs,
        provider: "codex",
        apiKeyHash: "",
        account: "",
      }, () => runJSON(command, commandArgs, "codex", "", ""));
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

  const commandArgs = [
    "usage",
    "--format",
    "json",
    "--json-only",
    "--provider",
    config.provider,
    "--source",
    resolveSource(config.provider, config.source),
  ];

  if (includeStatus) {
    commandArgs.push("--status");
  }
  if (!showCredits) {
    commandArgs.push("--no-credits");
  }
  const isDevin = normalizeProviderId(config.provider) === "devin";
  if (!isDevin) {
    if (config.allAccounts === true) {
      commandArgs.push("--all-accounts");
    } else if (clean(config.account)) {
      commandArgs.push("--account", clean(config.account));
    } else if (Number(config.accountIndex || 0) > 0) {
      commandArgs.push("--account-index", String(Number(config.accountIndex)));
    }
  }

  const command = commandForConfig(config);
  try {
    return sharedFetch("usage", {
      command,
      commandArgs,
      provider: normalizeProviderId(config.provider),
      apiKeyHash: hashSecret(config.apiKey),
      account: clean(config.account),
    }, () => {
      try {
        return runJSON(command, commandArgs, config.provider, config.apiKey, clean(config.account));
      } catch (error) {
        return [{
          provider: normalizeProviderId(config.provider),
          source: resolveSource(config.provider, config.source),
          error: { message: shortError(error, command) },
        }];
      }
    });
  } catch (error) {
    return [{
      provider: normalizeProviderId(config.provider),
      source: resolveSource(config.provider, config.source),
      error: {
        message: shortError(error, command),
      },
    }];
  }
}

/**
 * Demo provider: fixed local sample data for widget UI checks.
 * Put comma-separated remaining percents in the account field, e.g. "1,11,35,72".
 * Defaults to low/mid values so tray bar fill and rounding are easy to verify.
 */
function buildDemoUsagePayload(config) {
  const now = new Date();
  const percents = parseDemoPercents(clean(config.account));
  const usageRows = percents.map((percentLeft, index) => {
    const title = demoRowTitle(index, percentLeft);
    return {
      id: `demo-${index + 1}`,
      title,
      percentLeft,
      resetsAt: new Date(now.getTime() + (index + 1) * 36e5 * 6).toISOString(),
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

function runCost() {
  const configs = (effectiveProviderConfigs().length > 0
    ? effectiveProviderConfigs()
    : [{ provider, includeCost: true }])
    .filter((config) => config.includeCost !== false);

  if (configs.length === 0) {
    return { items: [], costError: null };
  }

  const items = [];
  const costErrors = [];
  const collectCostResults = (payload) => {
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
  };
  const seenProviders = new Set();

  const codexbarProviders = uniqueProviderIds(configs.filter((config) =>
    CODEXBAR_COST_PROVIDERS.has(normalizeProviderId(config.provider))
  ));
  if (codexbarProviders.length > 0) {
    collectCostResults(fetchCostWithCommand(
      currentCliPath(),
      codexbarProviders.length === 1 ? codexbarProviders[0] : "all",
      "codexbar",
    ));
    for (const id of codexbarProviders) {
      seenProviders.add(id);
    }
  }

  const nativeProvidersForCost = uniqueProviderIds(configs.filter((config) =>
    NATIVE_COST_PROVIDERS.has(normalizeProviderId(config.provider))
  ));
  for (const providerId of nativeProvidersForCost) {
    collectCostResults(fetchCostWithCommand(nativeCliPath, providerId, "native"));
    seenProviders.add(providerId);
  }

  // When auto-discovery requested a bare "all" without an explicit list, still
  // try native OpenCode costs so local spend shows even if usage cookies fail.
  if (provider === "all" && configs.every((config) => !config._fromConfigs)) {
    for (const providerId of NATIVE_COST_PROVIDERS) {
      if (!seenProviders.has(providerId)) {
        collectCostResults(fetchCostWithCommand(nativeCliPath, providerId, "native"));
      }
    }
  }

  return { items, costError: costErrors.length > 0 ? costErrors.join(" | ") : null };
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

function fetchCostWithCommand(command, providerId, backend) {
  const commandArgs = [
    "cost",
    "--format",
    "json",
    "--json-only",
    "--provider",
    providerId,
  ];
  try {
    const payload = sharedFetch("cost", {
      command,
      commandArgs,
      provider: providerId,
      backend,
    }, () => {
      try {
        const extraEnv = backend === "codexbar" ? { SWIFT_TESTING: "1" } : {};
        return runJSON(command, commandArgs, providerId, "", "", extraEnv);
      } catch (error) {
        return [{
          provider: "cost",
          error: { message: shortError(error, command) },
        }];
      }
    });
    return asArray(payload);
  } catch (error) {
    return [{
      provider: "cost",
      error: { message: shortError(error, command) },
    }];
  }
}

function sharedFetch(namespace, identity, producer) {
  if (sharedCacheSeconds <= 0) {
    return producer();
  }

  const cacheDir = sharedProviderCacheDir();
  fs.mkdirSync(cacheDir, { recursive: true, mode: 0o700 });
  const cacheKey = crypto.createHash("sha256")
    .update(JSON.stringify(identity))
    .digest("hex");
  const cachePath = path.join(cacheDir, `${namespace}-${cacheKey}.json`);
  const lockPath = `${cachePath}.lock`;
  const waitDeadline = Date.now() + timeoutMs + 5000;

  while (true) {
    const cached = readSharedCache(cachePath, forceRefresh);
    if (cached.hit) {
      return cached.value;
    }

    let lockFd = null;
    try {
      lockFd = fs.openSync(lockPath, "wx", 0o600);
      fs.writeFileSync(lockFd, `${process.pid}\n${Date.now()}\n`);

      // Another helper may have populated the cache between our read and lock.
      const afterLock = readSharedCache(cachePath, forceRefresh);
      if (afterLock.hit) {
        return afterLock.value;
      }

      const value = producer();
      writeSharedCache(cachePath, value);
      return value;
    } catch (error) {
      if (error?.code !== "EEXIST") {
        throw error;
      }
      removeStaleLock(lockPath);
      if (Date.now() >= waitDeadline) {
        // A broken peer must not prevent this widget from refreshing forever.
        return producer();
      }
      sleepMilliseconds(50);
    } finally {
      if (lockFd !== null) {
        try { fs.closeSync(lockFd); } catch {}
        try { fs.unlinkSync(lockPath); } catch {}
      }
    }
  }
}

function sharedProviderCacheDir() {
  const cacheHome = clean(process.env.XDG_CACHE_HOME) || path.join(os.homedir(), ".cache");
  return path.join(cacheHome, "codexbar-plasmoid", "provider-cache");
}

function readSharedCache(cachePath, forced) {
  try {
    const stat = fs.statSync(cachePath);
    const freshEnough = forced
      ? stat.mtimeMs >= requestStartedAt
      : Date.now() - stat.mtimeMs <= sharedCacheSeconds * 1000;
    if (!freshEnough) {
      return { hit: false, value: null };
    }
    return { hit: true, value: JSON.parse(fs.readFileSync(cachePath, "utf8")) };
  } catch {
    return { hit: false, value: null };
  }
}

function writeSharedCache(cachePath, value) {
  const temporaryPath = `${cachePath}.${process.pid}.${Date.now()}.tmp`;
  try {
    fs.writeFileSync(temporaryPath, JSON.stringify(value), { mode: 0o600 });
    fs.renameSync(temporaryPath, cachePath);
  } finally {
    try { fs.unlinkSync(temporaryPath); } catch {}
  }
}

function removeStaleLock(lockPath) {
  try {
    const stat = fs.statSync(lockPath);
    if (Date.now() - stat.mtimeMs > timeoutMs + 10000) {
      fs.unlinkSync(lockPath);
    }
  } catch {
    // The lock disappeared between checks; the next loop will read the cache.
  }
}

function sleepMilliseconds(milliseconds) {
  const sleeper = new Int32Array(new SharedArrayBuffer(4));
  Atomics.wait(sleeper, 0, 0, milliseconds);
}

function hashSecret(value) {
  const secret = clean(value);
  return secret ? crypto.createHash("sha256").update(secret).digest("hex") : "";
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
      timeout: timeoutMs,
      windowsHide: true,
    });
  } catch (error) {
    const partial = clean(error?.stdout?.toString?.());
    if (partial) {
      return JSON.parse(partial);
    }
    throw error;
  }
  const trimmed = stdout.trim();
  if (!trimmed) {
    return [];
  }
  return JSON.parse(trimmed);
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
        filteredEntries[index] = normalizeProvider({ provider: providerId, source: "local" }, cost);
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
  const limitResetCredits = normalizeLimitResetCredits(usage);
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
    rows,
    creditsRemaining,
    limitResetCredits,
    codeReviewRemainingPercent: numberOrNull(dashboard.codeReviewRemainingPercent),
    tokenUsage,
    dailyUsage,
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
        currencyCode: cost.currencyCode || "USD",
        sessionLabel: cost.sessionLabel || "Today",
        last30DaysLabel: cost.last30DaysLabel || "30d",
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
    stage: typeof pace.stage === "string" ? pace.stage : null,
    willLastToReset,
    // Negative = budget in reserve versus the expected burn; positive = deficit.
    deltaPercent,
    expectedUsedPercent: numberOrNull(pace.expectedUsedPercent),
    etaSeconds: numberOrNull(pace.etaSeconds),
    summary: typeof pace.summary === "string" ? pace.summary : null,
  };
}

function usageRows(providerId, usage, source, pace = {}) {
  if (Array.isArray(usage.usageRows)) {
    return usage.usageRows.map((row) => ({
      id: String(row.id || row.title || "usage"),
      title: String(row.title || "Usage"),
      percentLeft: numberOrNull(row.percentLeft),
      resetsAt: row.resetsAt || null,
      windowMinutes: numberOrNull(row.windowMinutes),
      pace: normalizePace(row.pace),
    })).filter((row) => row.percentLeft !== null);
  }

  const labels = providerLabels(providerId);
  const windows = [
    ["primary", labels.session, usage.primary],
    ["secondary", labels.weekly, usage.secondary],
    ["tertiary", labels.tertiary, usage.tertiary],
  ];

  return windows.map(([id, title, window]) => {
    const usedPercent = numberOrNull(window?.usedPercent);
    const remainingPercent = numberOrNull(window?.remainingPercent);
    const percentLeft = remainingPercent !== null
      ? remainingPercent
      : usedPercent !== null
        ? Math.max(0, Math.min(100, 100 - usedPercent))
        : null;
    const resetsAt = window?.resetsAt || null;
    // For API providers, a window without resetsAt is just a balance placeholder,
    // not a real usage bar. Skip it so the balance summary renders instead.
    if (source === "api" && !resetsAt && percentLeft !== null) {
      return null;
    }
    return {
      id,
      title,
      percentLeft,
      resetsAt,
      windowMinutes: numberOrNull(window?.windowMinutes),
      pace: normalizePace(pace?.[id]),
    };
  }).filter((row) => row !== null && row.percentLeft !== null);
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
      });
    }
  } else if (cost && !cost.error && (Array.isArray(cost.daily) || cost.last30DaysCostUSD != null || cost.sessionCostUSD != null)) {
    hasSource = true;
    for (const day of asArray(cost.daily)) {
      mergeDailyPoint(byDay, day.date, {
        totalTokens: integerOrNull(day.totalTokens),
        costUSD: numberOrNull(day.totalCost),
        models: normalizeModelBreakdowns(day.modelBreakdowns || day.models),
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

function commandForConfig(config) {
  if (usesNativeCli(config.provider, config.source)) {
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
  return Number.isInteger(number) ? number : Number.isFinite(number) ? Math.round(number) : null;
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
