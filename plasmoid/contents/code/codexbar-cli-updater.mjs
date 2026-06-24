#!/usr/bin/env node
// Downloads and updates the CodexBar CLI from GitHub Releases.
// Can be used standalone or imported by codexbar-plasmoid-helper.mjs.

import { createHash } from "node:crypto";
import { execFileSync, spawn } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import https from "node:https";
import os from "node:os";
import path from "node:path";
import { pipeline } from "node:stream/promises";
import { fileURLToPath, pathToFileURL } from "node:url";

function injectSqliteEnv(targetDir) {
  for (const envVar of ["NIX_LD_LIBRARY_PATH", "LD_LIBRARY_PATH"]) {
    const existing = process.env[envVar] || "";
    const paths = existing.split(":").filter(Boolean);
    if (!paths.includes(targetDir)) {
      process.env[envVar] = [targetDir, ...paths].join(":");
    }
  }
}

function extractDebDataTar(debBuffer) {
  if (debBuffer.toString("utf8", 0, 8) !== "!<arch>\n") {
    throw new Error("Invalid deb format");
  }
  let offset = 8;
  while (offset < debBuffer.length) {
    const name = debBuffer.toString("utf8", offset, offset + 16).trim();
    const sizeStr = debBuffer.toString("utf8", offset + 48, offset + 58).trim();
    const size = parseInt(sizeStr, 10);
    offset += 60; // skip header
    if (name.startsWith("data.tar")) {
      return {
        name,
        buffer: debBuffer.subarray(offset, offset + size)
      };
    }
    offset += size;
    if (size % 2 !== 0) {
      offset += 1; // skip padding byte
    }
  }
  throw new Error("data.tar not found in deb");
}

function extractTarball(archivePath, destDir) {
  fs.mkdirSync(destDir, { recursive: true, mode: 0o755 });
  const result = spawn("tar", ["-xf", archivePath, "-C", destDir], {
    stdio: ["ignore", "pipe", "pipe"],
    timeout: 60000,
  });
  return new Promise((resolve, reject) => {
    let stdout = "";
    let stderr = "";
    result.stdout.on("data", (data) => { stdout += data; });
    result.stderr.on("data", (data) => { stderr += data; });
    result.on("error", reject);
    result.on("close", (code) => {
      if (code !== 0) {
        reject(new Error(`tar extraction failed (${code}): ${stderr || stdout}`));
      } else {
        resolve();
      }
    });
  });
}

async function downloadSqlite(targetDir) {
  const arch = process.arch;
  let debArch = "amd64";
  if (arch === "arm64") {
    debArch = "arm64";
  } else if (arch === "ia32") {
    debArch = "i386";
  } else if (arch === "x64") {
    debArch = "amd64";
  }
  const url = `https://snapshot.debian.org/archive/debian/20240301T030000Z/pool/main/s/sqlite3/libsqlite3-0_3.45.1-1_${debArch}.deb`;
  
  const tempDir = path.join(targetDir, `.sqlite-download-${process.pid}-${Date.now()}`);
  fs.mkdirSync(tempDir, { recursive: true });
  
  try {
    const debBuffer = await downloadToBuffer(url);
    const dataTar = extractDebDataTar(debBuffer);
    const tarPath = path.join(tempDir, dataTar.name);
    fs.writeFileSync(tarPath, dataTar.buffer);
    
    const extractDir = path.join(tempDir, "extracted");
    fs.mkdirSync(extractDir, { recursive: true });
    
    await extractTarball(tarPath, extractDir);
    
    const findSqlite = (dir) => {
      const files = fs.readdirSync(dir);
      for (const file of files) {
        const fullPath = path.join(dir, file);
        if (fs.statSync(fullPath).isDirectory()) {
          const found = findSqlite(fullPath);
          if (found) return found;
        } else if (file === "libsqlite3.so.0" || file.startsWith("libsqlite3.so.0.")) {
          return fs.realpathSync(fullPath);
        }
      }
      return null;
    };
    
    const realSqlitePath = findSqlite(extractDir);
    if (!realSqlitePath) {
      throw new Error("libsqlite3.so.0 not found in extracted deb");
    }
    
    const destPath = path.join(targetDir, "libsqlite3.so.0");
    fs.copyFileSync(realSqlitePath, destPath);
    const realFileName = path.basename(realSqlitePath);
    if (realFileName !== "libsqlite3.so.0") {
      fs.copyFileSync(realSqlitePath, path.join(targetDir, realFileName));
    }
  } finally {
    try {
      fs.rmSync(tempDir, { recursive: true, force: true });
    } catch {}
  }
}

async function ensureSqlite(targetDir, testBinaryPath) {
  if (process.platform !== "linux") {
    return;
  }
  const sqliteFile = path.join(targetDir, "libsqlite3.so.0");
  if (fs.existsSync(sqliteFile)) {
    injectSqliteEnv(targetDir);
    return;
  }
  if (!testBinaryPath || !fs.existsSync(testBinaryPath)) {
    return;
  }
  try {
    execFileSync(testBinaryPath, ["--version"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      timeout: 5000,
    });
  } catch (error) {
    const errMsg = error.message || String(error);
    if (errMsg.includes("libsqlite3.so.0")) {
      await downloadSqlite(targetDir);
      injectSqliteEnv(targetDir);
    }
  }
}

const sqliteFile = path.join(managedDir(), "libsqlite3.so.0");
if (fs.existsSync(sqliteFile)) {
  injectSqliteEnv(managedDir());
}

const DEFAULT_REPO = "steipete/CodexBar";
const RELEASE_API_LATEST = (repo) => `https://api.github.com/repos/${repo}/releases/latest`;
const RELEASE_API_TAG = (repo, tag) => `https://api.github.com/repos/${repo}/releases/tags/${tag}`;
const CACHE_TTL_MS = 60 * 60 * 1000; // 1 hour
const HTTP_TIMEOUT_MS = 60 * 1000;
const MAX_REDIRECTS = 5;

function managedDir() {
  return path.join(os.homedir(), ".local", "share", "codexbar-plasmoid", "bin");
}

function managedBinary() {
  return path.join(managedDir(), "codexbar");
}

function cachePath(tag = "latest") {
  const safeTag = String(tag || "latest").replace(/[^a-zA-Z0-9._-]/g, "_");
  const name = safeTag === "latest" ? "update-cache.json" : `update-cache-${safeTag}.json`;
  return path.join(managedDir(), "..", name);
}

function ensureManagedDir() {
  const dir = managedDir();
  fs.mkdirSync(dir, { recursive: true, mode: 0o755 });
  return dir;
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

function clean(value) {
  return typeof value === "string" ? value.trim() : "";
}

function platformId() {
  switch (process.platform) {
    case "darwin":
      return "macos";
    case "linux":
      return "linux";
    case "win32":
      return "windows";
    default:
      return process.platform;
  }
}

function archId() {
  switch (process.arch) {
    case "x64":
      return "x86_64";
    case "arm64":
      return "aarch64";
    default:
      return process.arch;
  }
}

function candidateAssetNames(version) {
  const plat = platformId();
  const arch = archId();
  if (plat === "linux") {
    // Prefer the statically-linked musl build: it runs on non-FHS systems
    // (NixOS, Alpine, etc.) without libcurl/libstdc++/libsqlite3 at runtime.
    // Fall back to the glibc build for releases that don't ship a musl asset.
    return [
      `CodexBarCLI-v${version}-linux-musl-${arch}.tar.gz`,
      `CodexBarCLI-v${version}-linux-${arch}.tar.gz`,
    ];
  }
  return [`CodexBarCLI-v${version}-${plat}-${arch}.tar.gz`];
}

function userAgent() {
  return "codexbar-plasmoid";
}

async function httpsRequest(url, options = {}) {
  return new Promise((resolve, reject) => {
    const client = url.startsWith("https:") ? https : http;
    const req = client.get(
      url,
      {
        timeout: HTTP_TIMEOUT_MS,
        headers: {
          "User-Agent": userAgent(),
          Accept: options.accept || "application/octet-stream",
          ...(options.headers || {}),
        },
      },
      (response) => resolve(response)
    );
    req.on("error", reject);
    req.on("timeout", () => {
      req.destroy();
      reject(new Error(`Request timed out: ${url}`));
    });
  });
}

async function fetchWithRedirects(url, options = {}, redirectCount = 0) {
  if (redirectCount > MAX_REDIRECTS) {
    throw new Error(`Too many redirects for ${url}`);
  }
  const response = await httpsRequest(url, options);
  if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
    const location = new URL(response.headers.location, url).toString();
    return fetchWithRedirects(location, options, redirectCount + 1);
  }
  if (response.statusCode < 200 || response.statusCode >= 300) {
    const body = await readStreamText(response);
    throw new Error(`HTTP ${response.statusCode} for ${url}: ${body.slice(0, 200)}`);
  }
  return response;
}

async function readStreamText(stream) {
  const chunks = [];
  for await (const chunk of stream) {
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString("utf8");
}

async function fetchJson(url) {
  const response = await fetchWithRedirects(url, { accept: "application/vnd.github+json" });
  const text = await readStreamText(response);
  return JSON.parse(text);
}

async function downloadToFile(url, destPath) {
  const response = await fetchWithRedirects(url);
  const tmpPath = `${destPath}.tmp.${process.pid}`;
  try {
    const file = fs.createWriteStream(tmpPath);
    await pipeline(response, file);
    fs.renameSync(tmpPath, destPath);
  } catch (error) {
    try { fs.unlinkSync(tmpPath); } catch {}
    throw error;
  }
}

async function downloadToBuffer(url) {
  const response = await fetchWithRedirects(url);
  const chunks = [];
  for await (const chunk of response) {
    chunks.push(chunk);
  }
  return Buffer.concat(chunks);
}

function sha256File(filePath) {
  const hash = createHash("sha256");
  hash.update(fs.readFileSync(filePath));
  return hash.digest("hex");
}

function readUpdateCache(tag = "latest") {
  try {
    const text = fs.readFileSync(cachePath(tag), "utf8");
    const parsed = JSON.parse(text);
    if (parsed && typeof parsed.version === "string" && typeof parsed.assetUrl === "string") {
      return parsed;
    }
  } catch {
    // Malformed or missing cache is fine; fetch fresh metadata.
  }
  return null;
}

function writeUpdateCache(cache, tag = "latest") {
  ensureManagedDir();
  fs.writeFileSync(cachePath(tag), JSON.stringify(cache, null, 2), { mode: 0o644 });
}

function isCacheFresh(cache) {
  if (!cache || !cache.checkedAt) {
    return false;
  }
  const checked = new Date(cache.checkedAt).getTime();
  return Number.isFinite(checked) && Date.now() - checked < CACHE_TTL_MS;
}

async function fetchReleaseMetadata(repo, tag, force) {
  const cached = readUpdateCache(tag);
  if (!force && isCacheFresh(cached)) {
    return cached;
  }
  const url = tag === "latest" ? RELEASE_API_LATEST(repo) : RELEASE_API_TAG(repo, tag);
  const release = await fetchJson(url);
  const version = String(release.tag_name || "").replace(/^v/, "");
  if (!version) {
    throw new Error("GitHub release has no version tag");
  }
  const assets = release.assets || [];
  const name = candidateAssetNames(version).find((candidate) =>
    assets.some((a) => a.name === candidate)
  );
  if (!name) {
    throw new Error(`No release asset found for ${candidateAssetNames(version).join(" or ")}`);
  }
  const checksumName = `${name}.sha256`;
  const asset = assets.find((a) => a.name === name);
  const checksumAsset = assets.find((a) => a.name === checksumName);
  const metadata = {
    version,
    tag: release.tag_name,
    assetName: name,
    checksumName,
    assetUrl: asset.browser_download_url,
    checksumUrl: checksumAsset?.browser_download_url || null,
    checkedAt: new Date().toISOString(),
  };
  writeUpdateCache(metadata, tag);
  return metadata;
}

function getInstalledVersion(binaryPath) {
  if (!fs.existsSync(binaryPath)) {
    return null;
  }
  try {
    fs.accessSync(binaryPath, fs.constants.X_OK);
  } catch {
    fs.chmodSync(binaryPath, 0o755);
  }
  try {
    const stdout = execFileSync(binaryPath, ["--version"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      timeout: 10000,
    });
    const match = stdout.match(/(\d+\.\d+\.\d+[^\s]*)/);
    if (match) {
      return match[1];
    }
  } catch {
    // Binary could not be probed; fall through to the VERSION file.
  }
  // The CLI resolves its version from a sibling `VERSION` file. When the
  // binary prints no version (file missing at its argv[0] dir) or cannot run,
  // read the file next to the managed binary directly.
  try {
    const raw = clean(fs.readFileSync(path.join(path.dirname(binaryPath), "VERSION"), "utf8"));
    const match = raw.match(/(\d+\.\d+\.\d+[^\s]*)/);
    return match ? match[1] : null;
  } catch {
    return null;
  }
}

function extractTarGz(archivePath, destDir) {
  fs.mkdirSync(destDir, { recursive: true, mode: 0o755 });
  const result = spawn("tar", ["-xzf", archivePath, "-C", destDir], {
    stdio: ["ignore", "pipe", "pipe"],
    timeout: 60000,
  });
  return new Promise((resolve, reject) => {
    let stdout = "";
    let stderr = "";
    result.stdout.on("data", (data) => { stdout += data; });
    result.stderr.on("data", (data) => { stderr += data; });
    result.on("error", reject);
    result.on("close", (code) => {
      if (code !== 0) {
        reject(new Error(`tar extraction failed (${code}): ${stderr || stdout}`));
      } else {
        resolve();
      }
    });
  });
}

function cleanupStaleUpdateDirs(targetDir) {
  // Remove leftover `.update-*` temp directories from killed or crashed
  // update attempts. Each holds the downloaded tarball + extracted binary,
  // so accumulating them wastes real disk space.
  const cutoff = Date.now() - 10 * 60 * 1000;
  let entries = [];
  try {
    entries = fs.readdirSync(targetDir);
  } catch {
    return;
  }
  for (const entry of entries) {
    if (!entry.startsWith(".update-")) {
      continue;
    }
    const full = path.join(targetDir, entry);
    try {
      const st = fs.statSync(full);
      if (st.mtimeMs < cutoff) {
        fs.rmSync(full, { recursive: true, force: true });
      }
    } catch {}
  }
}

async function performUpdate(metadata, options = {}) {
  const targetDir = options.targetDir || managedDir();
  const targetBinary = path.join(targetDir, "codexbar");
  ensureManagedDir();
  cleanupStaleUpdateDirs(targetDir);

  const tempRoot = path.join(targetDir, `.update-${process.pid}-${Date.now()}`);
  fs.mkdirSync(tempRoot, { recursive: true, mode: 0o700 });
  let extractedBinary = null;

  try {
    const archivePath = path.join(tempRoot, metadata.assetName);
    await downloadToFile(metadata.assetUrl, archivePath);

    if (metadata.checksumUrl) {
      const checksumBuffer = await downloadToBuffer(metadata.checksumUrl);
      const expected = clean(checksumBuffer.toString("utf8")).split(/\s+/)[0].toLowerCase();
      const actual = sha256File(archivePath);
      if (expected && actual !== expected) {
        throw new Error(`Checksum mismatch for ${metadata.assetName}`);
      }
    }

    const extractDir = path.join(tempRoot, "extracted");
    await extractTarGz(archivePath, extractDir);

    const candidates = ["CodexBarCLI", "codexbar"];
    for (const name of candidates) {
      const candidate = path.join(extractDir, name);
      if (fs.existsSync(candidate)) {
        // The upstream tarball ships `codexbar` as a symlink to `CodexBarCLI`.
        // If we moved the symlink, it would break once the temp directory is
        // removed, so always install the real executable.
        extractedBinary = fs.realpathSync(candidate);
        break;
      }
    }
    if (!extractedBinary) {
      throw new Error("Extracted archive does not contain codexbar or CodexBarCLI");
    }

    fs.chmodSync(extractedBinary, 0o755);

    // Test the binary before replacing the managed copy.
    await ensureSqlite(targetDir, extractedBinary);
    const testStdout = execFileSync(extractedBinary, ["--version"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      timeout: 10000,
    });
    const versionMatch = testStdout.match(/(\d+\.\d+\.\d+[^\s]*)/);
    if (!versionMatch) {
      throw new Error("Downloaded binary does not report a version");
    }

    // Atomic replacement via rename.
    fs.renameSync(extractedBinary, targetBinary);
    // The CLI reads its version from a `VERSION` file next to the executable,
    // so install it alongside the managed binary (the tarball ships one).
    const versionFile = path.join(extractDir, "VERSION");
    if (fs.existsSync(versionFile)) {
      const tmpVersion = `${path.join(targetDir, "VERSION")}.tmp.${process.pid}`;
      fs.copyFileSync(versionFile, tmpVersion);
      fs.renameSync(tmpVersion, path.join(targetDir, "VERSION"));
    }
    return { version: versionMatch[1], targetBinary };
  } finally {
    try {
      fs.rmSync(tempRoot, { recursive: true, force: true });
    } catch {}
  }
}

function compareVersions(a, b) {
  const parse = (v) => String(v || "0").replace(/^v/, "").split(".").map((n) => parseInt(n, 10) || 0);
  const aa = parse(a);
  const bb = parse(b);
  for (let i = 0; i < Math.max(aa.length, bb.length); i += 1) {
    const diff = (aa[i] || 0) - (bb[i] || 0);
    if (diff !== 0) {
      return diff;
    }
  }
  return 0;
}

function needsUpdate(installed, latest) {
  if (!installed) {
    return true;
  }
  return compareVersions(latest, installed) > 0;
}

function buildStatus(metadata, installedVersion, targetBinary) {
  return {
    installedVersion,
    latestVersion: metadata.version,
    targetBinary,
    needsUpdate: needsUpdate(installedVersion, metadata.version),
  };
}

export async function checkUpdate(options = {}) {
  const repo = options.repo || DEFAULT_REPO;
  const tag = options.tag || "latest";
  const targetDir = options.targetDir || managedDir();
  const targetBinary = path.join(targetDir, "codexbar");
  const metadata = await fetchReleaseMetadata(repo, tag, options.force === true);
  await ensureSqlite(targetDir, targetBinary);
  const installedVersion = getInstalledVersion(targetBinary);
  return {
    ok: true,
    ...buildStatus(metadata, installedVersion, targetBinary),
    updated: false,
    error: null,
  };
}

export async function updateIfNeeded(options = {}) {
  const repo = options.repo || DEFAULT_REPO;
  const tag = options.tag || "latest";
  const targetDir = options.targetDir || managedDir();
  const targetBinary = path.join(targetDir, "codexbar");
  const force = options.force === true;

  const metadata = await fetchReleaseMetadata(repo, tag, force);
  await ensureSqlite(targetDir, targetBinary);
  const installedVersion = getInstalledVersion(targetBinary);
  const status = buildStatus(metadata, installedVersion, targetBinary);

  if (!status.needsUpdate && !force) {
    return {
      ok: true,
      ...status,
      updated: false,
      error: null,
    };
  }

  const result = await performUpdate(metadata, { targetDir });
  return {
    ok: true,
    previousVersion: installedVersion || null,
    installedVersion: result.version,
    latestVersion: metadata.version,
    targetBinary: result.targetBinary,
    needsUpdate: false,
    updated: true,
    error: null,
  };
}

export async function forceUpdate(options = {}) {
  return updateIfNeeded({ ...options, force: true });
}

function printJson(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

async function main(rawArgs) {
  const args = parseArgs(rawArgs);
  const action = args.action || "auto";
  const repo = args.repo || DEFAULT_REPO;
  const tag = args.tag || "latest";
  const targetDir = args["target-dir"] || managedDir();
  const force = args.force === "true";

  try {
    let result;
    if (action === "check") {
      result = await checkUpdate({ repo, tag, targetDir, force });
    } else if (action === "update") {
      result = await forceUpdate({ repo, tag, targetDir });
    } else if (action === "auto") {
      result = await updateIfNeeded({ repo, tag, targetDir, force });
    } else {
      throw new Error(`Unknown action: ${action}`);
    }
    printJson(result);
    process.exitCode = 0;
  } catch (error) {
    printJson({
      ok: false,
      installedVersion: null,
      latestVersion: null,
      targetBinary: path.join(targetDir, "codexbar"),
      needsUpdate: null,
      updated: false,
      error: error.message || String(error),
    });
    process.exitCode = 0;
  }
}

function isMainModule() {
  if (!process.argv[1]) {
    return false;
  }
  const argvPath = path.resolve(process.argv[1]);
  const argvUrl = pathToFileURL(argvPath).href;
  return import.meta.url === argvUrl || import.meta.url === `file://${argvPath}`;
}

if (isMainModule()) {
  main(process.argv.slice(2));
}
