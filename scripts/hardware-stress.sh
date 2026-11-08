#!/bin/bash
# SPDX-License-Identifier: LGPL-2.1-or-later
# Copyright (C) 2026 handyman0
# Open / calibrate / verify / cancel cycles against the real sensor through fprintd,
# sampling the daemon after every cycle. Prints CSV. DO NOT TOUCH THE READER while it runs:
# touches change what is being tested (the cancel-versus-capture race is a separate test).
#
# usage: scripts/hardware-stress.sh [cycles] [finger]      (needs sudo for /proc/<pid>/fd and the kernel log)
set -u
N=${1:-40}
FINGER=${2:-right-index-finger}
USER_NAME=${USER:-$(id -un)}
echo "cycle,client_exit,fprintd_pid,rss_kb,hwm_kb,threads,fds,usb_dev,disconnects_total"
count_disconnects() { sudo journalctl -k --no-pager 2>/dev/null | grep -c "usb .*: USB disconnect"; }
base=$(count_disconnects)
for i in $(seq 1 "$N"); do
  timeout 4 fprintd-verify -f "$FINGER" "$USER_NAME" >/dev/null 2>&1; code=$?
  pid=$(pidof fprintd | awk '{print $1}')
  if [ -n "$pid" ]; then
    rss=$(awk '/VmRSS/{print $2}' /proc/"$pid"/status); hwm=$(awk '/VmHWM/{print $2}' /proc/"$pid"/status)
    thr=$(awk '/Threads/{print $2}' /proc/"$pid"/status); fds=$(sudo ls /proc/"$pid"/fd 2>/dev/null | wc -l)
  else pid=-; rss=-; hwm=-; thr=-; fds=-; fi
  dev=$(lsusb | awk '/06cb:009a/{gsub(":","",$4); print $4}')
  echo "$i,$code,$pid,$rss,$hwm,$thr,$fds,$dev,$(( $(count_disconnects) - base ))"
done
# exit code 124 = the client waited the whole 4 s (healthy: the cancel was clean);
# 1 = the claim or open failed; 0 = a finger matched (somebody touched the reader).
