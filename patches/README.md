# libfprint patches for the Validity/Synaptics `validity` driver

`libfprint-mr626-validity-t480.patch` applies on top of libfprint merge request
[!626](https://gitlab.freedesktop.org/libfprint/libfprint/-/merge_requests/626)
(native `validity` driver), at commit `5db7da904f74f7fb912d4a18f700a9671d8ab4ac`
(branch `mr626`). The driver itself belongs to its upstream authors; this patch is
LGPL-2.1+ like the rest of libfprint and only adds local hardening and deployment
options used while bringing up `06cb:009a` on Debian:

| Change | Why |
| --- | --- |
| `LIBFPRINT_VALIDITY_ALLOW_FLASH_WRITES=1` gate | Firmware-extension upload, pairing, reset and "clear storage" erase and rewrite the sensor flash. They now fail closed unless the variable is set. |
| `LIBFPRINT_VALIDITY_DATA_DIR` | Extra, ownership-checked directory searched before the system paths for the data `.bin` files, so no package install is needed for a first test. |
| `LIBFPRINT_VALIDITY_USE_FALLBACK_IDENTITY=1` | A sensor paired by an unprivileged process uses python-validity's fallback host identity; a root service (`fprintd`) would otherwise derive a different TLS key. |
| Cancel cleanup | A verify, identify or enroll cancelled while the sensor is capturing or matching now stops the capture (`0x04`), ends a match session or the on-chip enroll session, and turns the LED off before completing. Without it the sensor stayed busy: the next open failed calibration with status `0x0450`, or the next match failed with `0x04fc`. |
| unit test | Covers the flash-write gate. |

Apply (from a checkout of the base commit):

```sh
git checkout 5db7da904f74f7fb912d4a18f700a9671d8ab4ac
git apply /path/to/t480-fingerprint/patches/libfprint-mr626-validity-t480.patch
```

See `docs/t480-debian-notes.md` for the full procedure and results. Nothing here
contains keys, serial numbers, or fingerprint data.
