#!/usr/bin/env bash
# Run plasmawindowed / plasmoidviewer inside an isolated virtual KWin session.
# Does not touch the host Plasma desktop (no panel restarts, no host windows).
#
# Usage:
#   ./scripts/run-virtual-plasma.sh                  # plasmawindowed package path
#   ./scripts/run-virtual-plasma.sh --viewer planar  # plasmoidviewer form factor
#   ./scripts/run-virtual-plasma.sh --viewer horizontal
#   ./scripts/run-virtual-plasma.sh --installed      # use installed plugin id
#   ./scripts/run-virtual-plasma.sh --cmd 'env'      # custom command
#   ./scripts/run-virtual-plasma.sh --width 1280 --height 720
#   ./scripts/run-virtual-plasma.sh --keep           # leave session up; print env
#   ./scripts/run-virtual-plasma.sh --stop SOCKET    # stop a --keep session
#
# Logs: /tmp/codexbar-virtual-plasma-<socket>.log
# On success prints READY line with DBUS + WAYLAND socket, then runs the app.

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/repo-env.sh
source "$repo_root/scripts/lib/repo-env.sh"
package_dir="$CODEXBAR_PACKAGE_DIR"
plugin_id="$CODEXBAR_PLUGIN_ID"
latest_dir="$CODEXBAR_VIRTUAL_LATEST_DIR"

width=1280
height=720
mode="windowed"   # windowed = plasmawindowed package; viewer = plasmoidviewer
formfactor="planar"
use_installed=0
keep=0
stop_socket=""
custom_cmd=()
timeout_secs=0

usage() {
  sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --width) width="$2"; shift 2 ;;
    --height) height="$2"; shift 2 ;;
    --viewer)
      mode="viewer"
      if [[ $# -ge 2 && "$2" != --* ]]; then
        formfactor="$2"
        shift 2
      else
        shift
      fi
      ;;
    --installed) use_installed=1; shift ;;
    --keep) keep=1; shift ;;
    --stop) stop_socket="$2"; shift 2 ;;
    --timeout) timeout_secs="$2"; shift 2 ;;
    --cmd) shift; custom_cmd=("$@"); break ;;
    --) shift; custom_cmd=("$@"); break ;;
    *)
      echo "Unknown option: $1" >&2
      usage 1
      ;;
  esac
done

runtime_dir="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

stop_session() {
  local sock="$1"
  local state="$runtime_dir/codexbar-virtual-plasma-${sock}.state"
  if [[ -f "$state" ]]; then
    # shellcheck disable=SC1090
    source "$state"
    if [[ -n "${WRAPPER_PID:-}" ]] && kill -0 "$WRAPPER_PID" 2>/dev/null; then
      kill "$WRAPPER_PID" 2>/dev/null || true
      wait "$WRAPPER_PID" 2>/dev/null || true
    fi
    rm -f "$state"
  fi
  # Best-effort: kill kwin bound to this socket by PID file only (no pkill -f self-match)
  local kwin_pid_file="$runtime_dir/codexbar-virtual-plasma-${sock}.kwin.pid"
  if [[ -f "$kwin_pid_file" ]]; then
    local kp
    kp="$(cat "$kwin_pid_file" 2>/dev/null || true)"
    if [[ -n "$kp" ]] && kill -0 "$kp" 2>/dev/null; then
      kill "$kp" 2>/dev/null || true
    fi
    rm -f "$kwin_pid_file"
  fi
  rm -f "$runtime_dir/${sock}" "$runtime_dir/${sock}.lock" 2>/dev/null || true
  echo "Stopped virtual plasma session: $sock"
}

if [[ -n "$stop_socket" ]]; then
  stop_session "$stop_socket"
  exit 0
fi

if ! command -v kwin_wayland >/dev/null 2>&1; then
  echo "kwin_wayland is required." >&2
  exit 1
fi
if ! command -v dbus-run-session >/dev/null 2>&1; then
  echo "dbus-run-session is required." >&2
  exit 1
fi

# Build app command
app_cmd=()
if [[ ${#custom_cmd[@]} -gt 0 ]]; then
  app_cmd=("${custom_cmd[@]}")
elif [[ "$mode" == "viewer" ]]; then
  if ! command -v plasmoidviewer >/dev/null 2>&1; then
    echo "plasmoidviewer is required (plasma-sdk)." >&2
    exit 1
  fi
  if [[ "$use_installed" -eq 1 ]]; then
    app_cmd=(plasmoidviewer -a "$plugin_id" -f "$formfactor")
  else
    # plasmoidviewer needs installed plugin id (path is not a package arg on Plasma 6)
    if ! kpackagetool6 --type Plasma/Applet --list 2>/dev/null | grep -q "$plugin_id"; then
      echo "Plugin $plugin_id not installed; run ./scripts/install-plasmoid.sh first or use default plasmawindowed mode." >&2
      exit 1
    fi
    app_cmd=(plasmoidviewer -a "$plugin_id" -f "$formfactor")
  fi
  case "$formfactor" in
    horizontal) app_cmd+=(-l bottomedge) ;;
    vertical) app_cmd+=(-l leftedge) ;;
  esac
else
  if ! command -v plasmawindowed >/dev/null 2>&1; then
    echo "plasmawindowed is required." >&2
    exit 1
  fi
  if [[ "$use_installed" -eq 1 ]]; then
    app_cmd=(plasmawindowed "$plugin_id")
  else
    app_cmd=(plasmawindowed "$package_dir")
  fi
fi

socket_name="wayland-codexbar-$$-$(date +%s)"
log_file="/tmp/codexbar-virtual-plasma-${socket_name}.log"
state_file="$runtime_dir/codexbar-virtual-plasma-${socket_name}.state"
kwin_pid_file="$runtime_dir/codexbar-virtual-plasma-${socket_name}.kwin.pid"
app_log="/tmp/codexbar-virtual-app-${socket_name}.log"

# Clean stale socket files for this name
rm -f "$runtime_dir/${socket_name}" "$runtime_dir/${socket_name}.lock" 2>/dev/null || true

# Serialize app command for embedding in wrapper
printf -v app_cmd_str '%q ' "${app_cmd[@]}"

wrapper_script=$(
  cat <<EOF
set -euo pipefail
echo "DBUS_SESSION_BUS_ADDRESS=\$DBUS_SESSION_BUS_ADDRESS"

cleanup() {
  if [[ -n "\${APP_PID:-}" ]] && kill -0 "\$APP_PID" 2>/dev/null; then
    kill "\$APP_PID" 2>/dev/null || true
  fi
  if [[ -n "\${KWIN_PID:-}" ]] && kill -0 "\$KWIN_PID" 2>/dev/null; then
    kill "\$KWIN_PID" 2>/dev/null || true
  fi
  if [[ -n "\${AT_SPI_PID:-}" ]] && kill -0 "\$AT_SPI_PID" 2>/dev/null; then
    kill "\$AT_SPI_PID" 2>/dev/null || true
  fi
  wait "\${APP_PID:-}" "\${KWIN_PID:-}" "\${AT_SPI_PID:-}" 2>/dev/null || true
}
trap cleanup EXIT TERM INT HUP

# Isolated AT-SPI so we do not attach to the host a11y bus
export ATSPI_DBUS_IMPLEMENTATION=dbus-daemon
if [[ -x /usr/libexec/at-spi-bus-launcher ]]; then
  /usr/libexec/at-spi-bus-launcher --launch-immediately &
elif [[ -x /usr/lib/at-spi-bus-launcher ]]; then
  /usr/lib/at-spi-bus-launcher --launch-immediately &
fi
AT_SPI_PID=\$!
sleep 0.2

dbus-update-activation-environment --systemd \
  WAYLAND_DISPLAY=${socket_name} QT_QPA_PLATFORM=wayland 2>/dev/null || \
  dbus-update-activation-environment \
  WAYLAND_DISPLAY=${socket_name} QT_QPA_PLATFORM=wayland 2>/dev/null || true

# KWin --virtual must not nest as a client of the host compositor
env -u WAYLAND_DISPLAY -u QT_QPA_PLATFORM \
  KWIN_WAYLAND_NO_PERMISSION_CHECKS=1 \
  KWIN_SCREENSHOT_NO_PERMISSION_CHECKS=1 \
  kwin_wayland --virtual --no-lockscreen \
    --width ${width} --height ${height} \
    --socket ${socket_name} &
KWIN_PID=\$!
echo "\$KWIN_PID" > "${kwin_pid_file}"

for _ in \$(seq 1 100); do
  [[ -e "${runtime_dir}/${socket_name}" ]] && break
  sleep 0.1
done
if [[ ! -e "${runtime_dir}/${socket_name}" ]]; then
  echo "KWin socket did not appear: ${socket_name}" >&2
  exit 1
fi
sleep 0.3
echo "READY"
echo "WAYLAND_DISPLAY=${socket_name}"
echo "KWIN_PID=\$KWIN_PID"

export WAYLAND_DISPLAY=${socket_name}
export QT_QPA_PLATFORM=wayland
export XDG_SESSION_TYPE=wayland

# App under isolated session
${app_cmd_str} > "${app_log}" 2>&1 &
APP_PID=\$!
echo "APP_PID=\$APP_PID"
echo "APP_LOG=${app_log}"

if [[ "${timeout_secs}" -gt 0 ]]; then
  sleep "${timeout_secs}"
  exit 0
fi

if [[ "${keep}" -eq 1 ]]; then
  # Stay up until wrapper is killed
  wait "\$KWIN_PID"
else
  # Exit when app exits (or kwin dies)
  wait "\$APP_PID"
  app_rc=\$?
  exit "\$app_rc"
fi
EOF
)

# Run isolated session
# shellcheck disable=SC2086
if [[ "$keep" -eq 1 || "$timeout_secs" -gt 0 ]]; then
  dbus-run-session -- bash -c "$wrapper_script" >"$log_file" 2>&1 &
  wrapper_pid=$!
  # Wait for READY
  ready=0
  for _ in $(seq 1 100); do
    if grep -q '^READY$' "$log_file" 2>/dev/null; then
      ready=1
      break
    fi
    if ! kill -0 "$wrapper_pid" 2>/dev/null; then
      break
    fi
    sleep 0.1
  done
  if [[ "$ready" -ne 1 ]]; then
    echo "Virtual plasma failed to start. Log: $log_file" >&2
    cat "$log_file" >&2 || true
    kill "$wrapper_pid" 2>/dev/null || true
    exit 1
  fi
  dbus_addr="$(grep -m1 '^DBUS_SESSION_BUS_ADDRESS=' "$log_file" | cut -d= -f2-)"
  {
    echo "WRAPPER_PID=$wrapper_pid"
    echo "DBUS_SESSION_BUS_ADDRESS=$dbus_addr"
    echo "WAYLAND_DISPLAY=$socket_name"
    echo "LOG_FILE=$log_file"
    echo "APP_LOG=$app_log"
  } >"$state_file"

  mkdir -p "$latest_dir"
  ln -sfn "$app_log" "$latest_dir/app.log"
  ln -sfn "$log_file" "$latest_dir/session.log"
  printf '%s\n' "$socket_name" >"$latest_dir/socket"
  {
    echo "WAYLAND_DISPLAY=$socket_name"
    echo "DBUS_SESSION_BUS_ADDRESS=$dbus_addr"
    echo "WRAPPER_PID=$wrapper_pid"
    echo "APP_LOG=$app_log"
    echo "SESSION_LOG=$log_file"
  } >"$latest_dir/env"

  cat <<OUT
READY virtual plasma
  WAYLAND_DISPLAY=$socket_name
  DBUS_SESSION_BUS_ADDRESS=$dbus_addr
  WRAPPER_PID=$wrapper_pid
  session log: $log_file
  app log:     $app_log
  state file:  $state_file
  latest:      $latest_dir/  (app.log, session.log, socket, env)
Stop with: $0 --stop $socket_name
OUT
  if [[ "$timeout_secs" -gt 0 ]]; then
    wait "$wrapper_pid" || true
    rm -f "$state_file"
  fi
else
  # Foreground: stream logs, exit with app
  mkdir -p "$latest_dir"
  ln -sfn "$app_log" "$latest_dir/app.log"
  ln -sfn "$log_file" "$latest_dir/session.log"
  printf '%s\n' "$socket_name" >"$latest_dir/socket"
  exec > >(tee "$log_file") 2>&1
  echo "Starting virtual plasma: ${app_cmd[*]}"
  echo "Socket: $socket_name  log: $log_file  app log: $app_log"
  echo "Latest pointers: $latest_dir/"
  dbus-run-session -- bash -c "$wrapper_script"
fi
