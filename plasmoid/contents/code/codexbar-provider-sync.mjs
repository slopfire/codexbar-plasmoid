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
  } else if (action === "read-selection") {
    printJson(readSelection(args.instance));
  } else if (action === "write-selection") {
    const selection = normalizeSelection(args.selection);
    writeSelection(args.instance, selection);
    printJson({
      ok: true,
      exists: true,
      action: "write-selection",
      instance: selectionInstanceKey(args.instance),
      entryIds: selection.entryIds,
      providers: selection.providers,
    });
  } else {
    throw new Error(`Unknown provider sync action: ${action}`);
  }
} catch (error) {
  printJson({
    ok: false,
    exists: false,
    action,
    providers: [],
    entryIds: [],
    error: error.message || String(error),
  });
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

function configDir() {
  const configHome = clean(process.env.XDG_CONFIG_HOME) || path.join(os.homedir(), ".config");
  return path.join(configHome, "codexbar-plasmoid");
}

function sharedProvidersPath() {
  return path.join(configDir(), "shared-providers.json");
}

function selectionStorePath(instance) {
  const key = selectionInstanceKey(instance);
  return path.join(configDir(), "selection", `${key}.json`);
}

function selectionInstanceKey(instance) {
  const raw = clean(instance) || "default";
  // Keep filenames boring and portable.
  return raw.replace(/[^A-Za-z0-9._-]+/g, "_").slice(0, 80) || "default";
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
  atomicWriteJson(sharedProvidersPath(), providers);
}

function readSelection(instance) {
  const filePath = selectionStorePath(instance);
  if (!fs.existsSync(filePath)) {
    return {
      ok: true,
      exists: false,
      action: "read-selection",
      instance: selectionInstanceKey(instance),
      entryIds: [],
      providers: [],
    };
  }
  const selection = normalizeSelection(fs.readFileSync(filePath, "utf8"));
  return {
    ok: true,
    exists: true,
    action: "read-selection",
    instance: selectionInstanceKey(instance),
    entryIds: selection.entryIds,
    providers: selection.providers,
  };
}

function writeSelection(instance, selection) {
  atomicWriteJson(selectionStorePath(instance), {
    entryIds: selection.entryIds,
    providers: selection.providers,
    updatedAt: new Date().toISOString(),
  });
}

function atomicWriteJson(filePath, value) {
  const directory = path.dirname(filePath);
  const temporaryPath = `${filePath}.${process.pid}.${Date.now()}.tmp`;
  fs.mkdirSync(directory, { recursive: true, mode: 0o700 });
  try {
    fs.writeFileSync(temporaryPath, `${JSON.stringify(value, null, 2)}\n`, { mode: 0o600 });
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
    includeCost: item?.includeCost !== false,
  }));
}

function normalizeSelection(raw) {
  const parsed = typeof raw === "string" ? JSON.parse(raw || "{}") : (raw || {});
  if (Array.isArray(parsed)) {
    // Older/simple payloads may be a bare entry-id list.
    return {
      entryIds: normalizeStringList(parsed),
      providers: [],
    };
  }
  if (!parsed || typeof parsed !== "object") {
    throw new Error("Selection payload must be a JSON object or array");
  }
  return {
    entryIds: normalizeStringList(parsed.entryIds),
    providers: normalizeStringList(parsed.providers),
  };
}

function normalizeStringList(value) {
  if (!Array.isArray(value)) {
    return [];
  }
  return value.map((item) => String(item || "").trim()).filter((item) => item.length > 0);
}

function clean(value) {
  return typeof value === "string" ? value.trim() : "";
}

function printJson(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}
