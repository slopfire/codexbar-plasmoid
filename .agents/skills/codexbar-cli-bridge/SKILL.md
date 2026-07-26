---
name: codexbar-cli-bridge
description: Use when changing the Splazma CodexBar plasmoid helper or data contract between the CodexBar CLI and QML. Applies to plasmoid/contents/code/codexbar-plasmoid-helper.mjs, CLI flags, JSON normalization, provider rows, credits, status, costs, history, account/source settings, and mock CLI test data.
---

# CodexBar CLI Bridge

## Scope

The plasmoid must consume the upstream Swift CLI in `./codexbar`; do not duplicate provider fetching logic in QML.

Main file:

```text
plasmoid/contents/code/codexbar-plasmoid-helper.mjs
```

Related UI consumers:

```text
plasmoid/contents/ui/main.qml
plasmoid/contents/ui/ProviderCard.qml
plasmoid/contents/ui/UsageBarRow.qml
plasmoid/contents/ui/HistoryChart.qml
```

## CLI Contract

The helper shells out to:

```sh
codexbar usage --format json --json-only --provider <provider> --source <source>
codexbar cost --format json --json-only --provider <provider>
```

For Linux-native providers (`antigravity`, `cursor`, `devin`, `opencode`, `opencodego`) with `source=native` or
`source=native-auth`, the helper calls the bundled Rust binary at `plasmoid/contents/code/codexbar-plasmoid`
instead of `codexbar`. Antigravity `native-auth` uses user tokens under `~/.config/antigravity-usage` from
`codexbar-plasmoid login --provider antigravity` (browser OAuth) or `antigravity-usage login`; plain `native`
probes the local IDE and falls back to those tokens when idle. Login and token refresh use the desktop
OAuth *app* client extracted at runtime from the local `agy` binary (optional env/config override) — never
commit those credentials.

Optional usage flags are driven by settings:

- `--status`
- `--no-credits`
- `--account <label>`
- `--account-index <n>`
- `--all-accounts`

Always keep helper output as one JSON object on stdout. Even failures should be normalized to JSON so QML can render an error instead of failing to parse.

## Normalized Output Shape

The helper should output:

```json
{
  "ok": true,
  "generatedAt": "ISO-8601",
  "requestedProvider": "all",
  "entries": [
    {
      "provider": "codex",
      "account": "user@example.com",
      "organization": null,
      "plan": "plus",
      "source": "oauth",
      "version": "0.6.0",
      "updatedAt": "ISO-8601",
      "status": { "indicator": "none", "description": "Operational" },
      "error": null,
      "rows": [
        { "id": "primary", "title": "Session", "percentLeft": 63, "resetsAt": "ISO-8601" }
      ],
      "creditsRemaining": 112.4,
      "limitResetCredits": {
        "availableCount": 1,
        "nextExpiresAt": "2026-08-12T17:49:58Z",
        "items": [
          {
            "id": "codex-reset-credit-…",
            "title": "Full reset",
            "description": "You've been granted one free rate limit reset.",
            "status": "available",
            "resetType": "codex_rate_limits",
            "grantedAt": "2026-07-13T17:49:58Z",
            "expiresAt": "2026-08-12T17:49:58Z"
          }
        ],
        "updatedAt": "2026-07-24T14:01:21Z"
      },
      "codeReviewRemainingPercent": 91,
      "tokenUsage": {
        "sessionCostUSD": 2.45,
        "sessionTokens": 128000,
        "last30DaysCostUSD": 41.2,
        "last30DaysTokens": 2180000,
        "currencyCode": "USD",
        "sessionLabel": "Today",
        "last30DaysLabel": "30d"
      },
      "dailyUsage": [
        {
          "dayKey": "2026-06-10",
          "totalTokens": 128000,
          "costUSD": 2.45,
          "models": [
            { "name": "gpt-5.6-sol", "costUSD": 2.45, "totalTokens": 128000 }
          ],
          "limitResets": [
            { "title": "Weekly", "percentLeft": 35, "resetsAt": "2026-06-10T12:00:00Z" }
          ]
        }
      ]
    }
  ],
  "costError": null
}
```

On command failure:

```json
{
  "ok": false,
  "generatedAt": "ISO-8601",
  "requestedProvider": "all",
  "entries": [],
  "error": "CodexBar CLI not found: codexbar"
}
```

## Rules

- Use `execFileSync`, not shell concatenation, inside the helper.
- Keep command timeout bounded by the plasmoid setting.
- Preserve Linux behavior: web-backed sources may fail for providers that require macOS browser/WebKit access; surface the CLI error.
- Treat `usage.primary/secondary/tertiary.usedPercent` as used percent and convert to percent left with `100 - usedPercent` when `remainingPercent` is absent.
- Use `openaiDashboard.dailyBreakdown` for credit history when available; otherwise use `cost.daily`.
- Always pad `dailyUsage` to a continuous last-30 local-calendar-day window (zero-cost flat days when missing).
- Preserve per-day `modelBreakdowns` as `models: [{ name, costUSD, totalTokens }]`.
- Annotate calendar days where a usage row's `resetsAt` lands and `percentLeft > 0` as `limitResets` (unused limit resets).
- Map Codex `usage.codexResetCredits` to entry `limitResetCredits` (`availableCount`, `nextExpiresAt`, `items`). When the primary source is `cli`/`codex-cli` and omits that field, enrich from a best-effort oauth usage fetch.
- Cost lookup is best effort. A cost failure should populate `costError`, not discard successful usage entries.
- QML number formatting is Qt/QML, not browser JS. Use `Number(value).toLocaleString(Qt.locale(), "f", digits)`, not options objects.

## Validation

Preferred:

```sh
./scripts/agent-check.sh
```

Helper-focused:

```sh
./scripts/setup-mock-cli.sh
node --check plasmoid/contents/code/codexbar-plasmoid-helper.mjs
PATH=/tmp/codexbar-plasma-mock:$PATH \
  node plasmoid/contents/code/codexbar-plasmoid-helper.mjs --provider all --timeout 5
```

When changing visible data fields, also run virtual Plasma with the mock on `PATH`
(`./scripts/run-virtual-plasma.sh --timeout 20`) and confirm the app log has no QML
errors. Use host computer-use only if interactive a11y is required.

## References

Read `references/mock-data.md` when building or refreshing the mock CLI used for runtime verification.
