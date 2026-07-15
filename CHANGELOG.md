# Changelog

All notable changes to the CodexBar Plasma widget are documented in this file.

## 0.1.2 — 2026-07-15

### Maintenance

- Bump release version

## 0.1.1 — 2026-07-14

### Improvements

- Remember the last selected providers in the expanded view
- Reduce automatic refresh frequency to avoid provider rate limits
- Add configurable tray usage bar styles

## 0.1.0 — 2026-07-12

Initial public release for Plasma 6.

### Features

- Compact tray representation with provider icons or multi-provider usage bars
- Full dashboard for usage limits, status, credits, costs, and recent history
- Per-provider source selection (`auto`, CLI/local, OAuth, API, web, Linux Helper, Native Auth)
- Optional multi-provider selection in the expanded view
- Automatic CodexBar CLI updates from GitHub releases (opt-in)
- Bundled Linux helper (`codexbar-plasmoid`) for Antigravity, Cursor, Devin, OpenCode, and OpenCode Go
- Email anonymization before data reaches QML (enabled by default)

### Requirements

- KDE Plasma 6
- Node.js on `PATH` (helper scripts)
- CodexBar CLI on `PATH`, or enable **Auto-download from GitHub** in widget settings
