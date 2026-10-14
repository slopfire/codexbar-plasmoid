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

`codexbar cost` also accepts `--refresh`. The helper passes it for manual refreshes only, on the `codexbar` backend — native backends keep their existing
arguments. The flag is deliberately excluded from the shared cache identity, so a manual refresh overwrites the slot other widgets read instead of creating a
refresh-only entry.

Codex cost is scanned by the bundled Rust binary first (upstream `codexbar cost` SIGSEGVs walking Codex history on Linux); only a missing-or-all-errors native result triggers one upstream `--provider codex` attempt. Claude keeps its own upstream scan.

For Linux-native providers (`antigravity`, `commandcode`, `cursor`, `devin`, `grok`, `opencode`, `opencodego`) with `source=native` or
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

Sync server flags (all optional; empty values are ignored):

- `--sync-url <base>` — when set, the helper publishes its locally fetched
  entries after each successful refresh (best effort; failures surface as
  `publish: {ok: false}` on the snapshot, never as a JSON break), and provider
  rows with `source: "remote"` read `GET <base>/v1/usage?provider=<id>` from
  the server (the bundled Rust binary's `sync-server` command) instead of
  spawning the CLI/native fetchers for that provider.
- `--sync-token <token>` — bearer token for both publish and remote reads.
- `--sync-device <id>` — device id for publications (default: hostname, sanitized).

Publish rules: only entries without `error` and only providers this device
fetches locally fly to the server, so server-sourced data can never loop back
and a failed fetch cannot clobber a device's last good snapshot. Remote rows
return the server's already-normalized merged entries (each carries
`remoteDevices` provenance); they replace any local cards for the same
provider and skip the local cost scan for that provider, since history totals
are merged server-side.

Always keep helper output as one JSON object on stdout. Even failures should be normalized to JSON so QML can render an error instead of failing to parse.

## Resilience contract

`plasmoid/contents/code/codexbar-resilience.mjs` owns failure handling; keep it
pure and unit-tested. `sharedFetch` is the only cache entry point and now
behaves as a two-tier store:

- `<namespace>-<key>.json` holds the last **successful** payload, kept past its
  freshness window so it can be served stale. Legacy files (raw payloads) are
  read as before; a legacy file whose payload is only errors is discarded rather
  than promoted to good data.
- `<namespace>-<key>.state.json` holds the failure count, last error, last
  failure payload, and the cooldown deadline. Failures are negative-cached only
  for that cooldown (30s doubling to a 10min cap, or the provider's Retry-After),
  never for the whole usage/cost window.

Rules:

- Classify every failure with `classifyFailureMessage`: `transient`, `auth`,
  `unsupported`, or `permanent`. Only transient failures are retried, and only
  `transient`/`unsupported` failures move to a fallback source.
- Three settings drive the layer, each passed from `main.qml` and clamped again
  in the helper by `boundedNumber` (an unset arg falls back to the KConfigXT
  default, so a bare helper invocation behaves the same):
  `--retry-attempts` (1-4, default 2) → `runWithRetries` attempts,
  `--cooldown-cap-seconds` (30-3600, default 180) → `cooldownCapMs`,
  `--cost-timeout` (60-1800, default 300) → the worker's `execTimeoutMs`.
- Serve the last good payload annotated with `stale`, `staleSince`, and
  `lastAttemptError` when a refresh fails. QML renders that as a soft retry note;
  a red error card is reserved for a provider with no cached data at all.
- Retry transient failures once via `runWithRetries`, bounded by the request
  timeout: a timeout consumes the budget, so only an early failure is re-run.
- `sourceFallbacks` (in the helper) maps a provider to fallback sources. Codex
  falls back from `cli` (the app-server JSON-RPC bridge) to `oauth`, which reads
  the same limits independently. Skip `--account`/`--all-accounts` for the
  `oauth` source, which rejects token-account flags.
- Cost is a background scanner, never a render dependency. `codexbar cost`
  takes 60-120s, so `fetchCostWithCommand` is called with `revalidate` (parent,
  reads cache + spawns a worker) or `synchronous: true` (the `--cost-worker`
  child, runs the scan with `--cost-timeout`, default 300s). `spawnCostWorker`
  uses `cost-scan.requested` in the cache dir as its interlock; the worker
  clears it when it finishes. Do not make the render path wait on a cost scan.
- Cost staleness never marks a whole entry stale; only a stale *usage* payload
  does. `usageUnavailable` carries the usage failure when a card falls back to
  local spend.

If you add a provider fallback, retryable error shape, or new background pass,
add it here and cover it in `tests/helper-resilience.test.mjs` (pure logic) or
`tests/helper-stale-serving.test.mjs` (helper process, mock CLI).

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
      "stale": false,
      "staleSince": null,
      "lastAttemptError": null,
      "usageUnavailable": null,
      "rows": [
        {
          "id": "primary",
          "title": "Session",
          "percentLeft": 63,
          "resetsAt": "ISO-8601",
          "windowMinutes": 300,
          "pace": {
            "willLastToReset": true,
            "deltaPercent": -23,
            "expectedUsedPercent": 60,
            "etaSeconds": null,
            "summary": "23% in reserve | Expected 60% used | Lasts until reset"
          }
        }
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
        "provenance": "listPriceEstimate",
        "historyCoverageIsEstablished": false,
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
- Carry `windowMinutes` and a normalized `pace` object on each row when the CLI reports them (native `usage.usageRows` rows supply both directly; standard windows read `usage.<window>.windowMinutes` and `item.pace.<window>`).
- `pace.summary` is CodexBar's own prose and is rendered verbatim. Leave it null for pace the helper computes itself so QML can build a translated line from `deltaPercent` / `expectedUsedPercent` / `etaSeconds`.
- Compute fallback pace only from elapsed window time. When a row has no CLI pace and its reset lands at or beyond the full window length, report no pace at all rather than a verdict the data cannot support.
- Append `usage.extraRateWindows` entries (`{ id, title, window }`) after the standard windows for both row shapes — the standard windows and native `usage.usageRows` payloads. Namespace their row ids as `extra:<id>`: a row id keys tray-bar selection and the per-window CLI pace lookup, so an unnamespaced scoped window could shadow primary/secondary/tertiary and inherit their pace report. Collapse a window the CLI reports twice; keep same-id-different-data windows as `extra:<id>#n`.
- Use `openaiDashboard.dailyBreakdown` for credit history when available; otherwise use `cost.daily`.
- Always pad `dailyUsage` to a continuous last-30 local-calendar-day window (zero-cost flat days when missing).
- Preserve per-day `modelBreakdowns` as `models: [{ name, costUSD, totalTokens }]`.
- Annotate calendar days where a usage row's `resetsAt` lands and `percentLeft > 0` as `limitResets` (unused limit resets).
- Map Codex `usage.codexResetCredits` to entry `limitResetCredits` (`availableCount`, `nextExpiresAt`, `items`). When the primary source is `cli`/`codex-cli` and omits that field, enrich from a best-effort oauth usage fetch.
- Cost lookup is best effort. A cost failure should populate `costError`, not discard successful usage entries.
- Preserve `cost.provenance` (top level or `cost.totals.provenance`) and `cost.historyCoverageIsEstablished` on `tokenUsage`, so the card can label list-price estimates and warn while the local scan is still catching up.
- QML number formatting is Qt/QML, not browser JS. Use `Number(value).toLocaleString(Qt.locale(), "f", digits)`, not options objects.

## Validation

Preferred:

```sh
./scripts/agent-check.sh
```

Helper-focused:

```sh
node --test tests/*.test.mjs          # resilience + stale-serving contracts
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
