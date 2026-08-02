#!/usr/bin/env bash
# Load the Providers settings page in isolated virtual KWin and verify account
# discovery. No window is created in the host desktop session.

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
log_file="${TMPDIR:-/tmp}/codexbar-config-smoke.log"
test_package="$repo_root/tests/config-smoke-plasmoid"
pass_marker="${TMPDIR:-/tmp}/codexbar-config-smoke.pass"

if ! command -v plasmawindowed >/dev/null 2>&1; then
  echo "plasmawindowed is required for the isolated config smoke test." >&2
  exit 1
fi
if ! command -v kwin_wayland >/dev/null 2>&1; then
  echo "kwin_wayland is required for the isolated config smoke test." >&2
  exit 1
fi

mock_bin_dir="$("$repo_root/scripts/setup-mock-cli.sh" --print-bin)"
"$repo_root/scripts/setup-mock-cli.sh" >/dev/null
unlink "$pass_marker" 2>/dev/null || true

set +e
PATH="$mock_bin_dir:$PATH" \
  "$repo_root/scripts/run-virtual-plasma.sh" \
    --timeout 5 \
    --cmd plasmawindowed "$test_package" \
    >"$log_file" 2>&1
rc=$?
set -e

app_log_line="$(rg -o 'app log:[[:space:]]+/tmp/[^[:space:]]+' "$log_file" | tail -1 || true)"
app_log="$(printf '%s' "$app_log_line" | sed -E 's/^app log:[[:space:]]+//')"
if [[ -z "$app_log" || ! -f "$app_log" ]]; then
  echo "Isolated config smoke did not produce an app log (exit $rc): $log_file" >&2
  tail -80 "$log_file" >&2
  exit 1
fi

if [[ "$rc" -ne 0 || ! -f "$pass_marker" ]]; then
  echo "Isolated config smoke failed (exit $rc): $app_log" >&2
  tail -80 "$app_log" >&2
  tail -80 "$log_file" >&2
  exit 1
fi

if rg -n -i 'qt\.accessibility\.atspi|QML (Error|TypeError|ReferenceError)|CONFIG_SMOKE_FAIL|Error loading QML' "$app_log" >&2; then
  echo "Isolated config smoke logged an accessibility or QML error: $app_log" >&2
  exit 1
fi

session_log_line="$(rg -o 'session log:[[:space:]]+/tmp/[^[:space:]]+' "$log_file" | head -1 || true)"
session_log="$(printf '%s' "$session_log_line" | sed -E 's/^session log:[[:space:]]+//')"
if [[ -n "$session_log" && -f "$session_log" ]] \
    && rg -n -i 'qt\.accessibility\.atspi|AT-SPI:|org\.a11y\..*Permission denied' "$session_log" >&2; then
  echo "Isolated config smoke logged an accessibility error: $session_log" >&2
  exit 1
fi

echo "Config smoke passed: account picker discovered mock accounts (virtual KWin)."
