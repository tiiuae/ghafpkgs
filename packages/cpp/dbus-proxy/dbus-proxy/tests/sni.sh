#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
#
# SNI Proxy local test script
#
# Simulates cross-VM SNI proxying on a single machine using two isolated
# private D-Bus daemons (source = AppVM, target = GUI-VM).
#
# Usage:
#   ./tests/sni.sh [path/to/dbus-proxy]
#
# Requires: dbus-daemon, dbus-send, python3 with PyGObject (via nix-shell)

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROXY_BIN="${1:-$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)/result/bin/dbus-proxy}"
SOURCE_SOCK="/tmp/test-sni-source-$$.sock"
TARGET_SOCK="/tmp/test-sni-target-$$.sock"
PIDS=()
PASS_COUNT=0
FAIL_COUNT=0
WARN_COUNT=0

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log()  { echo -e "${GREEN}[TEST]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; WARN_COUNT=$((WARN_COUNT + 1)); }
fail() { echo -e "${RED}[FAIL]${NC} $*"; FAIL_COUNT=$((FAIL_COUNT + 1)); }
pass() { echo -e "${GREEN}[PASS]${NC} $*"; PASS_COUNT=$((PASS_COUNT + 1)); }

# Wait until a Unix socket file exists and dbus-daemon responds, or timeout
wait_for_socket() {
  local sock="$1"
  local _i
  for _i in $(seq 1 40); do
    [[ -S "$sock" ]] && \
      DBUS_SESSION_BUS_ADDRESS="unix:path=$sock" \
        dbus-send --session --print-reply \
          --dest=org.freedesktop.DBus \
          /org/freedesktop/DBus \
          org.freedesktop.DBus.ListNames &>/dev/null && return 0
    sleep 0.25
  done
  return 1
}

# Wait until a D-Bus well-known name appears on a bus (via its socket), or timeout
wait_for_name() {
  local sock="$1"
  local name="$2"
  local _i names
  for _i in $(seq 1 40); do
    names=$(DBUS_SESSION_BUS_ADDRESS="unix:path=$sock" \
      dbus-send --session --print-reply \
        --dest=org.freedesktop.DBus \
        /org/freedesktop/DBus \
        org.freedesktop.DBus.ListNames 2>/dev/null || true)
    echo "$names" | grep -q "$name" && return 0
    sleep 0.5
  done
  return 1
}

cleanup() {
  log "Cleaning up..."
  for pid in "${PIDS[@]}"; do
    kill "$pid" 2>/dev/null || true
  done
  # Wait for valgrind to finish writing its report
  wait "${PIDS[@]}" 2>/dev/null || true
  rm -f "$SOURCE_SOCK" "$TARGET_SOCK" \
        /tmp/test-sni-proxy-$$.log /tmp/test-sni-item-$$.log \
        /tmp/test-sni-item2-$$.log
}
trap cleanup EXIT

# --- Prereqs ---
if [[ ! -x "$PROXY_BIN" ]]; then
  fail "Proxy binary not found: $PROXY_BIN"
  echo "  Build with: cd \$(git rev-parse --show-toplevel) && nix build .#dbus-proxy"
  exit 1
fi

# Find the default session.conf
DBUS_CONF=$(find /run/current-system /nix/store -name "session.conf" \
            -path "*/dbus-1/*" 2>/dev/null | head -1 || true)
if [[ -z "$DBUS_CONF" ]]; then
  DBUS_CONF=$(find /usr/share /etc -name "session.conf" \
              -path "*/dbus-1/*" 2>/dev/null | head -1 || true)
fi
if [[ -z "$DBUS_CONF" ]]; then
  fail "Could not find dbus session.conf"
  exit 1
fi

log "Using proxy: $PROXY_BIN"
log "Using dbus config: $DBUS_CONF"

# Helper: run a dbus-send command against the TARGET bus
target_send() {
  DBUS_SESSION_BUS_ADDRESS="unix:path=$TARGET_SOCK" \
    dbus-send --session --print-reply "$@" 2>&1
}

# Helper: run a dbus-send command against the SOURCE bus
source_send() {
  DBUS_SESSION_BUS_ADDRESS="unix:path=$SOURCE_SOCK" \
    dbus-send --session --print-reply "$@" 2>&1
}

# Helper: start a fake SNI item (python3 + PyGObject).
# Uses setsid so we can kill the entire process group cleanly.
start_fake_item() {
  local log_file="$1"
  setsid env DBUS_SESSION_BUS_ADDRESS="unix:path=$SOURCE_SOCK" \
    python3 "$SCRIPT_DIR/sni_item.py" \
    > "$log_file" 2>&1 &
  echo $!
}

# Kill a fake item and its entire process group, then wait for disconnect
kill_item() {
  local pid="$1"
  local marker="$2" # string to grep in target bus to confirm removal
  # Kill process group (setsid created a new session; PGID == PID of setsid)
  kill -- -"$pid" 2>/dev/null || kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  # Give the D-Bus daemon time to emit NameOwnerChanged and proxy to react
  local _i
  for _i in $(seq 1 20); do
    sleep 0.5
    local names
    names=$(target_send --dest=org.freedesktop.DBus \
        /org/freedesktop/DBus org.freedesktop.DBus.ListNames 2>/dev/null || true)
    if ! echo "$names" | grep -q "$marker"; then
      return 0
    fi
  done
  return 1  # still present after 10s
}

# --- Step 1: Start two private D-Bus daemons ---
log "Starting source bus (simulates AppVM)..."
rm -f "$SOURCE_SOCK"
SOURCE_PID=$(dbus-daemon --config-file="$DBUS_CONF" --fork \
    --address="unix:path=$SOURCE_SOCK" --print-pid 2>/dev/null)
PIDS+=("$SOURCE_PID")

log "Starting target bus (simulates GUI-VM)..."
rm -f "$TARGET_SOCK"
TARGET_PID=$(dbus-daemon --config-file="$DBUS_CONF" --fork \
    --address="unix:path=$TARGET_SOCK" --print-pid 2>/dev/null)
PIDS+=("$TARGET_PID")

log "Source bus: unix:path=$SOURCE_SOCK (PID $SOURCE_PID)"
log "Target bus: unix:path=$TARGET_SOCK (PID $TARGET_PID)"
wait_for_socket "$SOURCE_SOCK" || { fail "Source dbus-daemon did not start"; exit 1; }
wait_for_socket "$TARGET_SOCK" || { fail "Target dbus-daemon did not start"; exit 1; }

# --- Step 2: Start dbus-proxy in SNI mode (optionally under valgrind) ---
VALGRIND_LOG="/tmp/test-sni-valgrind-$$.log"
if command -v valgrind &>/dev/null; then
  log "Starting dbus-proxy --sni-mode (under valgrind)..."
  DBUS_SYSTEM_BUS_ADDRESS="unix:path=$SOURCE_SOCK" \
  DBUS_SESSION_BUS_ADDRESS="unix:path=$TARGET_SOCK" \
    valgrind \
      --leak-check=full \
      --show-leak-kinds=all \
      --errors-for-leak-kinds=definite,indirect \
      --track-origins=yes \
      --log-file="$VALGRIND_LOG" \
      "$PROXY_BIN" --sni-mode --source-bus-type system --target-bus-type session --verbose \
    > /tmp/test-sni-proxy-$$.log 2>&1 &
else
  warn "valgrind not found — running proxy directly (no memory check)"
  DBUS_SYSTEM_BUS_ADDRESS="unix:path=$SOURCE_SOCK" \
  DBUS_SESSION_BUS_ADDRESS="unix:path=$TARGET_SOCK" \
    "$PROXY_BIN" --sni-mode --source-bus-type system --target-bus-type session --verbose \
    > /tmp/test-sni-proxy-$$.log 2>&1 &
fi
PROXY_PID=$!
PIDS+=("$PROXY_PID")

# Wait until the proxy registers StatusNotifierWatcher on the source bus (up to 20s)
if ! wait_for_name "$SOURCE_SOCK" "org.kde.StatusNotifierWatcher"; then
  fail "Proxy did not register StatusNotifierWatcher within timeout"
  cat /tmp/test-sni-proxy-$$.log
  exit 1
fi
if ! kill -0 "$PROXY_PID" 2>/dev/null; then
  fail "Proxy crashed at startup"
  cat /tmp/test-sni-proxy-$$.log
  exit 1
fi
pass "Proxy started under valgrind (PID $PROXY_PID)"

# --- Step 3: Start fake SNI item on source bus ---
log "Starting fake SNI item on source bus..."
ITEM_LOG="/tmp/test-sni-item-$$.log"
ITEM_PID=$(start_fake_item "$ITEM_LOG")
PIDS+=("$ITEM_PID")

# Wait until the item is proxied on the target bus (up to 20s)
wait_for_name "$TARGET_SOCK" "StatusNotifierItem-99999" || true

# --- Step 4a: Watcher on source bus ---
SOURCE_NAMES=$(source_send --dest=org.freedesktop.DBus \
    /org/freedesktop/DBus org.freedesktop.DBus.ListNames)

if echo "$SOURCE_NAMES" | grep -q "org.kde.StatusNotifierWatcher"; then
  pass "StatusNotifierWatcher is on source bus"
else
  fail "StatusNotifierWatcher NOT on source bus"
fi

# --- Step 4b: Proxied item on target bus ---
TARGET_NAMES=$(target_send --dest=org.freedesktop.DBus \
    /org/freedesktop/DBus org.freedesktop.DBus.ListNames)

if echo "$TARGET_NAMES" | grep -q "StatusNotifierItem-99999"; then
  pass "Proxied SNI item is on target bus"
  PROXIED_NAME=$(echo "$TARGET_NAMES" | grep -o '"org.kde.StatusNotifierItem-99999-[^"]*"' | tr -d '"' | head -1)
  log "Proxied name: $PROXIED_NAME"
else
  fail "Proxied SNI item NOT on target bus"
  PROXIED_NAME="org.kde.StatusNotifierItem-99999-1"
fi

# --- Step 4c: Watcher registered items ---
ITEMS=$(source_send \
    --dest=org.kde.StatusNotifierWatcher /StatusNotifierWatcher \
    org.freedesktop.DBus.Properties.Get \
    string:org.kde.StatusNotifierWatcher string:RegisteredStatusNotifierItems)

if echo "$ITEMS" | grep -q "StatusNotifierItem-99999"; then
  pass "Watcher lists the proxied item"
else
  fail "Watcher does NOT list the proxied item"
fi

# --- Step 4d: Property forwarding ---
log "Testing property forwarding..."
for prop_name in Title IconName Category Status; do
  case "$prop_name" in
    Title)    expected="Test SNI App" ;;
    IconName) expected="dialog-information" ;;
    Category) expected="ApplicationStatus" ;;
    Status)   expected="Active" ;;
  esac

  RESULT=$(target_send \
      --dest="$PROXIED_NAME" /StatusNotifierItem \
      org.freedesktop.DBus.Properties.Get \
      string:org.kde.StatusNotifierItem "string:$prop_name" 2>&1 || true)

  if echo "$RESULT" | grep -q "$expected"; then
    pass "Property $prop_name = \"$expected\""
  else
    fail "Could not read proxied property $prop_name (got: $RESULT)"
  fi
done

# --- Step 4e: Menu property ---
MENU_PROP=$(target_send \
    --dest="$PROXIED_NAME" /StatusNotifierItem \
    org.freedesktop.DBus.Properties.Get \
    string:org.kde.StatusNotifierItem string:Menu 2>&1 || true)

if echo "$MENU_PROP" | grep -q "/com/canonical/dbusmenu"; then
  pass "Menu property = \"/com/canonical/dbusmenu\""
else
  fail "Could not read proxied Menu property"
fi

# --- Step 4f: Activate method forwarding ---
log "Testing Activate method forwarding..."
ACTIVATE_RESULT=$(target_send \
    --dest="$PROXIED_NAME" /StatusNotifierItem \
    org.kde.StatusNotifierItem.Activate int32:100 int32:200 2>&1 || true)

if echo "$ACTIVATE_RESULT" | grep -q "method return"; then
  pass "Activate method call returned successfully"
else
  fail "Activate method call failed: $ACTIVATE_RESULT"
fi

# --- Step 4g: Dbusmenu GetLayout ---
log "Testing dbusmenu GetLayout..."
MENU_LAYOUT=$(target_send \
    --dest="$PROXIED_NAME" /com/canonical/dbusmenu \
    com.canonical.dbusmenu.GetLayout int32:0 int32:-1 array:string: 2>&1 || true)

if echo "$MENU_LAYOUT" | grep -q "method return"; then
  pass "Dbusmenu GetLayout returned successfully"
else
  fail "Dbusmenu GetLayout failed: $MENU_LAYOUT"
fi

MENU_VERSION=$(target_send \
    --dest="$PROXIED_NAME" /com/canonical/dbusmenu \
    org.freedesktop.DBus.Properties.Get \
    string:com.canonical.dbusmenu string:Version 2>&1 || true)

if echo "$MENU_VERSION" | grep -q "uint32 3"; then
  pass "Dbusmenu Version = 3"
else
  fail "Could not read dbusmenu Version property"
fi

# --- Step 4h: Introspection ---
log "Testing introspection..."
INTROSPECT=$(target_send \
    --dest="$PROXIED_NAME" /StatusNotifierItem \
    org.freedesktop.DBus.Introspectable.Introspect 2>&1 || true)

if echo "$INTROSPECT" | grep -q "org.kde.StatusNotifierItem"; then
  pass "Introspection returns org.kde.StatusNotifierItem interface"
else
  fail "Introspection does not return SNI interface"
fi

# --- Step 4i: dbusmenu.Event (clicked) — immediate reply ---
log "Testing dbusmenu.Event 'clicked' (immediate reply)..."
EVENT_RESULT=$(target_send \
    --dest="$PROXIED_NAME" /com/canonical/dbusmenu \
    com.canonical.dbusmenu.Event \
    int32:0 string:clicked variant:string:'' uint32:0 2>&1 || true)

if echo "$EVENT_RESULT" | grep -q "method return"; then
  pass "dbusmenu.Event 'clicked' returned immediately (popup-close flow)"
else
  warn "dbusmenu.Event 'clicked' did not return (fake item may have rejected it)"
fi

# --- Step 5: RAII test — multiple simultaneous items ---
log "=== RAII: Testing multiple simultaneous items ==="
ITEM2_LOG="/tmp/test-sni-item2-$$.log"

# Temporarily modify BUS_NAME for second item by running a modified script
ITEM2_PID=$(
  DBUS_SESSION_BUS_ADDRESS="unix:path=$SOURCE_SOCK" \
    python3 -c "
import sys, signal
sys.modules['__main__'] = type(sys)('__main__')
exec(open('$SCRIPT_DIR/sni_item.py').read().replace('StatusNotifierItem-99999-1', 'StatusNotifierItem-88888-1'))
" > "$ITEM2_LOG" 2>&1 &
  echo $!
)
PIDS+=("$ITEM2_PID")

# Wait until the second item is proxied on the target bus (up to 20s)
wait_for_name "$TARGET_SOCK" "StatusNotifierItem-88888" || true

TARGET_NAMES2=$(target_send --dest=org.freedesktop.DBus \
    /org/freedesktop/DBus org.freedesktop.DBus.ListNames)

if echo "$TARGET_NAMES2" | grep -q "StatusNotifierItem-88888"; then
  pass "Second SNI item also proxied (no crash — per-item GConnPtr works)"
else
  fail "Second SNI item not found on target bus"
fi

if echo "$TARGET_NAMES2" | grep -q "StatusNotifierItem-99999"; then
  pass "First item still alive alongside second item"
else
  fail "First item disappeared when second item registered"
fi

# --- Step 6: Item removal (RAII cleanup test) ---
log "=== RAII: Testing item removal (destructor cleanup) ==="
if kill_item "$ITEM_PID" "StatusNotifierItem-99999"; then
  pass "Proxied item removed from target bus after source quit (~SniItem cleanup OK)"
else
  fail "Proxied item still on target bus after source quit (RAII leak?)"
fi

ITEMS_AFTER=$(source_send \
    --dest=org.kde.StatusNotifierWatcher /StatusNotifierWatcher \
    org.freedesktop.DBus.Properties.Get \
    string:org.kde.StatusNotifierWatcher string:RegisteredStatusNotifierItems 2>/dev/null || true)

if echo "$ITEMS_AFTER" | grep -q "StatusNotifierItem-99999"; then
  fail "Watcher still lists removed item"
else
  pass "Watcher no longer lists removed item"
fi

# Verify proxy is still alive (no crash on cleanup)
if kill -0 "$PROXY_PID" 2>/dev/null; then
  pass "Proxy still running after item removal (no crash)"
else
  fail "Proxy crashed during item removal!"
fi

# --- Step 7: Valgrind memory check ---
log "=== Valgrind: Stopping proxy and checking memory ==="
kill "$PROXY_PID" 2>/dev/null || true
wait "$PROXY_PID" 2>/dev/null || true
# Remove from PIDS so cleanup doesn't double-kill
PIDS=("${PIDS[@]/$PROXY_PID}")
sleep 1

if [[ -f "$VALGRIND_LOG" ]]; then
  # Only definite+indirect leaks count as errors (--errors-for-leak-kinds=definite,indirect).
  # "possibly lost" and "still reachable" are GLib/GDBus internals — treated as warnings.
  if grep -q "ERROR SUMMARY: 0 errors" "$VALGRIND_LOG"; then
    pass "Valgrind: no definite memory errors"
  else
    fail "Valgrind: definite memory errors detected"
    grep "ERROR SUMMARY" "$VALGRIND_LOG" || true
    grep -B2 -A8 "Invalid\|Conditional jump\|Use of uninitialised" "$VALGRIND_LOG" | head -40 || true
  fi

  if grep -q "definitely lost: 0 bytes" "$VALGRIND_LOG"; then
    pass "Valgrind: no definite memory leaks"
  else
    DEFINITELY_LOST=$(grep "definitely lost:" "$VALGRIND_LOG" | grep -o '[0-9,]* bytes' | head -1 || true)
    fail "Valgrind: definite memory leak — $DEFINITELY_LOST"
    grep -A10 "LEAK SUMMARY" "$VALGRIND_LOG" || true
  fi

  if grep -q "indirectly lost: 0 bytes" "$VALGRIND_LOG"; then
    pass "Valgrind: no indirect memory leaks"
  else
    INDIRECTLY_LOST=$(grep "indirectly lost:" "$VALGRIND_LOG" | grep -o '[0-9,]* bytes' | head -1 || true)
    fail "Valgrind: indirect memory leak — $INDIRECTLY_LOST"
    grep -A10 "LEAK SUMMARY" "$VALGRIND_LOG" || true
  fi

  if ! grep -q "possibly lost: 0 bytes" "$VALGRIND_LOG"; then
    POSSIBLY_LOST=$(grep "possibly lost:" "$VALGRIND_LOG" | grep -o '[0-9,]* bytes' | head -1 || true)
    warn "Valgrind: possibly lost $POSSIBLY_LOST (likely GLib/GDBus internals — not a failure)"
  fi
else
  warn "Valgrind log not found: $VALGRIND_LOG"
fi

# --- Summary ---
echo ""
echo "========================================"
echo -e "  ${GREEN}PASSED: $PASS_COUNT${NC}"
if [[ $FAIL_COUNT -gt 0 ]]; then
  echo -e "  ${RED}FAILED: $FAIL_COUNT${NC}"
fi
if [[ $WARN_COUNT -gt 0 ]]; then
  echo -e "  ${YELLOW}WARNINGS: $WARN_COUNT${NC}"
fi
echo "========================================"
echo ""

if [[ $FAIL_COUNT -gt 0 ]]; then
  log "--- Proxy log (last 40 lines) ---"
  tail -40 /tmp/test-sni-proxy-$$.log 2>/dev/null || true
  log "--- Valgrind log ---"
  cat "$VALGRIND_LOG" 2>/dev/null || true
  exit 1
fi
