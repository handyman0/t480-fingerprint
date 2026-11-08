// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 handyman0

//! `build`: apply the patch to a libfprint checkout, compile it and run its tests.

use super::{build_dir, git_head};
use crate::consts::*;
use crate::sys::*;
use std::path::Path;

#[derive(Debug, PartialEq)]
pub enum PatchState {
    NotApplied,
    Applied,
    Conflicts,
}

pub fn patch_state(src: &Path, patch: &Path) -> PatchState {
    let s = src.to_string_lossy();
    let p = patch.to_string_lossy();
    let ok = |extra: &[&str]| {
        let mut args = vec!["-C", &s, "apply", "--check"];
        args.extend_from_slice(extra);
        args.push(&p);
        capture("git", &args).is_some_and(|(c, _, _)| c == 0)
    };
    if ok(&[]) {
        PatchState::NotApplied
    } else if ok(&["-R"]) {
        PatchState::Applied
    } else {
        PatchState::Conflicts
    }
}

pub fn build(src: &Path, patch: &Path, allow_other_base: bool, apply: bool) -> Res<()> {
    if !patch.is_file() {
        return Err(format!("patch not found: {}", patch.display()));
    }
    let head = git_head(src)?;
    if head != BASE_COMMIT && !allow_other_base {
        return Err(format!(
            "libfprint is at {} but the patch was made against {BASE_COMMIT}.\n\
             Use `fetch-source`, or pass --allow-other-base if you know what you are doing.",
            &head[..head.len().min(12)]
        ));
    }
    let s = src.to_string_lossy().into_owned();
    let b = build_dir(src);
    let bd = b.to_string_lossy().into_owned();
    let mut actions = Vec::new();
    match patch_state(src, patch) {
        PatchState::NotApplied => actions.push(cmd(
            "Apply the validity patch (flash-write gate, data directory, fallback identity)",
            &["git", "-C", &s, "apply", &patch.to_string_lossy()],
            false,
        )),
        PatchState::Applied => println!("The patch is already applied; skipping it.\n"),
        PatchState::Conflicts => {
            return Err("the patch does not apply to this checkout (conflicts)".into());
        }
    }
    if !b.join("build.ninja").exists() {
        actions.push(cmd(
            "Configure the build (installs under /usr/local, validity and the default drivers)",
            &[
                "meson",
                "setup",
                &bd,
                &s,
                "--prefix=/usr/local",
                &format!("--libdir=lib/{}", multiarch()),
                "-Ddrivers=default",
                "-Ddoc=false",
                "-Dgtk-examples=false",
            ],
            false,
        ));
    }
    actions.push(cmd("Compile", &["ninja", "-C", &bd], false));
    actions.push(cmd(
        "Run libfprint's own tests",
        &["meson", "test", "-C", &bd],
        false,
    ));
    execute(&actions, apply)
}
