// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 handyman0

//! Fixed facts about the supported sensor and where its pieces live.

pub const VID: u16 = 0x06cb;
pub const PID: u16 = 0x009a;
pub const USB_ID: &str = "06cb:009a";

/// libfprint merge request !626 (native `validity` driver): the commit the patch applies to.
pub const LIBFPRINT_URL: &str = "https://gitlab.freedesktop.org/libfprint/libfprint.git";
pub const MR_REF: &str = "refs/merge-requests/626/head";
pub const BASE_COMMIT: &str = "5db7da904f74f7fb912d4a18f700a9671d8ab4ac";
pub const PATCH_NAME: &str = "libfprint-mr626-validity-t480.patch";

/// Vendor installer that carries the firmware extension. Its SHA-512 is the value
/// published in python-validity's firmware table; the file is never redistributed here.
pub const FIRMWARE_URL: &str = "https://download.lenovo.com/pccbbs/mobiles/nz3gf07w.exe";
pub const FIRMWARE_EXE: &str = "nz3gf07w.exe";
pub const FIRMWARE_SHA512: &str = "a4a4e6058b1ea8ab721953d2cfd775a1e7bc589863d160e5ebbb90344858f147d695103677a8df0b2de0c95345df108bda97196245b067f45630038fb7c807cd";
pub const FWEXT_NAME: &str = "6_07f_lenovo_mis_qm.xpfwext";

pub const SYSTEM_DATA_DIR: &str = "/usr/share/libfprint/validity";
pub const DROPIN_FILE: &str = "/etc/systemd/system/fprintd.service.d/validity.conf";
pub const PAM_GDM_FINGERPRINT: &str = "/etc/pam.d/gdm-fingerprint";
pub const PAM_COMMON_AUTH: &str = "/etc/pam.d/common-auth";

/// `fprintd` runs as root and would derive a different TLS key from the real DMI
/// serial than the unprivileged process that paired the sensor; this keeps one identity.
pub const DROPIN_CONTENT: &str =
    "[Service]\nEnvironment=LIBFPRINT_VALIDITY_USE_FALLBACK_IDENTITY=1\n";

pub const LOCK_SCREEN_TRIES: u32 = 1000;
pub const LOCK_SCREEN_TIMEOUT: u32 = 600;

/// Debian multiarch triplet used for the libfprint install (`/usr/local/lib/<triplet>`).
pub fn multiarch() -> String {
    crate::sys::capture("dpkg-architecture", &["-qDEB_HOST_MULTIARCH"])
        .filter(|(code, out, _)| *code == 0 && !out.is_empty())
        .map(|(_, out, _)| out)
        .unwrap_or_else(|| "x86_64-linux-gnu".into())
}
