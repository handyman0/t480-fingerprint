// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 handyman0

//! `install`: put the patched libfprint, the data files and the firmware in place.

use super::build_dir;
use crate::consts::*;
use crate::data::{COMMON_FILES, DEVICE_FILES};
use crate::sys::*;
use std::fs;
use std::path::Path;

pub fn expected_data_files() -> Vec<String> {
    COMMON_FILES
        .iter()
        .map(|f| f.to_string())
        .chain(
            DEVICE_FILES
                .iter()
                .map(|f| format!("{:04x}_{:04x}/{f}", VID, PID)),
        )
        .collect()
}

pub fn install(src: &Path, data_dir: &Path, firmware: &Path, apply: bool) -> Res<()> {
    let b = build_dir(src);
    if !b.join("build.ninja").exists() {
        return Err(format!(
            "no build in {}: run `build --apply` first",
            b.display()
        ));
    }
    let mut actions = vec![cmd(
        "Install the patched libfprint under /usr/local (the Debian package stays untouched)",
        &[
            "meson",
            "install",
            "--no-rebuild",
            "-C",
            &b.to_string_lossy(),
        ],
        true,
    )];
    for rel in expected_data_files() {
        let from = data_dir.join(&rel);
        let bytes = fs::read(&from).map_err(|e| {
            format!(
                "missing data file {}: {e} (run `data` first)",
                from.display()
            )
        })?;
        actions.push(write_root(
            "Install a driver data file",
            &Path::new(SYSTEM_DATA_DIR).join(&rel),
            0o644,
            bytes,
        ));
    }
    let fw = fs::read(firmware).map_err(|e| {
        format!(
            "firmware file {}: {e} (run `firmware` first)",
            firmware.display()
        )
    })?;
    actions.push(write_root(
        "Install the firmware extension file (vendor firmware; only read by the driver)",
        &Path::new(SYSTEM_DATA_DIR).join(FWEXT_NAME),
        0o644,
        fw,
    ));
    actions.push(write_root(
        "Make fprintd (root) use the same host identity as the process that pairs the sensor",
        Path::new(DROPIN_FILE),
        0o644,
        DROPIN_CONTENT.as_bytes().to_vec(),
    ));
    actions.push(cmd("Refresh the library cache", &["ldconfig"], true));
    actions.push(cmd(
        "Reload systemd units",
        &["systemctl", "daemon-reload"],
        true,
    ));
    actions.push(cmd(
        "Reload udev rules",
        &["udevadm", "control", "--reload"],
        true,
    ));
    actions.push(cmd(
        "Restart fprintd",
        &["systemctl", "restart", "fprintd"],
        true,
    ));
    execute(&actions, apply)?;
    if apply {
        println!(
            "\nNext: provision the sensor if it is blank (see docs/t480-debian-notes.md; that step writes to\n\
             the sensor's flash and is deliberately manual), then `fprintd-enroll`."
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eleven_data_files_are_expected() {
        let files = expected_data_files();
        assert_eq!(files.len(), 11);
        assert!(files.contains(&"06cb_009a/reset.bin".to_string()));
        assert!(files.contains(&"fw_pubkey_y.bin".to_string()));
    }

    #[test]
    fn the_dropin_sets_the_fallback_identity() {
        assert!(DROPIN_CONTENT.starts_with("[Service]\n"));
        assert!(DROPIN_CONTENT.contains("LIBFPRINT_VALIDITY_USE_FALLBACK_IDENTITY=1"));
    }
}
