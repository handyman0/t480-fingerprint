// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 handyman0

//! `lock-screen`: keep the reader active on the GNOME lock screen until a finger matches.

use crate::consts::*;
use crate::doctor::pam_fprintd_args;
use crate::sys::*;
use std::fs;
use std::path::Path;

fn canonical_line(tries: u32, timeout: u32) -> String {
    format!("auth\trequired\tpam_fprintd.so max-tries={tries} timeout={timeout}")
}

/// Rewrite the first `auth ... pam_fprintd.so` line; `None` if the file has no such line.
pub fn set_retry(config: &str, tries: u32, timeout: u32) -> Option<String> {
    let mut done = false;
    let rewritten: Vec<String> = config
        .lines()
        .map(|l| {
            if !done && pam_fprintd_args(l).is_some() {
                done = true;
                canonical_line(tries, timeout)
            } else {
                l.to_string()
            }
        })
        .collect();
    done.then(|| rewritten.join("\n") + "\n")
}

/// Back to the stock line (no options).
pub fn clear_retry(config: &str) -> Option<String> {
    let mut done = false;
    let rewritten: Vec<String> = config
        .lines()
        .map(|l| {
            if !done && pam_fprintd_args(l).is_some() {
                done = true;
                "auth\trequired\tpam_fprintd.so".to_string()
            } else {
                l.to_string()
            }
        })
        .collect();
    done.then(|| rewritten.join("\n") + "\n")
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn lock_screen(apply: bool) -> Res<()> {
    let current = fs::read_to_string(PAM_GDM_FINGERPRINT)
        .map_err(|e| format!("cannot read {PAM_GDM_FINGERPRINT}: {e} (is gdm3 installed?)"))?;
    let updated = set_retry(&current, LOCK_SCREEN_TRIES, LOCK_SCREEN_TIMEOUT)
        .ok_or_else(|| format!("{PAM_GDM_FINGERPRINT} has no pam_fprintd auth line"))?;
    let competes = fs::read_to_string(PAM_COMMON_AUTH)
        .ok()
        .and_then(|c| pam_fprintd_args(&c))
        .is_some();
    if current == updated && !competes {
        println!("The lock screen is already configured to retry; nothing to do.");
        return Ok(());
    }
    let backup = format!("/root/t480-setup-backup/{}", unix_now());
    println!(
        "Why two changes: GNOME starts the password and fingerprint PAM stacks together. Debian's\n\
              common-auth asks pam_fprintd with max-tries=1, claims the reader first, spends its single\n\
              try and releases it, so the lock screen's own stack never gets the reader. Removing\n\
              fprintd from common-auth also stops sudo and terminals from asking for a fingerprint.\n"
    );
    let mut actions = vec![cmd(
        "Back up /etc/pam.d",
        &[
            "sh",
            "-c",
            &format!("mkdir -p {backup} && cp -a /etc/pam.d {backup}/"),
        ],
        true,
    )];
    if current != updated {
        actions.push(write_root(
            "Let the lock screen retry until the fingerprint matches",
            Path::new(PAM_GDM_FINGERPRINT),
            0o644,
            updated.into_bytes(),
        ));
    }
    if competes {
        actions.push(cmd(
            "Stop common-auth from competing for the reader",
            &["pam-auth-update", "--remove", "fprintd"],
            true,
        ));
    }
    execute(&actions, apply)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STOCK: &str = "#%PAM-1.0\nauth\trequisite\tpam_nologin.so\nauth\trequired\tpam_fprintd.so\n\
auth\toptional\tpam_gnome_keyring.so\npassword required       pam_fprintd.so\n";

    #[test]
    fn retry_rewrites_only_the_auth_line() {
        let out = set_retry(STOCK, 1000, 600).unwrap();
        assert!(out.contains("auth\trequired\tpam_fprintd.so max-tries=1000 timeout=600\n"));
        assert!(out.contains("password required       pam_fprintd.so\n"));
        assert!(out.contains("auth\trequisite\tpam_nologin.so\n"));
        assert_eq!(out.matches("max-tries").count(), 1);
    }

    #[test]
    fn retry_is_idempotent_and_reversible() {
        let once = set_retry(STOCK, 1000, 600).unwrap();
        assert_eq!(set_retry(&once, 1000, 600).unwrap(), once);
        assert_eq!(clear_retry(&once).unwrap(), STOCK);
    }

    #[test]
    fn retry_requires_a_fprintd_line() {
        assert!(set_retry("auth required pam_unix.so\n", 5, 5).is_none());
        assert!(clear_retry("").is_none());
    }
}
