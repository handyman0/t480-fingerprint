# ThinkPad T480 / T480s (06cb:009a) on Debian: working notes

Status: **in progress**. Items are marked *verified* only when observed on the
the test machine; everything else is a hypothesis or a source-code reading.

## Target
- Sensor: USB `06cb:009a` ("Synaptics Metallica MIS Touch Fingerprint Reader"),
  reported firmware 6.7 by the driver probe.
- OS: Debian 13, kernel 6.12. Distro packages: `fprintd` 1.94.5, `libfprint-2-2` 1.94.9.
- Stock `libfprint` 1.94.9 does not claim this sensor: `fprintd-list` prints
  "No devices available" *(verified)*.

## Approach
`06cb:009a` belongs to the Validity/VCSFW family. It is **not** the same
protocol as `06cb:00dc` (a different protocol and driver): do not
reuse its opcodes or pairing material.

The candidate is the native `validity` driver from libfprint merge request
!626 (still unmerged upstream). It builds on Debian *(verified)* and lists
`06cb:009a` as supported *(verified)*, with the stock tests passing.

## Findings so far
1. **Build without sudo.** Without `libudev-dev`, configure with
   `-Ddrivers=validity -Dudev_rules=disabled -Dudev_hwdb=disabled`
   (`-Dintrospection=false -Ddoc=false -Dgtk-examples=false` to trim the build).
   Installing for real later needs the udev rules.
2. **Missing runtime data.** The first run on hardware got as far as claiming
   the device and probing it, then failed with
   `Device data files not found for 06cb:009a ... Install the libfprint-validity-data package.`
   *(verified)*. That package exists only in a third-party Ubuntu PPA, which
   does not help on Debian.
3. **Data can be generated locally.** The driver reads `<dir>/*.bin` and
   `<dir>/<vid>_<pid>/*.bin`, each file being `[data][HMAC-SHA256]`. `t480-setup data` builds them
   (the integrity value is read from the libfprint checkout, not stored here) from the
   MIT-licensed [python-validity](https://github.com/uunicorn/python-validity)
   sources (read as text, never executed). A patched build can be pointed at
   the result with `LIBFPRINT_VALIDITY_DATA_DIR`, so no system install is needed.
   The generated set is accepted by the driver and the sensor *(verified)*.
4. **Flash writes are gated.** Firmware-extension upload, pairing and "clear storage"
   erase and rewrite the sensor flash. Keep them disabled
   (`LIBFPRINT_VALIDITY_ALLOW_FLASH_WRITES` unset) except for the one deliberate run
   that needs them; the example `clear-storage` reports "not supported" while the gate
   is closed. Individual prints can be removed with the `manage-prints` example.
5. **Tooling gotcha.** libfprint's `examples/enroll` reads the finger choice from
   stdin; when launched from a GUI/panel without stdin it exits at once with
   `Unknown finger selected`. Pipe the choice in (`printf '6\n' | ./examples/enroll`).

6. **Pairing works with the generated data** *(verified)*. On a blank flash
   (`partitions=0`) the driver, with flash writes explicitly enabled, completed
   the TLS handshake, partitioned the flash, wrote the TLS blob and rebooted the
   sensor, which re-enumerated on USB.
7. **Calibration crashes the sensor above ~12 KiB** *(verified)*. After pairing,
   open reaches sensor identification (type `0x0199`) and finishes calibration
   iteration 1/3. Iteration 2 embeds the first iteration's data, giving a
   13,665-byte command. The USB write completes, then the sensor drops off the
   bus (kernel: `USB disconnect`, a new device number appears). Zeroing the data
   values does not help; limiting the number of the 28 calibration lines does:
   up to 24 lines (11,841 bytes) works, 25 lines (12,297 bytes) fails, so the
   threshold sits near 12 KiB.
8. **Enrollment commands are rejected** *(verified)*. With the open sequence
   completed (using the line limit above), the first `create_enrollment`
   command returns status `0x0401` before any touch, even after the user
   storage (`StgWindsor`) was created on the blank database.
9. **Main hypothesis** *(not verified)*: the sensor's firmware extension
   (`.xpfwext`, "fwext") is not loaded. python-validity uploads it before
   calibrating, and it probably raises the command-size limit and enables the
   enrollment commands. The extension ships inside Lenovo's fingerprint driver
   installer; uploading it writes to the sensor flash, so it needs an explicit
   decision from the person owning the machine.
10. **Firmware extension fixes both** *(verified)*. The extension file
    `6_07f_lenovo_mis_qm.xpfwext` comes from Lenovo's driver installer
    `nz3gf07w.exe` (check its SHA-512 against python-validity's table; extract with
    `innoextract`, packaged in Debian). Uploaded through the driver's own path
    (`LIBFPRINT_VALIDITY_ALLOW_FLASH_WRITES=1`, once), the driver reported
    `Upload verified -- firmware v1.2, 8 modules` and the sensor rebooted. After
    that, calibration completes with all 28 lines (no 12 KiB crash), `enroll`
    finishes 8/8 and `verify` matches (`libfprint` example programs).
11. **Host identity.** Without root the driver cannot read the DMI serial and falls
   back to a fixed identity string; use the same user for every run.

## System-wide install (verified on Debian 13 with GDM)
Verified on the test machine; steps in order:
1. Install the build dependencies and the extractor: `libudev-dev`, `innoextract`, and the `-dev` package that ships `udev.pc` (`systemd-dev` on Debian 13). `t480-setup doctor` lists what is missing and what is too old.
2. Build the patched `libfprint` with `--prefix=/usr/local --libdir=lib/x86_64-linux-gnu -Ddrivers=default`
   and `sudo ninja install` (`ldconfig` makes `fprintd` load it first; the distro
   package stays untouched; undo with `sudo ninja uninstall`).
3. Put the generated `*.bin` files in `/usr/share/libfprint/validity/` (and `06cb_009a/`),
   plus the extension file `6_07f_lenovo_mis_qm.xpfwext`.
4. If the sensor was paired by an unprivileged process, give `fprintd` the same host
   identity: a systemd drop-in with `Environment=LIBFPRINT_VALIDITY_USE_FALLBACK_IDENTITY=1`.
   (The TLS key is derived from the DMI product name and serial; root sees the real
   ones, an unprivileged process does not.)
5. `fprintd-enroll -f right-index-finger "$USER"`, then `fprintd-verify`.

Pitfalls seen:
- Prints enrolled with the libfprint example programs stay on the chip next to the
  `fprintd` ones; the chip may match the old record and `fprintd` reports no-match.
  List and delete them with the `manage-prints` example before enrolling with `fprintd`.
- After deleting records on the chip, `fprintd` keeps a local copy under
  `/var/lib/fprint/<user>/`; enrolling again then hangs until the D-Bus timeout.
  Remove the stale local file.
- Debian's PAM stack already contains `pam_fprintd` with a password fallback once
  `libpam-fprintd` is installed.

## Lock screen that keeps retrying
By default the lock screen accepts only one fingerprint attempt. GNOME starts two
PAM stacks at once, `gdm-password` and `gdm-fingerprint`. Debian's `common-auth`
(pulled in by `gdm-password`) carries `pam_fprintd max-tries=1 timeout=10`, which
claims the reader first, uses its single try and releases it; the claim made by
`gdm-fingerprint` is refused (`Device was already claimed`). Fix *(verified)*:
- `sudo pam-auth-update --remove fprintd` (takes `pam_fprintd` out of `common-auth`;
  `sudo` and terminals then stop asking for a fingerprint),
- in `/etc/pam.d/gdm-fingerprint` use `auth required pam_fprintd.so max-tries=1000 timeout=600`
  (package upgrades of `gdm3` may overwrite this file).
With this, the reader stays active across failed attempts and unlocks on the first match.

Known quirk: the reader's LED does not light while waiting for a touch, although the
driver sends the same LED command as python-validity and the sensor accepts it.
The sensor still detects the touch.

## Not tested yet
- Several fingers, the GDM login screen after a cold boot, suspend/resume.

## Verified end to end
- Lock screen unlocked with the enrolled finger *(verified)*.
- A non-enrolled finger is rejected: `verify-no-match` (nine consecutive attempts in one
  session; still too few to say anything about the false-accept rate) *(verified)*.
- A light or off-centre touch on the enrolled finger can be rejected by the chip
  (`Match interrupt type=4: no match`); a firm, centred touch matches.

## Open questions
- Which Lenovo installer carries the right extension for `06cb:009a` (sources disagree on the file name)?
- Does the fw public key used by python-validity match firmware 6.7?
- Do enroll and verify work end to end, and how to install it system-wide on
  Debian (patched `libfprint` + `fprintd`, udev rules, PAM)?

## Privacy
Never publish raw logs from debug runs, enrolled templates, captures, or
anything from your local working directories.
