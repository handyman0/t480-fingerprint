// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 handyman0

//! `firmware`: download the vendor installer, verify it and extract the firmware extension.

use crate::consts::*;
use crate::sha::{hex, sha256};
use crate::sys::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Download the vendor installer, verify it, and extract the firmware extension.
pub fn firmware(out: &Path, apply: bool) -> Res<()> {
    let exe = out.join(FIRMWARE_EXE);
    let target = out.join(FWEXT_NAME);
    println!("Source:   {FIRMWARE_URL}");
    println!("Expected: sha512 {}...", &FIRMWARE_SHA512[..24]);
    println!("Result:   {}", target.display());
    println!("The firmware belongs to its vendor and is not redistributed by this project.\n");
    for tool in ["curl", "sha512sum", "innoextract"] {
        if which(tool).is_none() {
            return Err(format!("{tool} is required (apt install {tool})"));
        }
    }
    if !apply {
        println!(
            "Dry run: would download, verify the hash, extract {FWEXT_NAME}. Repeat with --apply."
        );
        return Ok(());
    }
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    fs::set_permissions(out, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    if !exe.exists() {
        let status = std::process::Command::new("curl")
            .args(["-fSL", "--proto", "=https", "-o"])
            .arg(&exe)
            .arg(FIRMWARE_URL)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("download failed".into());
        }
    }
    let (code, sum, _) =
        capture("sha512sum", &[&exe.to_string_lossy()]).ok_or("cannot run sha512sum")?;
    let got = sum.split_whitespace().next().unwrap_or("");
    if code != 0 || got != FIRMWARE_SHA512 {
        return Err(format!(
            "SHA-512 mismatch for {}: got {got}; refusing to extract it",
            exe.display()
        ));
    }
    println!("SHA-512 matches.");
    let scratch = out.join("extracted");
    let _ = fs::remove_dir_all(&scratch);
    let status = std::process::Command::new("innoextract")
        .args(["--output-dir"])
        .arg(&scratch)
        .args(["--include", FWEXT_NAME, "--collisions", "overwrite"])
        .arg(&exe)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("innoextract failed".into());
    }
    let found = find_file(&scratch, FWEXT_NAME)
        .ok_or_else(|| format!("{FWEXT_NAME} not found in the installer"))?;
    let bytes = fs::read(&found).map_err(|e| e.to_string())?;
    fs::write(&target, &bytes).map_err(|e| e.to_string())?;
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&scratch);
    println!(
        "Extracted {} ({} bytes, sha256 {})",
        target.display(),
        bytes.len(),
        hex(&sha256(&bytes))
    );
    Ok(())
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(found) = find_file(&p, name) {
                return Some(found);
            }
        } else if p.file_name().is_some_and(|n| n == name) {
            return Some(p);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_file_searches_recursively() {
        let dir = std::env::temp_dir().join(format!("vs-find-{}", std::process::id()));
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("a/b/target.bin"), b"x").unwrap();
        assert_eq!(
            find_file(&dir, "target.bin"),
            Some(dir.join("a/b/target.bin"))
        );
        assert_eq!(find_file(&dir, "absent.bin"), None);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn firmware_hash_constant_has_sha512_length() {
        assert_eq!(FIRMWARE_SHA512.len(), 128);
        assert!(FIRMWARE_SHA512.bytes().all(|b| b.is_ascii_hexdigit()));
    }
}
