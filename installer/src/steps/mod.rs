// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 handyman0

//! The setup steps. Everything that touches the system is a plan: printed by default,
//! executed only with `--apply`. Flashing the sensor is deliberately not automated.
//!
//! One module per step; this file only holds what they share.

mod build;
mod fetch;
mod firmware;
mod install;
mod lockscreen;
mod uninstall;

pub use build::build;
pub use fetch::fetch_source;
pub use firmware::firmware;
pub use install::install;
pub use lockscreen::lock_screen;
#[cfg(test)]
pub use lockscreen::{clear_retry, set_retry};
pub use uninstall::uninstall;

use crate::consts::PATCH_NAME;
use crate::sys::{Res, capture};
use std::path::{Path, PathBuf};

pub fn default_patch() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../patches")
        .join(PATCH_NAME)
}

fn build_dir(src: &Path) -> PathBuf {
    src.join("build-install")
}

fn git_head(src: &Path) -> Res<String> {
    let dir = src.to_string_lossy();
    match capture("git", &["-C", &dir, "rev-parse", "HEAD"]) {
        Some((0, out, _)) => Ok(out),
        _ => Err(format!("{} is not a git checkout", src.display())),
    }
}
