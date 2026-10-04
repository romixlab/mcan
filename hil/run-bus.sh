#!/usr/bin/env bash
# Board <-> board HIL runner (FEATURES.md Q5a): B129A (STLink) and B135B (J-Link) on one CAN bus.
#
#   hil/run-bus.sh [scenario ...]      scenarios: frames_129_to_135 frames_135_to_129 soak_129_to_135 soak_135_to_129
#
# Each scenario starts the receiving board first (it waits up to 30 s for its first frame) and starts the sender once the receiver logged RX READY.
# Probes are picked by name; override with B129_PROBE / B135_PROBE (`VID:PID:SERIAL` from `probe-rs list`).
# Pass = both sides exit 0. Logs go to hil/bus-logs/.
set -u
cd "$(dirname "$0")"
export PATH="$HOME/.cargo/bin:$PATH"

pick() { probe-rs list | grep -i -- "$1" | head -1 | sed -E 's/.*-- ([0-9A-Fa-f]{4}:[0-9A-Fa-f]{4}:[^ ]+).*/\1/'; }
B129_PROBE=${B129_PROBE:-$(pick 'stlink')}
B135_PROBE=${B135_PROBE:-$(pick 'j-link')}
[ -n "$B129_PROBE" ] || { echo "no STLink for B129A (probe-rs list)"; exit 2; }
[ -n "$B135_PROBE" ] || { echo "no J-Link for B135B (probe-rs list)"; exit 2; }
echo "B129A probe $B129_PROBE, B135B probe $B135_PROBE"

mkdir -p bus-logs
export CARGO_TARGET_THUMBV6M_NONE_EABI_RUNNER="probe-rs run --chip STM32G0B1CEUx --probe $B129_PROBE"
export CARGO_TARGET_THUMBV7EM_NONE_EABIHF_RUNNER="probe-rs run --chip STM32H725IGKx --probe $B135_PROBE"

# Build both first so flashing starts right after (the receiver's wait is counted from its own start).
(cd b129 && cargo test --test bus --no-run -q) || exit 2
(cd b135 && cargo test --test bus --no-run -q) || exit 2

# A board keeps running its last test after probe-rs exits (a sender retries unACKed frames forever), so reset
# both before every run: the other side must not see leftovers.
reset_boards() {
  probe-rs reset --chip STM32G0B1CEUx --probe "$B129_PROBE" >/dev/null 2>&1
  probe-rs reset --chip STM32H725IGKx --probe "$B135_PROBE" >/dev/null 2>&1
}

run() { # board test log
  (cd "$1" && cargo test --test bus -q -- "$2") >"bus-logs/$3.log" 2>&1
}

# $1 scenario, $2 receiver board, $3 receiver test, $4 sender board, $5 sender test
scenario() {
  echo "== $1"
  reset_boards
  run "$2" "$3" "$1.rx" & rx=$!
  # The sender may only start once the receiver is flashed and listening, or it gives up before anyone ACKs.
  for _ in $(seq 120); do grep -q "RX READY" "bus-logs/$1.rx.log" 2>/dev/null && break; sleep 0.5; done
  grep -q "RX READY" "bus-logs/$1.rx.log" || { echo "FAIL $1: receiver never became ready"; kill $rx 2>/dev/null; failed=1; return; }
  run "$4" "$5" "$1.tx"; tx=$?
  wait $rx; rxs=$?
  if [ $tx -eq 0 ] && [ $rxs -eq 0 ]; then echo "PASS $1"; else echo "FAIL $1 (sender $tx, receiver $rxs), see hil/bus-logs/$1.*.log"; failed=1; fi
}

failed=0
reset_boards
# The relay latches, but set it explicitly: B129A has no terminator.
(cd b135 && cargo test --test bus -q -- terminate_on) >bus-logs/terminate_on.log 2>&1 || { echo "FAIL terminate_on"; exit 1; }

all="frames_129_to_135 frames_135_to_129 soak_129_to_135 soak_135_to_129"
for s in ${@:-$all}; do
  case $s in
    frames_129_to_135) scenario $s b135 receive_frames b129 send_frames ;;
    frames_135_to_129) scenario $s b129 receive_frames b135 send_frames ;;
    soak_129_to_135)   scenario $s b135 receive_soak b129 send_soak ;;
    soak_135_to_129)   scenario $s b129 receive_soak b135 send_soak ;;
    *) echo "unknown scenario $s"; failed=1 ;;
  esac
done
exit $failed
