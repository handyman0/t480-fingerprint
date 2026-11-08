#!/bin/bash
# SPDX-License-Identifier: LGPL-2.1-or-later
# Copyright (C) 2026 handyman0
#
# Build and test everything that does not need the sensor, inside a clean Debian system.
# Meant to run as root in a container: scripts/ci-debian.sh <path-to-source-tree>
#
#   docker run --rm -v "$PWD:/src:ro" debian:12 bash /src/scripts/ci-debian.sh /src
#
# What it proves: the installer builds and passes its tests, `doctor` knows the release's
# libraries, and the patched libfprint (with the validity driver) applies, compiles and passes
# its own test suite. It cannot prove the reader works: that needs the hardware.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
SRC=${1:?usage: ci-debian.sh <source tree>}
WORK=$(mktemp -d)
step() { printf '\n==== %s\n' "$*"; }

step "Debian release"
cat /etc/debian_version
. /etc/os-release && echo "$PRETTY_NAME"

step "Build dependencies"
apt-get update -qq
# udev.pc comes from `systemd-dev` on Debian 13.
EXTRA="systemd-dev"
apt-get install -y -qq --no-install-recommends \
  build-essential git curl ca-certificates pkg-config meson ninja-build \
  libglib2.0-dev libgusb-dev libssl-dev libudev-dev libgudev-1.0-dev libpixman-1-dev \
  libnss3-dev libcairo2-dev gobject-introspection libgirepository1.0-dev \
  innoextract python3 python3-gi gir1.2-glib-2.0 gir1.2-gudev-1.0 gir1.2-gusb-1.0 umockdev libumockdev-dev $EXTRA >/dev/null

cp -r "$SRC/." "$WORK/src"

step "Rust toolchain (the installer needs 1.85 or newer)"
curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable >/dev/null 2>&1
. "$HOME/.cargo/env"
rustc --version

step "Installer: build and unit tests"
cd "$WORK/src/installer"
cargo build --release --locked
cargo test --release --locked 2>&1 | grep -E "test result|FAILED|panicked"
BIN="$WORK/src/installer/target/release/t480-setup"

step "doctor (read-only)"
"$BIN" doctor || true

step "libfprint: fetch the commit the patch targets"
"$BIN" fetch-source --dir "$WORK/libfprint" --apply

step "Driver data files, built from python-validity at a pinned commit"
git clone -q https://github.com/uunicorn/python-validity.git "$WORK/pv"
git -C "$WORK/pv" checkout -q a6bbc21dce7b8b3c3cd92378a0b2579a2fb45920
"$BIN" data --python-validity "$WORK/pv" --libfprint-src "$WORK/libfprint" --out "$WORK/data" >/dev/null
test "$(find "$WORK/data" -type f | wc -l)" -eq 11 && echo "11 data files generated"
(cd "$WORK/src/installer" &&
  VALIDITY_REF="$WORK/pv" LIBFPRINT_REF="$WORK/libfprint" cargo test --release --locked -- data:: integrity:: 2>&1 | grep -E "test result|FAILED|error")

step "libfprint: apply the patch and compile"
# The installer also runs libfprint's whole test suite and fails if anything in it fails. Some of
# those tests (other vendors' drivers, emulated through umockdev) can fail inside a container for
# reasons that have nothing to do with this patch, so the verdict below compares against a baseline.
"$BIN" build --libfprint-src "$WORK/libfprint" --patch "$WORK/src/patches/libfprint-mr626-validity-t480.patch" --apply > "$WORK/build.log" 2>&1 || true
tail -4 "$WORK/build.log"

# meson exits non-zero when tests fail, and grep when nothing matches: both are data here.
failing() {
  { meson test -C "$1" --no-rebuild 2>&1 || true; } |
    { grep -E '^ *[0-9]+/[0-9]+ .*(FAIL|TIMEOUT|ERROR) ' || true; } |
    sed -E 's/^ *[0-9]+\/[0-9]+ //; s/ +(FAIL|TIMEOUT|ERROR) .*//' | sort -u
}

step "Baseline: the same test suite on the unpatched commit"
BASE=$(git -C "$WORK/libfprint" rev-parse HEAD)
git -C "$WORK/libfprint" worktree add -q -f "$WORK/lfp-base" "$BASE"
meson setup "$WORK/lfp-base/build" "$WORK/lfp-base" -Ddrivers=default -Ddoc=false -Dgtk-examples=false >/dev/null
ninja -C "$WORK/lfp-base/build" >/dev/null
failing "$WORK/lfp-base/build" > "$WORK/fail-base.txt"
failing "$WORK/libfprint/build-install" > "$WORK/fail-patched.txt"
echo "failing without the patch: $(wc -l < "$WORK/fail-base.txt")   failing with the patch: $(wc -l < "$WORK/fail-patched.txt")"
echo "failing in both (environment, not this patch):"; sed 's/^/   /' "$WORK/fail-base.txt"

step "Verdict"
NEW=$(comm -13 "$WORK/fail-base.txt" "$WORK/fail-patched.txt")
if [ -n "$NEW" ]; then echo "tests that the patch breaks:"; echo "$NEW"; exit 1; fi
# Both validity tests must run and pass: the C unit test and the end-to-end driver emulation.
OUT=$(meson test -C "$WORK/libfprint/build-install" --no-rebuild validity 2>&1 || true)
echo "$OUT" | grep -E ' / validity '
for want in '^Ok: *2' '^Fail: *0' '^Skipped: *0'; do
  echo "$OUT" | grep -q -E "$want" || { echo "the validity tests did not all run and pass (missing: $want)"; exit 1; }
done
echo "the patch breaks nothing, and both validity driver tests pass"

step "OK"
