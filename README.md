# Splazma CodexBar Plasmoid

This repository adds a Plasma 6 widget for the CodexBar CLI in `./codexbar`.
The plasmoid shells out to the existing CLI instead of duplicating provider logic, then renders usage limits,
credits, status, local token costs, and recent history with native Plasma/Kirigami controls.

## Install

Build or install the CodexBar CLI first, then make sure `codexbar` is on `PATH`.

```sh
./scripts/install-plasmoid.sh
```

For local preview without installing:

```sh
./scripts/run-windowed.sh
```

## Configure

Open the widget configuration from Plasma and adjust:

- CLI path
- enabled providers and each provider's source, account, and all-accounts mode
- refresh interval and CLI timeout
- status, credits, cost, and history visibility
- compact representation metric

The default Linux provider set is Codex through the CLI source and Gemini through the API source. More providers can be
added from the widget settings. Source selection is stored per provider because the CodexBar CLI does not support every
source for every provider, and some web-backed sources are macOS-only.

The Plasma package ID is `org.splazma.codexbar`.

## CLI Contract

The widget uses:

```sh
codexbar usage --format json --json-only --provider <provider> --source <source>
codexbar cost --format json --json-only --provider <provider>
```

Provider status, credits, account selection, and local cost history are controlled through the plasmoid settings and
mapped to the corresponding CodexBar CLI flags. The helper calls `codexbar usage` once per configured provider so each
provider can use its own source mode.

## Native Linux CLI

Cursor, OpenCode, and OpenCode Go are macOS-only in upstream CodexBar. This repository ships a Rust binary,
`splazma-codexbar`, bundled inside the plasmoid at `plasmoid/contents/code/splazma-codexbar`. It reads browser cookies
or `~/.codexbar/config.json` manual cookie headers and calls the same provider APIs directly.

Build and bundle it:

```sh
./scripts/build-native-cli.sh
```

`./scripts/install-plasmoid.sh` and `./scripts/run-windowed.sh` build the binary automatically before installing or
previewing the widget.

Run it directly:

```sh
plasmoid/contents/code/splazma-codexbar usage --format json --json-only --provider cursor --source native
```

In widget settings, choose **Native** as the source for Cursor, OpenCode, or OpenCode Go. Linux auto mode already
prefers Native for those providers.

Authentication options:

- `~/.codexbar/config.json` provider `cookieHeader`
- `SPLAZMA_CURSOR_COOKIE`, `SPLAZMA_OPENCODE_COOKIE`, or `SPLAZMA_OPENCODEGO_COOKIE`
- Chrome/Chromium/Firefox cookie import (`secret-tool` required for encrypted Chromium cookies)
- OpenCode Go local usage from `~/.local/share/opencode/opencode.db` when web cookies are unavailable
