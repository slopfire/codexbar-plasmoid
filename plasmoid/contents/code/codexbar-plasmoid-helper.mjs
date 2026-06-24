#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const args = parseArgs(process.argv.slice(2));
// KDE's plasmoid process inherits the user's session env from systemd --user
// (which sources $XDG_CONFIG_HOME/environment.d/*.conf), but env vars the
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
const providerConfigs = parseProviderConfigs(args.providers);
const includeCost = args.cost !== "false";
const includeStatus = args.status !== "false";
const showCredits = args.credits !== "false";
const anonymizeEmails = args.anonymizeEmails !== "false" && args["anonymize-emails"] !== "false";
const kdeProviderConfig = loadKdeProviderConfig();

const nativeProviders = new Set(["antigravity", "cursor", "opencode", "opencodego"]);

const linuxAutoFallbacks = {
  codex: "cli",
  claude: "cli",
  cursor: "native",
  opencode: "native",
  opencodego: "native",
  antigravity: "native",
  augment: "cli",
  factory: "cli",
  grok: "cli",
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
    const cost = includeCost ? runCost() : [];
    const snapshot = normalizeSnapshot(usage, cost);
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
  if (configs.length > 0) {
    return configs.flatMap((config) => asArray(runUsageForConfig(config)));
  }
  return runUsageForConfig({
    provider,
    source,
    account: clean(args.account),
    accountIndex: Number(args.accountIndex || 0),
    allAccounts: args.allAccounts === "true",
  });
}

function runUsageForConfig(config) {
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
  if (config.allAccounts === true) {
    commandArgs.push("--all-accounts");
  } else if (clean(config.account)) {
    commandArgs.push("--account", clean(config.account));
  } else if (Number(config.accountIndex || 0) > 0) {
    commandArgs.push("--account-index", String(Number(config.accountIndex)));
  }


  const command = commandForConfig(config);
  try {
    return runJSON(command, commandArgs, config.provider, config.apiKey);
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

function runCost() {
  const configs = effectiveProviderConfigs().length > 0
    ? effectiveProviderConfigs()
    : [{ provider }];
  const results = [];
  for (const config of configs) {
    const commandArgs = [
      "cost",
      "--format",
      "json",
      "--json-only",
      "--provider",
      config.provider,
    ];
    const command = commandForConfig(config);
    try {
      results.push(...asArray(runJSON(command, commandArgs, config.provider, config.apiKey || "")));
    } catch (error) {
      results.push({
        provider: "cost",
        error: {
          message: shortError(error, command),
        },
      });
    }
  }
  return results;
}

function effectiveProviderConfigs() {
  if (providerConfigs.length > 0) {
    return providerConfigs;
  }
  if (process.platform !== "darwin" && provider === "all" && source === "auto") {
    return [
      { provider: "codex", source: "cli", account: "", accountIndex: 0, allAccounts: false },
      { provider: "gemini", source: "api", account: "", accountIndex: 0, allAccounts: false },
    ];
  }
  return [];
}

function runJSON(command, commandArgs, providerId = "", apiKey = "") {
  const invocation = resolveCommandInvocation(command);
  let stdout = "";
  try {
    stdout = execFileSync(invocation.command, [...invocation.prefix, ...commandArgs], {
      encoding: "utf8",
      env: cliEnvForProvider(providerId, apiKey),
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

function normalizeSnapshot(usagePayload, costPayload) {
  const costByProvider = new Map();
  for (const item of asArray(costPayload)) {
    if (item && typeof item.provider === "string") {
      costByProvider.set(item.provider, item);
    }
  }

  const entries = asArray(usagePayload).map((item, index) => {
    const cost = costByProvider.get(item.provider);
    const normalized = normalizeProvider(item, cost);
    // Stable id so the switcher can select between multiple accounts of the
    // same provider (e.g. several OpenCode Go accounts). Prefer the account
    // email; fall back to the entry index to guarantee uniqueness.
    normalized.id = `${normalized.provider}:${normalized.account || String(index)}`;
    return normalized;
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

  if (filteredEntries.length === 0 && costByProvider.size > 0) {
    let fallbackIndex = 0;
    for (const [providerId, cost] of costByProvider) {
      if (providerId !== "cost") {
        const fallback = normalizeProvider({ provider: providerId, source: "local" }, cost);
        fallback.id = `${fallback.provider}:${fallback.account || String(fallbackIndex)}`;
        filteredEntries.push(fallback);
        fallbackIndex += 1;
      }
    }
  }

  return {
    ok: true,
    generatedAt: new Date().toISOString(),
    requestedProvider: provider,
    entries: filteredEntries,
    costError: costByProvider.get("cost")?.error?.message || null,
  };
}


function normalizeProvider(item, cost) {
  const providerId = item.provider || "unknown";
  const usage = item.usage || {};
  const dashboard = item.openaiDashboard || {};
  const identity = usage.identity || {};
  const source = item.source || "unknown";
  const rows = usageRows(providerId, usage, source);
  const dailyUsage = dailyUsagePoints(dashboard, cost);
  const rawAccount = item.account || usage.accountEmail || identity.accountEmail || null;
  const account = anonymizeEmails ? anonymizeEmailAddress(rawAccount) : rawAccount;

  let creditsRemaining = numberOrNull(item.credits?.remaining ?? usage.openRouterUsage?.balance);
  if (creditsRemaining === null && source === "api" && usage.primary?.resetDescription) {
    creditsRemaining = parseBalanceFromDescription(usage.primary.resetDescription);
  }

  return {
    provider: providerId,
    account,
    organization: usage.accountOrganization || identity.accountOrganization || null,
    plan: usage.loginMethod || identity.loginMethod || null,
    source,
    version: item.version || null,
    updatedAt: usage.updatedAt || item.credits?.updatedAt || cost?.updatedAt || new Date().toISOString(),
    status: item.status || null,
    error: item.error || null,
    rows,
    creditsRemaining,
    codeReviewRemainingPercent: numberOrNull(dashboard.codeReviewRemainingPercent),
    tokenUsage: cost ? {
      sessionCostUSD: numberOrNull(cost.sessionCostUSD),
      sessionTokens: integerOrNull(cost.sessionTokens),
      last30DaysCostUSD: numberOrNull(cost.last30DaysCostUSD),
      last30DaysTokens: integerOrNull(cost.last30DaysTokens),
      currencyCode: cost.currencyCode || "USD",
      sessionLabel: cost.sessionLabel || "Today",
      last30DaysLabel: cost.last30DaysLabel || "30d",
    } : null,
    dailyUsage,
  };
}

function usageRows(providerId, usage, source) {
  if (Array.isArray(usage.usageRows)) {
    return usage.usageRows.map((row) => ({
      id: String(row.id || row.title || "usage"),
      title: String(row.title || "Usage"),
      percentLeft: numberOrNull(row.percentLeft),
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
    return { id, title, percentLeft, resetsAt };
  }).filter((row) => row !== null && row.percentLeft !== null);
}

function parseBalanceFromDescription(description) {
  if (typeof description !== "string") {
    return null;
  }
  const match = description.match(/^\$([\d,.]+)/);
  if (!match) {
    return null;
  }
  return numberOrNull(match[1].replace(/,/g, ""));
}

function dailyUsagePoints(dashboard, cost) {
  const dashboardDays = Array.isArray(dashboard.dailyBreakdown)
    ? dashboard.dailyBreakdown.map((day) => ({
      dayKey: day.day,
      totalTokens: null,
      costUSD: numberOrNull(day.totalCreditsUsed),
    }))
    : [];
  if (dashboardDays.length > 0) {
    return dashboardDays.slice(-30);
  }

  const costDays = Array.isArray(cost?.daily)
    ? cost.daily.map((day) => ({
      dayKey: day.date,
      totalTokens: integerOrNull(day.totalTokens),
      costUSD: numberOrNull(day.totalCost),
    }))
    : [];
  return costDays.slice(-30);
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
      return { session: "Claude", weekly: "Gemini Pro", tertiary: "Gemini Flash" };
    case "opencode":
      return { session: "Rolling Usage", weekly: "Weekly Usage", tertiary: "Extra" };
    case "opencodego":
      return { session: "Rolling Usage", weekly: "Weekly Usage", tertiary: "Monthly Usage" };
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
    }));
}

function resolveSource(providerId, requestedSource) {
  const sourceMode = clean(requestedSource) || "auto";
  if (process.platform === "darwin" || sourceMode !== "auto") {
    return sourceMode;
  }
  const fallback = linuxAutoFallbacks[normalizeProviderId(providerId)];
  return fallback || sourceMode;
}

function usesNativeCli(providerId, requestedSource) {
  const resolved = resolveSource(providerId, requestedSource);
  return resolved === "native" && nativeProviders.has(normalizeProviderId(providerId));
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

function cliEnvForProvider(providerId, apiKey) {
  const env = { ...process.env };
  const envName = providerApiKeyEnvName(providerId);
  const resolvedApiKey = clean(apiKey) || providerApiKey(providerId);
  if (envName && resolvedApiKey && !clean(env[envName])) {
    env[envName] = resolvedApiKey;
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
