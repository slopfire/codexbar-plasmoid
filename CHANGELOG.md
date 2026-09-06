# Changelog

All notable changes to the CodexBar Plasma widget are documented in this file.

## Unreleased

### Features

- Show a time-remaining marker on each popup usage bar so you can see at a glance whether the budget outlasts the window
- Add a "Pace to reset" tray bar tint (green / yellow / red) driven by CodexBar's per-window pace report
- Surface CodexBar's pace summary in the usage bar tooltip

## 0.1.8 — 2026-09-04

### Fixes

- Restore configured colors for provider icons in the dashboard, provider tabs, and panel

## 0.1.7 — 2026-08-19

### Fixes

- Stop the CodexBar CLI from crashing on Linux while scanning Codex and Claude cost

## 0.1.6 — 2026-08-11

### Fixes

- Install the CodexBarCore resource bundle with managed CLI updates so OpenRouter and other plugin providers load
- Parse credit balances from CodexBar 0.48+ plugin usage details and login-method strings

## 0.1.5 — 2026-08-08

### Features

- Improve provider account configuration in settings
- Add built-in demo provider for offline UI testing

### Fixes

- Deduplicate OpenCode Go accounts

### Maintenance

- Add isolated virtual Plasma validation and config smoke tests


## 0.1.4 — 2026-07-25

### Maintenance

- Bump release version

## 0.1.3 — 2026-07-24

### Maintenance

- Bump release version

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
