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

For Linux-native providers (`cursor`, `opencode`, `opencodego`) with `source=native`, the helper calls the bundled
Rust binary at `plasmoid/contents/code/splazma-codexbar` instead of `codexbar`.

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
        { "dayKey": "2026-06-10", "totalTokens": 128000, "costUSD": 2.45 }
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
- Cost lookup is best effort. A cost failure should populate `costError`, not discard successful usage entries.
- QML number formatting is Qt/QML, not browser JS. Use `Number(value).toLocaleString(Qt.locale(), "f", digits)`, not options objects.

## Validation

Run:

```sh
node --check plasmoid/contents/code/codexbar-plasmoid-helper.mjs
PATH=/tmp/codexbar-plasma-mock:$PATH plasmoid/contents/code/codexbar-plasmoid-helper.mjs --provider all --timeout 5
qmllint plasmoid/contents/ui/*.qml plasmoid/contents/config/config.qml
```

When changing visible data fields, verify with KWin MCP using mock data that includes at least two providers, usage rows, credits, status, cost summaries, and daily history.

## References

Read `references/mock-data.md` when building or refreshing the mock CLI used for runtime verification.
