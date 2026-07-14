#!/usr/bin/env node
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const args = parseArgs(process.argv.slice(2));
const action = String(args.action || "read");

try {
  if (action === "read") {
    printJson(readSharedProviders());
  } else if (action === "write") {
    const providers = normalizeProviders(args.providers);
    writeSharedProviders(providers);
    printJson({ ok: true, exists: true, providers });
  } else {
    throw new Error(`Unknown provider sync action: ${action}`);
  }
} catch (error) {
  printJson({ ok: false, exists: false, providers: [], error: error.message || String(error) });
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

function sharedProvidersPath() {
  const configHome = clean(process.env.XDG_CONFIG_HOME) || path.join(os.homedir(), ".config");
  return path.join(configHome, "codexbar-plasmoid", "shared-providers.json");
}

function readSharedProviders() {
  const filePath = sharedProvidersPath();
  if (!fs.existsSync(filePath)) {
    return { ok: true, exists: false, providers: [] };
  }
  const providers = normalizeProviders(fs.readFileSync(filePath, "utf8"));
  return { ok: true, exists: true, providers };
}

function writeSharedProviders(providers) {
  const filePath = sharedProvidersPath();
  const directory = path.dirname(filePath);
  const temporaryPath = `${filePath}.${process.pid}.${Date.now()}.tmp`;
  fs.mkdirSync(directory, { recursive: true, mode: 0o700 });
  try {
    fs.writeFileSync(temporaryPath, `${JSON.stringify(providers, null, 2)}\n`, { mode: 0o600 });
    fs.renameSync(temporaryPath, filePath);
  } finally {
    try { fs.unlinkSync(temporaryPath); } catch {}
  }
}

function normalizeProviders(raw) {
  const parsed = typeof raw === "string" ? JSON.parse(raw || "[]") : raw;
  if (!Array.isArray(parsed)) {
    throw new Error("Shared providers must be a JSON array");
  }
  return parsed.map((item) => ({
    provider: clean(item?.provider) || "codex",
    source: clean(item?.source) || "auto",
    enabled: item?.enabled !== false,
    account: clean(item?.account),
    accountIndex: Math.max(0, Number(item?.accountIndex || 0)),
    allAccounts: item?.allAccounts === true,
    apiKey: clean(item?.apiKey),
  }));
}

function clean(value) {
  return typeof value === "string" ? value.trim() : "";
}

function printJson(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}
