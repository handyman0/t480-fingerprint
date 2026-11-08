# t480-fingerprint

Making the Synaptics/Validity fingerprint reader `06cb:009a` (ThinkPad T480/T480s)
work on Debian 13, including fingerprint unlock on the GNOME lock screen.

Stock `libfprint` does not claim this sensor (`fprintd-list` reports "No devices
available"). This repository collects what was needed to get it working on one
machine, with the evidence behind each step and the limits of what was tested.

> **Status:** checked on Debian 13 with GNOME, on one laptop and one sensor. What is verified,
> and what is not, is listed in [docs/testing-and-robustness.md](docs/testing-and-robustness.md).

## What is here

| Path | What it is |
| --- | --- |
| [`docs/t480-debian-notes.md`](docs/t480-debian-notes.md) | The full procedure, findings, pitfalls, and what is not verified. Start here. |
| [`installer/`](installer) | `t480-setup`: a Rust command line tool (standard library only) that automates the steps in the notes, as a plan you review first. It also builds the driver's data files from the [python-validity](https://github.com/uunicorn/python-validity) sources, read as text and never executed. |
| [`docs/testing-and-robustness.md`](docs/testing-and-robustness.md) | What was tested (leaks, sanitizers, stress on the real sensor), the results, and what is not covered. |
| [`patches/`](patches) | Patch for libfprint MR !626: fail-closed flash-write gate, private data directory, opt-in fallback host identity. |

## Warnings

- Setting the sensor up **writes to its flash** (pairing, firmware extension). Those
  steps are gated behind `LIBFPRINT_VALIDITY_ALLOW_FLASH_WRITES=1` in the patch and
  should be run deliberately, once. Understand the notes before you run them.
- The firmware extension is vendor firmware and is **not** redistributed here; the
  notes explain where it comes from and how to verify it.
- Never publish logs from debug runs, enrolled templates, captures, or serial numbers.

## Credits and licensing

This work stands on others': the native `validity` driver is libfprint merge request
[!626](https://gitlab.freedesktop.org/libfprint/libfprint/-/merge_requests/626) (its authors
own it), and the protocol knowledge comes from
[python-validity](https://github.com/uunicorn/python-validity) (MIT) and the
[Validity90](https://github.com/nmikhailov/Validity90) reverse-engineering effort.

Copyright (C) 2026 handyman0. The installer, the tools and the scripts are licensed under the
GNU LGPL v2.1 or later (`LICENSE`; each file carries an SPDX line). The patch is a change to
libfprint and keeps libfprint's license and its authors' copyright.

## Compatibility

- **Supported target:** Debian 13 (trixie) with GNOME and GDM. Other releases are not supported.
- **Needed to build the patched libfprint:** OpenSSL >= 3.0, glib >= 2.68, meson >= 0.62,
  gusb >= 0.2.0, and Rust 1.85 or newer for the installer. `t480-setup doctor` checks each one
  and refuses on a system that is too old.
- **Checked without a sensor** (`scripts/ci-debian.sh`, run in a clean Debian 13 container): the
  installer builds and passes its tests, the data files are generated, the patch applies to
  libfprint, it compiles, and both validity driver tests pass. The rest of libfprint's suite
  fails in the same 17 tests with and without the patch (other vendors' emulated drivers, an
  artifact of the container). This says nothing about the reader itself.
- **Lock screen:** the retry setting edits GDM's PAM service. On another desktop `fprintd`
  itself still works, but that tuning is not applied and the command stops with a message.

## The installer

```sh
cd installer && cargo build --release   # no dependencies to download
t480-setup doctor                       # read-only diagnosis of this machine
t480-setup fetch-source --dir ./libfprint --apply
t480-setup data --python-validity /path/to/python-validity \
    --libfprint-src ./libfprint --out ./validity-data
t480-setup firmware --out ./firmware --apply
t480-setup build --libfprint-src ./libfprint --apply
t480-setup install --libfprint-src ./libfprint --data ./validity-data \
    --firmware ./firmware/6_07f_lenovo_mis_qm.xpfwext        # prints the plan
t480-setup install ... --apply          # runs it with sudo
t480-setup lock-screen --apply          # keep the reader active until a match
```

Anything that changes the system prints its plan and only runs with `--apply`.
**Provisioning the sensor (pairing, firmware upload) writes to its flash and is not
automated**: do it by hand, once, following the notes. Verified so far: the data files are
byte-identical to the ones the sensor accepted, the firmware download is hash-checked and
extracts to the file that worked, and the patch applies to a fresh libfprint checkout,
builds and passes its 40 tests. `uninstall --apply` returned a test system to the stock Debian configuration, and `install --apply`
plus `lock-screen --apply` set it up again. That was one machine with one sensor; it has not been
repeated on a second laptop or a clean virtual machine.
