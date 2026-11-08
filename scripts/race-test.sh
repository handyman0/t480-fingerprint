#!/bin/bash
# SPDX-License-Identifier: LGPL-2.1-or-later
# Copyright (C) 2026 handyman0
# Touch-versus-cancel race: for each delay, ask for a touch, cancel the verification
# <delay> ms after the sensor reports the finger, then check that the sensor is still healthy.
# It talks to you with desktop notifications and speech (pt-BR); follow the prompts.
#
# The touch is detected from fprintd's debug log (G_MESSAGES_DEBUG=all must be set for the
# service): during a verification the driver does not update fprintd's finger-present property.
#
# usage: scripts/race-test.sh [delay_ms ...]     (needs sudo for the journal and kernel log)
# output: CSV on stdout. health_exit: 124 = healthy (a clean 5 s wait), 1 = claim/open failed.
set -u
USER_NAME=${USER:-$(id -un)}
FINGER=${FINGER:-right-index-finger}
DELAYS=("$@"); [ ${#DELAYS[@]} -eq 0 ] && DELAYS=(0 200 400 600 800 1000 1200 1500)

NOTE_ID=0
# One transient notification that is replaced on every prompt (so nothing piles up and nothing
# stays on screen: GNOME keeps "critical" ones until dismissed), a short sound, and speech.
# Speech uses the Brazilian Portuguese voice of espeak-ng ("pt-BR"; plain "pt" is the Portugal one).
say() {
  local id
  id=$(notify-send -p -r "$NOTE_ID" -e -u normal -t 3000 "Leitor de digital" "$1" 2>/dev/null) && NOTE_ID=${id:-0}
  pw-play /usr/share/sounds/freedesktop/stereo/message.oga 2>/dev/null &
  spd-say -l pt-BR "$2" 2>/dev/null
  echo "# [$(date +%T)] $1" >&2
}
now_ms() { echo $(( $(date +%s%N) / 1000000 )); }
disconnects() { sudo journalctl -k --no-pager 2>/dev/null | grep -c "usb .*: USB disconnect"; }
warnings_since() { sudo journalctl -u fprintd --no-pager --since "$1" 2>/dev/null | grep -c -E "match_finger failed|status=0x0450|already open|dropped off the bus"; }

LIVE=$(mktemp)
sudo journalctl -u fprintd -f -n0 --no-pager -o cat >"$LIVE" 2>/dev/null &
trap 'sudo pkill -f "journalctl -u fprintd -f" 2>/dev/null; rm -f "$LIVE"' EXIT
sleep 1

echo "round,delay_ms,touched,touch_to_cancel_ms,client_alive_at_cancel,health_exit,usb_dev,new_disconnects,warnings"
base=$(disconnects); round=0
for d in "${DELAYS[@]}"; do
  round=$((round+1)); out=$(mktemp); since=$(date +"%Y-%m-%d %H:%M:%S")
  fprintd-verify -f "$FINGER" "$USER_NAME" >"$out" 2>&1 & client=$!
  for _ in $(seq 1 300); do grep -q "Verifying:" "$out" && break; sleep 0.1; done
  mark=$(stat -c %s "$LIVE")
  say "Encoste o dedo agora" "Encoste o dedo agora"
  touched=0; t0=0
  for _ in $(seq 1 3000); do
    if tail -c +$((mark+1)) "$LIVE" | grep -q "Verify interrupt: type=0x02"; then touched=1; t0=$(now_ms); break; fi
    sleep 0.01
  done
  [ $touched -eq 1 ] && sleep "$(awk "BEGIN{print $d/1000}")"
  t1=$(now_ms)
  alive=0; kill -0 "$client" 2>/dev/null && alive=1
  kill -TERM "$client" 2>/dev/null; wait "$client" 2>/dev/null; rm -f "$out"
  say "Solte o dedo e não encoste" "Solte o dedo. Não encoste."
  sleep 5
  timeout 5 fprintd-verify -f "$FINGER" "$USER_NAME" >/dev/null 2>&1; health=$?
  dev=$(lsusb | awk '/06cb:009a/{gsub(":","",$4); print $4}')
  echo "$round,$d,$touched,$([ $touched -eq 1 ] && echo $((t1-t0)) || echo -),$alive,$health,$dev,$(( $(disconnects) - base )),$(warnings_since "$since")"
  sleep 2
done
say "Teste terminado" "Teste terminado"
