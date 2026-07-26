#!/usr/bin/env bash
# Install a deterministic mock `codexbar` for helper/UI tests (does not touch host Plasma).
#
# Usage:
#   ./scripts/setup-mock-cli.sh
#   PATH="$(./scripts/setup-mock-cli.sh --print-bin):$PATH" ...
#
# The mock lives under $CODEXBAR_MOCK_DIR (default /tmp/codexbar-plasma-mock).

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/repo-env.sh
source "$repo_root/scripts/lib/repo-env.sh"

print_bin=0
if [[ "${1:-}" == "--print-bin" ]]; then
  print_bin=1
fi

mkdir -p "$CODEXBAR_MOCK_DIR"
mock_bin="$CODEXBAR_MOCK_DIR/codexbar"

cat >"$mock_bin" <<'MOCK'
#!/usr/bin/env bash
# Deterministic CodexBar CLI mock for plasmoid tests.
set -euo pipefail

cmd="${1:-}"
shift || true

# Tolerate common flags; ignore values we do not need for the mock payload.
provider="all"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --provider) provider="${2:-all}"; shift 2 ;;
    --format|--source|--account|--account-index|--timeout) shift 2 || true ;;
    --json-only|--status|--no-credits|--all-accounts) shift ;;
    *) shift ;;
  esac
done

iso_now="$(date -u +"%Y-%m-%dT%H:%M:%SZ" 2>/dev/null || echo "2026-07-26T12:00:00Z")"
day0="$(date -u +"%Y-%m-%d" 2>/dev/null || echo "2026-07-26")"
day1="$(date -u -d 'yesterday' +"%Y-%m-%d" 2>/dev/null || echo "2026-07-25")"

# Shape matches real CLI: top-level provider + nested usage / cost fields.
usage_json='[
  {
    "provider": "codex",
    "account": "mock-codex@example.com",
    "source": "oauth",
    "version": "0.0.0-mock",
    "credits": { "remaining": 112.4, "updatedAt": "'"$iso_now"'" },
    "usage": {
      "accountEmail": "mock-codex@example.com",
      "loginMethod": "plus",
      "updatedAt": "'"$iso_now"'",
      "primary": { "usedPercent": 37, "remainingPercent": 63, "resetsAt": "'"$iso_now"'" },
      "secondary": { "usedPercent": 20, "remainingPercent": 80, "resetsAt": "'"$iso_now"'" },
      "codeReview": { "remainingPercent": 91 },
      "codexResetCredits": {
        "availableCount": 1,
        "nextExpiresAt": "2026-08-12T17:49:58Z",
        "items": [],
        "updatedAt": "'"$iso_now"'"
      },
      "openaiDashboard": {
        "dailyBreakdown": [
          { "date": "'"$day1"'", "totalTokens": 50000, "costUSD": 1.1 },
          { "date": "'"$day0"'", "totalTokens": 128000, "costUSD": 2.45 }
        ]
      }
    },
    "status": { "indicator": "none", "description": "Operational" }
  },
  {
    "provider": "claude",
    "account": "mock-claude@example.com",
    "source": "cli",
    "version": "0.0.0-mock",
    "usage": {
      "accountEmail": "mock-claude@example.com",
      "loginMethod": "pro",
      "updatedAt": "'"$iso_now"'",
      "primary": { "usedPercent": 40, "remainingPercent": 60, "resetsAt": "'"$iso_now"'" },
      "secondary": { "usedPercent": 55, "remainingPercent": 45, "resetsAt": "'"$iso_now"'" },
      "tertiary": { "usedPercent": 10, "remainingPercent": 90, "resetsAt": "'"$iso_now"'" }
    },
    "status": { "indicator": "none", "description": "Operational" }
  }
]'

cost_json='[
  {
    "provider": "codex",
    "sessionCostUSD": 2.45,
    "sessionTokens": 128000,
    "last30DaysCostUSD": 41.2,
    "last30DaysTokens": 2180000,
    "currencyCode": "USD",
    "updatedAt": "'"$iso_now"'",
    "daily": [
      { "date": "'"$day1"'", "costUSD": 1.1, "totalTokens": 50000, "modelBreakdowns": [ { "name": "gpt-mock", "costUSD": 1.1, "totalTokens": 50000 } ] },
      { "date": "'"$day0"'", "costUSD": 2.45, "totalTokens": 128000, "modelBreakdowns": [ { "name": "gpt-mock", "costUSD": 2.45, "totalTokens": 128000 } ] }
    ]
  },
  {
    "provider": "claude",
    "sessionCostUSD": 0.8,
    "sessionTokens": 40000,
    "last30DaysCostUSD": 12.0,
    "last30DaysTokens": 500000,
    "currencyCode": "USD",
    "updatedAt": "'"$iso_now"'",
    "daily": [
      { "date": "'"$day0"'", "costUSD": 0.8, "totalTokens": 40000, "modelBreakdowns": [ { "name": "claude-mock", "costUSD": 0.8, "totalTokens": 40000 } ] }
    ]
  }
]'

case "$cmd" in
  usage)
    printf '%s\n' "$usage_json"
    ;;
  cost)
    printf '%s\n' "$cost_json"
    ;;
  --version|version)
    echo "codexbar 0.0.0-mock"
    ;;
  *)
    # Unknown subcommands: still emit JSON-ish empty for resilience tests
    echo "[]"
    ;;
esac
MOCK

chmod +x "$mock_bin"

if [[ "$print_bin" -eq 1 ]]; then
  printf '%s\n' "$CODEXBAR_MOCK_DIR"
else
  echo "Mock CLI installed: $mock_bin"
  echo "Use: PATH=$CODEXBAR_MOCK_DIR:\$PATH <command>"
fi
