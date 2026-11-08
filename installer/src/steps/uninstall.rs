// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 handyman0

//! `uninstall`: undo `install` and `lock-screen`. Enrolled prints and the sensor itself are not touched.

use super::build_dir;
use super::lockscreen::clear_retry;
use crate::consts::*;
use crate::sys::*;
use std::fs;
use std::path::Path;

pub fn uninstall(src: Option<&Path>, apply: bool) -> Res<()> {
    let mut actions = Vec::new();
    if let Some(src) = src {
        let b = build_dir(src);
        if b.join("build.ninja").exists() {
            actions.push(cmd(
                "Remove the patched libfprint from /usr/local",
                &["ninja", "-C", &b.to_string_lossy(), "uninstall"],
                true,
            ));
        }
    }
    actions.push(cmd(
        "Remove the driver data and firmware files",
        &["rm", "-rf", SYSTEM_DATA_DIR],
        true,
    ));
    actions.push(cmd(
        "Remove the fprintd drop-in",
        &["rm", "-f", DROPIN_FILE],
        true,
    ));
    if let Ok(cfg) = fs::read_to_string(PAM_GDM_FINGERPRINT)
        && let Some(stock) = clear_retry(&cfg)
        && stock != cfg
    {
        actions.push(write_root(
            "Restore the stock lock screen PAM line",
            Path::new(PAM_GDM_FINGERPRINT),
            0o644,
            stock.into_bytes(),
        ));
    }
    actions.push(cmd(
        "Let common-auth use fprintd again",
        &["pam-auth-update", "--enable", "fprintd"],
        true,
    ));
    actions.push(cmd("Refresh the library cache", &["ldconfig"], true));
    actions.push(cmd(
        "Reload systemd units",
        &["systemctl", "daemon-reload"],
        true,
    ));
    actions.push(cmd(
        "Restart fprintd",
        &["systemctl", "restart", "fprintd"],
        true,
    ));
    println!("Enrolled fingerprints and anything written to the sensor itself are not touched.\n");
    execute(&actions, apply)
}
