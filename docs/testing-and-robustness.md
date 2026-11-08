# Testing and robustness

What was checked, how, and what the results do and do not show. Everything here was
measured on one ThinkPad T480-family laptop (Debian 13, GNOME) with one sensor firmware
(version 6.7, build 164).

## 1. Memory safety of the code

| Check | Tool | Result |
| --- | --- | --- |
| Installer, unit tests (35, 8 of them randomized stress tests) | `cargo test` | all pass |
| Installer, lints | `clippy -D warnings`, `rustfmt` | clean |
| Installer, every real command | `valgrind --leak-check=full` (fails on *possible* leaks too) | 0 errors, 0 bytes lost in `help`, `doctor`, `data`, `lock-screen`, `install`, `uninstall` |
| Driver, C unit tests (204 cases) | `valgrind` | 0 bytes definitely, indirectly or possibly lost; 0 errors |
| Driver, same tests | AddressSanitizer + UBSan + LeakSanitizer | pass |
| Driver, emulated end-to-end tests | ASan + UBSan, slow unwinder (full stacks) | no memory error, no undefined behavior; the only leaks reported belong to the Python interpreter that drives the test and to GLib start-up, none to the driver |

Two things worth knowing if you repeat this:

- With ASan's default fast unwinder, GLib frames truncate the stack to one or two entries, so
  "no driver frames in the leak report" proves nothing. Use
  `ASAN_OPTIONS=fast_unwind_on_malloc=0:malloc_context_size=40`.
- Meson's `testlog.txt` copies the **whole environment** of the session into the log. Keep test
  logs private.

## 2. Daemon stress on the real sensor

`scripts/hardware-stress.sh` runs open, calibrate, start verification, cancel cycles through
`fprintd`, sampling the daemon after each one. **Do not touch the reader while it runs.**

Clean run, 40 cycles, no touches:

| Metric | Result |
| --- | --- |
| Cycles that waited the full 4 s and were cancelled cleanly | 40 / 40 |
| `fprintd` process | the same one throughout |
| USB re-enumerations | 0 |
| Open file descriptors | constant (12) |
| Threads | constant (7) |
| Resident memory | 15,032 to 15,292 KB in the first 20 cycles (allocator warm-up), then 15,292 to 15,296 KB for the next 20: it plateaus, which is what a leak does not do |

### A real bug found this way

Before the fix, killing a client while the sensor waited for a finger left the sensor in
capture mode. The next open failed calibration with status `0x0450` ("busy"), so one cancelled
verification (for example, unlocking with the password while the fingerprint prompt was
active) could leave the reader unusable until something reset it. The driver now stops the
capture (`0x04`), ends the on-chip enroll session when there is one, and turns the LED off
before completing a cancelled verify, identify or enroll. Before the fix 1 of 40 stress cycles
worked; after it, 40 of 40.

## 3. Finger tests on the installed stack

All through `fprintd`, with the driver that includes the cancel cleanup:

| Test | Result |
| --- | --- |
| Enrolled finger, first attempt, firm and centred touch | match (several times) |
| Enrolled finger, light or off-centre touch | can be rejected: one run read a bad first capture (`error=0x080a1000`) and then reported no match; the next attempt matched |
| Non-enrolled finger | rejected each time (`Match interrupt type=4: no match`, capture without error) |
| Retry session kept open across failures (like the lock screen) | reader stays active; matched on the touch that followed |

A verification that ends because the client's timeout expired before anyone touched the reader
is not a failed match: check the sensor events (`Verify interrupt: type=0x02` is a finger down)
before concluding anything.

## 4. Not covered yet

- **Cancelling while the sensor is matching.** In a run where the reader was touched during the
  stress, every `match_finger failed: status=0x04fc` (3 of 3) came right after a verification
  cancelled in state 13, `VERIFY_WAIT_MATCH_INT` (the match had started, the result had not
  arrived), while the 15 cancellations in state 5 (waiting for the finger) were all clean. The
  match-session cleanup (`MATCH_CLEANUP`, 0x62) was added to the abort path for that reason. It is
  supported by that correlation but **not confirmed on hardware**: two controlled sweeps
  (`scripts/race-test.sh`, cancelling 0 to 1500 ms after the finger is detected) never hit the
  window, which is only a couple of hundred milliseconds wide. Two neighbouring symptoms are not
  explained and may not be covered: a `TLS response: unknown content type` error in the cycle
  after a state-13 cancel, and once the sensor dropping off the USB bus while in state 13.
- Long runs (hundreds of cycles) and suspend/resume.
- The false-accept rate. Nine consecutive attempts with a non-enrolled finger were rejected,
  which says nothing about a population.

## 5. Findings along the way

- During a **verification** the driver never updates fprintd'"'"'s `finger-present` and
  `finger-needed` properties (enrollment does). Anything that waits for them, including
  `scripts/race-test.sh` in its first version, waits forever; the touch is only visible in the
  debug log (`Verify interrupt: type=0x02`). GNOME cannot show a "place your finger" state for
  verification because of this.
- `scripts/race-test.sh` uses `notify-send` with `--transient` and a replaced notification id
  (GNOME keeps `critical` notifications on screen until dismissed) and the `pt-BR` espeak-ng
  voice (plain `pt` is the Portugal one).
