// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 handyman0

//! Build the libfprint `validity` runtime data files from python-validity sources.
//!
//! The driver loads `<dir>/*.bin` (common) and `<dir>/<vid>_<pid>/*.bin` (per device).
//! Each file is `[data][HMAC-SHA256(data)]`; the key is read from the libfprint checkout (see
//! `integrity`). The check detects corruption, it does not authenticate. The python-validity
//! sources (MIT) are read as text and never executed.

use crate::sha::{hex, hmac_sha256, sha256};
use std::fs;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Common files the driver requires, in the order it loads them.
pub const COMMON_FILES: [&str; 7] = [
    "partition_sig_standard.bin",
    "partition_sig_0090.bin",
    "ca_pubkey.bin",
    "tls_password.bin",
    "gwk_sign.bin",
    "fw_pubkey_x.bin",
    "fw_pubkey_y.bin",
];

pub const DEVICE_FILES: [&str; 4] = [
    "init.bin",
    "init_clean_slate.bin",
    "reset.bin",
    "db_write_enable.bin",
];

pub struct Generated {
    pub common: Vec<(&'static str, Vec<u8>)>,
    pub device: Vec<(&'static str, Vec<u8>)>,
}

fn hexval(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Keep only hex digits (python-validity strips every non-word character).
pub fn unhex(text: &str) -> Result<Vec<u8>, String> {
    let digits: Vec<u8> = text.bytes().filter_map(hexval).collect();
    if !digits.len().is_multiple_of(2) {
        return Err("odd number of hex digits".into());
    }
    Ok(digits.chunks(2).map(|p| p[0] << 4 | p[1]).collect())
}

/// Top-level assignment `name = unhex('''...''')` or `name = unhexlify('...')`.
pub fn literal(source: &str, name: &str) -> Result<Vec<u8>, String> {
    let bytes = source.as_bytes();
    let mut from = 0;
    while let Some(rel) = source[from..].find(name) {
        let pos = from + rel;
        from = pos + name.len();
        if pos != 0 && bytes[pos - 1] != b'\n' {
            continue;
        }
        let mut rest = source[from..].trim_start_matches([' ', '\t']);
        let Some(after_eq) = rest.strip_prefix('=') else {
            continue;
        };
        rest = after_eq.trim_start_matches([' ', '\t']);
        // `unhexlify` must be tried first: `unhex` is a prefix of it.
        let Some(call) = rest
            .strip_prefix("unhexlify")
            .or_else(|| rest.strip_prefix("unhex"))
        else {
            continue;
        };
        let Some(call) = call.trim_start_matches(' ').strip_prefix('(') else {
            continue;
        };
        let call = call.trim_start();
        let (quote, body) = if let Some(b) = call.strip_prefix("'''") {
            ("'''", b)
        } else if let Some(b) = call.strip_prefix('\'') {
            ("'", b)
        } else {
            continue;
        };
        let end = body
            .find(quote)
            .ok_or_else(|| format!("unterminated literal: {name}"))?;
        return unhex(&body[..end]);
    }
    Err(format!("literal not found: {name}"))
}

/// 32-byte little-endian encoding of a hex integer such as `0xf727...`.
pub fn int_le32(text: &str) -> Result<Vec<u8>, String> {
    let digits = text.strip_prefix("0x").unwrap_or(text);
    if digits.len() > 64 {
        return Err("integer wider than 256 bits".into());
    }
    let mut be = unhex(&format!("{digits:0>64}"))?;
    be.reverse();
    Ok(be)
}

/// The two coordinates of `fwpub = ec.EllipticCurvePublicNumbers(x, y, ...)`.
pub fn fw_pubkey(tls_source: &str) -> Result<(Vec<u8>, Vec<u8>), String> {
    let marker = "fwpub = ec.EllipticCurvePublicNumbers(";
    let start = tls_source
        .find(marker)
        .ok_or("fw public key not found in tls.py")?
        + marker.len();
    let mut rest = &tls_source[start..];
    let mut coords = Vec::new();
    for _ in 0..2 {
        let i = rest
            .find("0x")
            .ok_or("fw public key coordinates not found")?;
        let digits_len = rest[i + 2..]
            .bytes()
            .take_while(|b| hexval(*b).is_some())
            .count();
        coords.push(int_le32(&rest[i..i + 2 + digits_len])?);
        rest = &rest[i + 2 + digits_len..];
    }
    let y = coords.pop().unwrap();
    let x = coords.pop().unwrap();
    Ok((x, y))
}

pub fn blob_module(vid: u16, pid: u16) -> Option<&'static str> {
    match (vid, pid) {
        (0x06cb, 0x009a) => Some("blobs_9a.py"),
        (0x138a, 0x0097) => Some("blobs_97.py"),
        (0x138a, 0x009d) => Some("blobs_9d.py"),
        (0x138a, 0x0090) => Some("blobs_90.py"),
        _ => None,
    }
}

pub fn build(python_validity: &Path, vid: u16, pid: u16) -> Result<Generated, String> {
    let module = blob_module(vid, pid)
        .ok_or_else(|| format!("no python-validity blob module known for {vid:04x}:{pid:04x}"))?;
    let src = python_validity.join("validitysensor");
    let read = |name: &str| {
        fs::read_to_string(src.join(name))
            .map_err(|e| format!("cannot read {}: {e}", src.join(name).display()))
    };
    let blobs = read(module)?;
    let tls = read("tls.py")?;
    let flash = read("init_flash.py")?;
    let (px, py) = fw_pubkey(&tls)?;
    Ok(Generated {
        common: vec![
            (COMMON_FILES[0], literal(&flash, "partition_signature")?),
            (
                COMMON_FILES[1],
                literal(&flash, "partition_signature_0090")?,
            ),
            (COMMON_FILES[2], literal(&tls, "crt_hardcoded")?),
            (COMMON_FILES[3], literal(&tls, "password_hardcoded")?),
            (COMMON_FILES[4], literal(&tls, "gwk_sign_hardcoded")?),
            (COMMON_FILES[5], px),
            (COMMON_FILES[6], py),
        ],
        device: vec![
            (DEVICE_FILES[0], literal(&blobs, "init_hardcoded")?),
            (
                DEVICE_FILES[1],
                literal(&blobs, "init_hardcoded_clean_slate")?,
            ),
            (DEVICE_FILES[2], literal(&blobs, "reset_blob")?),
            (DEVICE_FILES[3], literal(&blobs, "db_write_enable")?),
        ],
    })
}

pub fn with_trailer(data: &[u8], key: &[u8]) -> Vec<u8> {
    let mut out = data.to_vec();
    out.extend_from_slice(&hmac_sha256(key, data));
    out
}

fn private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    f.write_all(bytes).map_err(|e| e.to_string())?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())
}

/// Write the files (0700 directories, 0600 files); returns what was written.
pub fn write_all(
    out: &Path,
    vid: u16,
    pid: u16,
    generated: &Generated,
    key: &[u8],
) -> Result<Vec<(PathBuf, usize, String)>, String> {
    let device_dir = out.join(format!("{vid:04x}_{pid:04x}"));
    private_dir(out)?;
    private_dir(&device_dir)?;
    let mut written = Vec::new();
    for (dir, files) in [
        (out, &generated.common),
        (device_dir.as_path(), &generated.device),
    ] {
        for (name, data) in files {
            let path = dir.join(name);
            write_private(&path, &with_trailer(data, key))?;
            written.push((path, data.len(), hex(&sha256(data))[..12].to_string()));
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unhex_ignores_whitespace_and_rejects_odd_length() {
        assert_eq!(unhex("de ad\nBE ef").unwrap(), vec![0xde, 0xad, 0xbe, 0xef]);
        assert!(unhex("abc").is_err());
    }

    #[test]
    fn literal_handles_both_call_forms() {
        let src =
            "x = 1\nfoo = unhex('''\n01 02\n03''')\nbar = unhexlify('0a0b')\nbaz = other('00')\n";
        assert_eq!(literal(src, "foo").unwrap(), vec![1, 2, 3]);
        assert_eq!(literal(src, "bar").unwrap(), vec![0x0a, 0x0b]);
        assert!(literal(src, "baz").is_err());
        assert!(literal(src, "missing").is_err());
    }

    #[test]
    fn literal_ignores_mentions_that_are_not_assignments() {
        let src = "# foo = unhex('ff')\nuse(foo = unhex('ee'))\nfoo = unhex('01')\n";
        assert_eq!(literal(src, "foo").unwrap(), vec![1]);
    }

    #[test]
    fn literal_reports_unterminated_strings() {
        assert!(literal("foo = unhex('''0102", "foo").is_err());
    }

    #[test]
    fn coordinates_are_little_endian_and_padded() {
        let v = int_le32("0x0102").unwrap();
        assert_eq!(v.len(), 32);
        assert_eq!(&v[..2], &[0x02, 0x01]);
        assert!(v[2..].iter().all(|b| *b == 0));
        assert!(int_le32(&format!("0x{}", "f".repeat(65))).is_err());
    }

    #[test]
    fn fw_pubkey_reads_two_coordinates() {
        let src = "fwpub = ec.EllipticCurvePublicNumbers(\n 0x0a, 0x0b, ec.SECP256R1())";
        let (x, y) = fw_pubkey(src).unwrap();
        assert_eq!((x[0], y[0]), (0x0a, 0x0b));
        assert!(fw_pubkey("nothing").is_err());
    }

    #[test]
    fn trailer_is_the_hmac_of_the_data() {
        let key = [7u8; 32];
        let out = with_trailer(b"hello", &key);
        assert_eq!(out.len(), 5 + 32);
        assert_eq!(&out[..5], b"hello");
        assert_eq!(&out[5..], &hmac_sha256(&key, b"hello"));
    }

    #[test]
    fn files_are_private() {
        let dir = std::env::temp_dir().join(format!("vs-data-{}", std::process::id()));
        let g = Generated {
            common: vec![("a.bin", vec![1])],
            device: vec![("b.bin", vec![2])],
        };
        let written = write_all(&dir, 0x06cb, 0x009a, &g, &[7u8; 32]).unwrap();
        assert_eq!(written.len(), 2);
        assert_eq!(
            fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(dir.join("06cb_009a/b.bin"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Checks the real python-validity sources when `VALIDITY_REF` points at a checkout.
    #[test]
    fn real_sources_match_the_validated_hashes() {
        let Ok(reference) = std::env::var("VALIDITY_REF") else {
            return;
        };
        let g = build(Path::new(&reference), 0x06cb, 0x009a).unwrap();
        let digest = |files: &[(&str, Vec<u8>)], name: &str| {
            hex(&sha256(&files.iter().find(|(n, _)| *n == name).unwrap().1))[..12].to_string()
        };
        // Values accepted by the driver and the sensor on real hardware.
        assert_eq!(
            digest(&g.common, "partition_sig_standard.bin"),
            "aa67e62c9de7"
        );
        assert_eq!(digest(&g.common, "ca_pubkey.bin"), "ec5d900e5a79");
        assert_eq!(digest(&g.common, "fw_pubkey_x.bin"), "ef5b3f9e4338");
        assert_eq!(digest(&g.common, "fw_pubkey_y.bin"), "a1a15a4b776e");
        assert_eq!(digest(&g.device, "init.bin"), "3404634893ce");
        assert_eq!(digest(&g.device, "reset.bin"), "a2e7567075c1");
        assert_eq!(
            g.common
                .iter()
                .find(|(n, _)| *n == "ca_pubkey.bin")
                .unwrap()
                .1
                .len(),
            420
        );
    }
}
