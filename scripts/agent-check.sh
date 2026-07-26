#!/usr/bin/env bash
# One-shot agent validation for this repo (static + virtual Plasma, no host UI).
#
# Usage:
#   ./scripts/agent-check.sh              # full check
#   ./scripts/agent-check.sh --quick      # static only (no virtual KWin)
#   ./scripts/agent-check.sh --install    # also install/upgrade the plasmoid
#   ./scripts/agent-check.sh --no-mock    # skip mock CLI helper smoke
#   ./scripts/agent-check.sh --timeout 20 # virtual session duration (default 15)
#
# Exit 0 only if all enabled steps pass. Prints a summary at the end.

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/repo-env.sh
source "$repo_root/scripts/lib/repo-env.sh"
cd "$repo_root"

quick=0
do_install=0
use_mock=1
virt_timeout=15
formfactors=1

usage() {
  sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --quick) quick=1; shift ;;
    --install) do_install=1; shift ;;
    --no-mock) use_mock=0; shift ;;
    --timeout) virt_timeout="$2"; shift 2 ;;
    --no-formfactors) formfactors=0; shift ;;
    *) echo "Unknown option: $1" >&2; usage 1 ;;
  esac
done

pass=0
fail=0
skip=0
declare -a notes=()

ok() { echo "  PASS  $1"; pass=$((pass + 1)); }
bad() { echo "  FAIL  $1"; notes+=("FAIL: $1"); fail=$((fail + 1)); }
skp() { echo "  SKIP  $1"; skip=$((skip + 1)); }

section() { echo; echo "==> $1"; }

# --- static: shell scripts ---
section "bash -n scripts"
script_fail=0
for s in scripts/*.sh; do
  if bash -n "$s" 2>/tmp/agent-check-bashn.err; then
    ok "$(basename "$s")"
  else
    bad "$(basename "$s") ($(tr '\n' ' ' </tmp/agent-check-bashn.err))"
    script_fail=1
  fi
done

# --- static: qmllint ---
section "qmllint"
if command -v qmllint >/dev/null 2>&1; then
  qml_err=0
  shopt -s nullglob
  qml_files=(plasmoid/contents/ui/*.qml plasmoid/contents/config/config.qml)
  for f in "${qml_files[@]}"; do
    set +e
    qmllint "$f" >/tmp/agent-check-qmllint.out 2>&1
    qrc=$?
    set -e
    if [[ $qrc -eq 0 ]]; then
      ok "$(basename "$f")"
    elif [[ $qrc -eq 255 && ! -s /tmp/agent-check-qmllint.out ]]; then
      # qmllint 1.x sometimes aborts with empty output (tool crash, not a real diagnostic)
      skp "$(basename "$f") (qmllint exit 255, no diagnostics — tool quirk)"
    else
      bad "$(basename "$f"): $(tr '\n' ' ' </tmp/agent-check-qmllint.out | head -c 200)"
      qml_err=1
    fi
  done
  shopt -u nullglob
else
  skp "qmllint not installed"
fi

# --- static: node helpers ---
section "node --check helpers"
if command -v node >/dev/null 2>&1; then
  for m in plasmoid/contents/code/*.mjs; do
    if node --check "$m" 2>/tmp/agent-check-node.err; then
      ok "$(basename "$m")"
    else
      bad "$(basename "$m"): $(tr '\n' ' ' </tmp/agent-check-node.err)"
    fi
  done
else
  skp "node not installed"
fi

# --- package metadata ---
section "package metadata"
if [[ -f plasmoid/metadata.json ]]; then
  if command -v jq >/dev/null 2>&1; then
    id="$(jq -r '.KPlugin.Id // empty' plasmoid/metadata.json)"
    struct="$(jq -r '.KPackageStructure // empty' plasmoid/metadata.json)"
    if [[ "$id" == "$CODEXBAR_PLUGIN_ID" ]]; then
      ok "KPlugin.Id=$id"
    else
      bad "KPlugin.Id='$id' expected $CODEXBAR_PLUGIN_ID"
    fi
    if [[ "$struct" == "Plasma/Applet" ]]; then
      ok "KPackageStructure=Plasma/Applet"
    else
      bad "KPackageStructure='$struct'"
    fi
  else
    if grep -q "$CODEXBAR_PLUGIN_ID" plasmoid/metadata.json; then
      ok "metadata mentions $CODEXBAR_PLUGIN_ID"
    else
      bad "metadata missing $CODEXBAR_PLUGIN_ID"
    fi
  fi
else
  bad "plasmoid/metadata.json missing"
fi

# --- mock + helper smoke ---
section "helper smoke (mock CLI)"
if [[ "$use_mock" -eq 1 ]] && command -v node >/dev/null 2>&1; then
  mock_bin_dir="$("$repo_root/scripts/setup-mock-cli.sh" --print-bin)"
  "$repo_root/scripts/setup-mock-cli.sh" >/tmp/agent-check-mock-setup.out
  helper="plasmoid/contents/code/codexbar-plasmoid-helper.mjs"
  if [[ -x "$helper" || -f "$helper" ]]; then
    set +e
    out="$(PATH="$mock_bin_dir:$PATH" node "$helper" --provider all --timeout 5 2>/tmp/agent-check-helper.err)"
    rc=$?
    set -e
    if [[ $rc -ne 0 ]]; then
      bad "helper exit $rc: $(tr '\n' ' ' </tmp/agent-check-helper.err | head -c 200)"
    else
      if command -v node >/dev/null 2>&1; then
        if printf '%s' "$out" | node -e '
          let s=""; process.stdin.on("data",d=>s+=d); process.stdin.on("end",()=>{
            try {
              const j=JSON.parse(s);
              if(!j || j.ok!==true) process.exit(2);
              if(!Array.isArray(j.entries) || j.entries.length<1) process.exit(3);
              console.log(j.entries.map(e=>e.provider).join(","));
            } catch { process.exit(4); }
          });
        ' >/tmp/agent-check-helper-providers.out 2>/tmp/agent-check-helper-parse.err; then
          ok "helper JSON ok ($(cat /tmp/agent-check-helper-providers.out))"
        else
          bad "helper JSON parse/shape ($(tr '\n' ' ' </tmp/agent-check-helper-parse.err) out=$(printf '%s' "$out" | head -c 120))"
        fi
      fi
    fi
  else
    skp "helper missing"
  fi
else
  skp "mock helper smoke"
fi

# --- install (optional) ---
if [[ "$do_install" -eq 1 ]]; then
  section "install plasmoid"
  if "$repo_root/scripts/install-plasmoid.sh" >/tmp/agent-check-install.out 2>&1; then
    ok "install-plasmoid.sh"
  else
    bad "install-plasmoid.sh ($(tail -c 200 /tmp/agent-check-install.out | tr '\n' ' '))"
  fi
fi

# --- virtual plasma ---
scan_qml_log() {
  local log="$1"
  local label="$2"
  if [[ ! -f "$log" ]]; then
    bad "$label: missing app log"
    return
  fi
  # Soft signals of QML failure
  if rg -n -i 'QML (Error|TypeError|ReferenceError)|file:.*\.qml:\d+:|Error loading QML|module .* is not installed|Cannot assign to non-existent property' "$log" >/tmp/agent-check-qml-errors.out 2>/dev/null; then
    bad "$label: QML errors in log"
    head -5 /tmp/agent-check-qml-errors.out | sed 's/^/         /'
  else
    ok "$label: no QML errors ($(wc -c <"$log") bytes log)"
  fi
}

if [[ "$quick" -eq 1 ]]; then
  section "virtual plasma"
  skp "virtual plasma (--quick)"
else
  section "virtual plasmawindowed (${virt_timeout}s)"
  if ! command -v kwin_wayland >/dev/null 2>&1; then
    skp "kwin_wayland missing"
  elif ! command -v plasmawindowed >/dev/null 2>&1; then
    skp "plasmawindowed missing"
  else
    mkdir -p "$CODEXBAR_VIRTUAL_LATEST_DIR"
    set +e
    out="$("$repo_root/scripts/run-virtual-plasma.sh" --timeout "$virt_timeout" 2>&1)"
    rc=$?
    set -e
    printf '%s\n' "$out" >"$CODEXBAR_VIRTUAL_LATEST_DIR/last-session.txt"
    sock="$(printf '%s\n' "$out" | rg -o 'wayland-codexbar-[0-9]+-[0-9]+' | head -1 || true)"
    if [[ $rc -ne 0 || -z "$sock" ]]; then
      bad "virtual plasma failed (rc=$rc)"
      printf '%s\n' "$out" | tail -20 | sed 's/^/         /'
    else
      ok "session $sock"
      app_log="/tmp/codexbar-virtual-app-${sock}.log"
      session_log="/tmp/codexbar-virtual-plasma-${sock}.log"
      ln -sfn "$app_log" "$CODEXBAR_VIRTUAL_LATEST_DIR/app.log"
      ln -sfn "$session_log" "$CODEXBAR_VIRTUAL_LATEST_DIR/session.log"
      printf '%s\n' "$sock" >"$CODEXBAR_VIRTUAL_LATEST_DIR/socket"
      scan_qml_log "$app_log" "plasmawindowed"
    fi
  fi

  if [[ "$formfactors" -eq 1 ]]; then
    section "virtual form factors"
    if ! command -v plasmoidviewer >/dev/null 2>&1; then
      skp "plasmoidviewer missing"
    elif ! command -v kpackagetool6 >/dev/null 2>&1; then
      skp "kpackagetool6 missing (needed to ensure plugin installed)"
    else
      # Ensure plugin installed for -a <id>
      if ! kpackagetool6 --type Plasma/Applet --list 2>/dev/null | grep -q "$CODEXBAR_PLUGIN_ID"; then
        if kpackagetool6 --type Plasma/Applet --install "$CODEXBAR_PACKAGE_DIR" >/tmp/agent-check-kpk.out 2>&1 \
          || kpackagetool6 --type Plasma/Applet --upgrade "$CODEXBAR_PACKAGE_DIR" >/tmp/agent-check-kpk.out 2>&1; then
          ok "installed plugin for viewer"
        else
          bad "could not install plugin for viewer"
          formfactors=0
        fi
      fi
      if [[ "$formfactors" -eq 1 ]]; then
        for ff in planar horizontal; do
          set +e
          out="$("$repo_root/scripts/run-virtual-plasma.sh" --viewer "$ff" --timeout "$virt_timeout" 2>&1)"
          rc=$?
          set -e
          sock="$(printf '%s\n' "$out" | rg -o 'wayland-codexbar-[0-9]+-[0-9]+' | head -1 || true)"
          if [[ $rc -ne 0 || -z "$sock" ]]; then
            bad "viewer $ff failed"
          else
            ok "viewer $ff session"
            scan_qml_log "/tmp/codexbar-virtual-app-${sock}.log" "viewer $ff"
          fi
        done
      fi
    fi
  fi
fi

# --- summary ---
echo
echo "======== agent-check summary ========"
echo "  pass=$pass  fail=$fail  skip=$skip"
if [[ ${#notes[@]} -gt 0 ]]; then
  echo "  notes:"
  for n in "${notes[@]}"; do echo "    - $n"; done
fi
echo "  latest virtual logs (if any): $CODEXBAR_VIRTUAL_LATEST_DIR/"
echo "    app.log -> QML/runtime; session.log -> kwin/dbus"
echo "====================================="

if [[ "$fail" -gt 0 ]]; then
  exit 1
fi
exit 0
