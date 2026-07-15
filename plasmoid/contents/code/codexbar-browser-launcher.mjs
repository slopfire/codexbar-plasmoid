#!/usr/bin/env node
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";

const args = parseArgs(process.argv.slice(2));
const url = safeHttpsUrl(args.url);

if (!url) {
  process.exitCode = 1;
} else {
  launch(browserCommand(String(args.source || ""), url) || defaultBrowserCommand(url));
}

function parseArgs(rawArgs) {
  const parsed = {};
  for (let index = 0; index < rawArgs.length; index += 1) {
    const token = rawArgs[index];
    if (!token.startsWith("--")) continue;
    const key = token.slice(2);
    const next = rawArgs[index + 1];
    if (next !== undefined && !next.startsWith("--")) {
      parsed[key] = next;
      index += 1;
    }
  }
  return parsed;
}

function safeHttpsUrl(value) {
  try {
    const url = new URL(String(value || ""));
    const isLoopbackHttp = url.protocol === "http:"
      && (url.hostname === "127.0.0.1" || url.hostname === "localhost" || url.hostname === "[::1]");
    return url.protocol === "https:" || isLoopbackHttp ? url.toString() : "";
  } catch {
    return "";
  }
}

function browserCommand(source, url) {
  const chromium = source.match(/^(Chrome|Chromium|Brave|Edge|Helium) \((Default|Profile [0-9]+)\)$/);
  if (chromium) {
    const commands = {
      Chrome: ["google-chrome", "google-chrome-stable", "chrome"],
      Chromium: ["chromium", "chromium-browser"],
      Brave: ["brave-browser", "brave"],
      Edge: ["microsoft-edge", "microsoft-edge-stable"],
      Helium: ["helium"],
    };
    return { commands: commands[chromium[1]], args: [`--profile-directory=${chromium[2]}`, url] };
  }

  const firefox = source.match(/^Firefox \(([^()]+)\)$/);
  if (firefox) {
    return profileCommand(["firefox", "firefox-esr"], [path.join(os.homedir(), ".mozilla", "firefox", firefox[1])], url);
  }

  const zen = source.match(/^(Zen|Zen Flatpak) \(([^()]+)\)$/);
  if (zen) {
    const roots = zen[1] === "Zen Flatpak"
      ? [path.join(os.homedir(), ".var", "app", "app.zen_browser.zen", "zen")]
      : [path.join(os.homedir(), ".zen"), path.join(os.homedir(), ".config", "zen")];
    const profilePaths = roots.map((root) => path.join(root, zen[2]));
    const profilePath = profilePaths.find((candidate) => fs.existsSync(candidate)) || profilePaths[0];
    const commands = zen[1] === "Zen Flatpak"
      ? [["flatpak", "run", "app.zen_browser.zen"]]
      : [["zen-browser"], ["zen"]];
    return { commands: commands.map((command) => command[0]), args: [...commands[0].slice(1), "--profile", profilePath, url] };
  }

  return null;
}

function profileCommand(commands, profiles, url) {
  const profile = profiles.find((candidate) => fs.existsSync(candidate)) || profiles[0];
  return { commands, args: ["--profile", profile, url] };
}

function defaultBrowserCommand(url) {
  return { commands: ["xdg-open", "gio", "kde-open6", "kde-open5", "sensible-browser"], args: [url] };
}

function launch(command) {
  const tryCommand = (index) => {
    if (index >= command.commands.length) {
      process.exitCode = 1;
      return;
    }
    const executable = command.commands[index];
    const args = executable === "gio" ? ["open", ...command.args] : command.args;
    const child = spawn(executable, args, { detached: true, stdio: "ignore" });
    child.once("error", () => tryCommand(index + 1));
    child.unref();
  };
  tryCommand(0);
}
