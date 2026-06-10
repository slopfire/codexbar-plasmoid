#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const args = parseArgs(process.argv.slice(2));
const timeoutMs = Math.max(5, Number(args.timeout || 45)) * 1000;
const cliPath = args.cli || process.env.CODEXBAR_CLI || "codexbar";
const nativeCliPath = args.nativeCli || process.env.CODEXBAR_NATIVE_CLI || resolveNativeCliPath();
const provider = clean(args.provider) || "all";
const source = clean(args.source) || "auto";
const providerConfigs = parseProviderConfigs(args.providers);
const includeCost = args.cost !== "false";
const includeStatus = args.status !== "false";
const showCredits = args.credits !== "false";
const anonymizeEmails = args.anonymizeEmails !== "false" && args["anonymize-emails"] !== "false";

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

try {
  const usage = runUsage();
  const cost = includeCost ? runCost() : [];
  const snapshot = normalizeSnapshot(usage, cost);
  process.stdout.write(`${JSON.stringify(snapshot)}\n`);
} catch (error) {
  process.stdout.write(`${JSON.stringify(errorSnapshot(error))}\n`);
  process.exitCode = 0;
}

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

  if (!anonymizeEmails) {
    commandArgs.push("--anonymize-emails", "false");
  }

  const command = commandForConfig(config);
  try {
    return runJSON(command, commandArgs);
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
      results.push(...asArray(runJSON(command, commandArgs)));
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

function runJSON(command, commandArgs) {
  const invocation = resolveCommandInvocation(command);
  let stdout = "";
  try {
    stdout = execFileSync(invocation.command, [...invocation.prefix, ...commandArgs], {
      encoding: "utf8",
      env: process.env,
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

  const entries = asArray(usagePayload).map((item) => {
    const cost = costByProvider.get(item.provider);
    return normalizeProvider(item, cost);
  });

  if (entries.length === 0 && costByProvider.size > 0) {
    for (const [providerId, cost] of costByProvider) {
      if (providerId !== "cost") {
        entries.push(normalizeProvider({ provider: providerId, source: "local" }, cost));
      }
    }
  }

  return {
    ok: true,
    generatedAt: new Date().toISOString(),
    requestedProvider: provider,
    entries,
    costError: costByProvider.get("cost")?.error?.message || null,
  };
}

function normalizeProvider(item, cost) {
  const providerId = item.provider || "unknown";
  const usage = item.usage || {};
  const dashboard = item.openaiDashboard || {};
  const identity = usage.identity || {};
  const rows = usageRows(providerId, usage);
  const dailyUsage = dailyUsagePoints(dashboard, cost);
  const rawAccount = item.account || usage.accountEmail || identity.accountEmail || null;
  const account = anonymizeEmails ? anonymizeEmailAddress(rawAccount) : rawAccount;

  return {
    provider: providerId,
    account,
    organization: usage.accountOrganization || identity.accountOrganization || null,
    plan: usage.loginMethod || identity.loginMethod || null,
    source: item.source || "unknown",
    version: item.version || null,
    updatedAt: usage.updatedAt || item.credits?.updatedAt || cost?.updatedAt || new Date().toISOString(),
    status: item.status || null,
    error: item.error || null,
    rows,
    creditsRemaining: numberOrNull(item.credits?.remaining),
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

function usageRows(providerId, usage) {
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
    return { id, title, percentLeft, resetsAt: window?.resetsAt || null };
  }).filter((row) => row.percentLeft !== null);
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
  return cliPath;
}

function resolveCommandInvocation(command) {
  return { command, prefix: [] };
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

function shortError(error, command = cliPath) {
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
