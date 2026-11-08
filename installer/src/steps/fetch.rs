// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 handyman0

//! `fetch-source`: libfprint at the commit the patch was made against.

use crate::consts::*;
use crate::sys::*;
use std::path::Path;

pub fn fetch_source(dir: &Path, apply: bool) -> Res<()> {
    if dir.exists() {
        return Err(format!(
            "{} already exists; choose a new directory",
            dir.display()
        ));
    }
    let d = dir.to_string_lossy().into_owned();
    let actions = vec![
        cmd(
            "Clone libfprint (no checkout yet)",
            &["git", "clone", "--no-checkout", LIBFPRINT_URL, &d],
            false,
        ),
        cmd(
            "Fetch merge request !626 (the native validity driver)",
            &[
                "git",
                "-C",
                &d,
                "fetch",
                "origin",
                &format!("{MR_REF}:refs/heads/mr626"),
            ],
            false,
        ),
        cmd(
            "Check out the commit the patch was made against",
            &["git", "-C", &d, "checkout", "-b", "t480-setup", BASE_COMMIT],
            false,
        ),
    ];
    execute(&actions, apply)
}
