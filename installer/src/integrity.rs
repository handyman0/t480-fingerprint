// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 handyman0

//! The 32-byte value the driver uses to check its data files.
//!
//! It is part of libfprint's source, so it is read from the checkout the installer already
//! has instead of being copied into this project.

use crate::sys::Res;
use std::fs;
use std::path::Path;

pub const KEY_LEN: usize = 32;
const SOURCE: &str = "libfprint/drivers/validity/validity_data.c";
const MARKER: &str = "hmac_key[";

/// Extract the byte array declared as `hmac_key[32] = { 0x.., ... };` from C source text.
pub fn parse(source: &str) -> Res<[u8; KEY_LEN]> {
    let start = source
        .find(MARKER)
        .ok_or("integrity array not found in the driver source")?;
    let body = &source[start..];
    let open = body.find('{').ok_or("integrity array has no body")?;
    let close = body[open..]
        .find("};")
        .ok_or("integrity array is not terminated")?
        + open;
    let bytes = hex_bytes(&strip_comments(&body[open + 1..close]));
    bytes
        .try_into()
        .map_err(|v: Vec<u8>| format!("integrity array has {} bytes, expected {KEY_LEN}", v.len()))
}

pub fn read(libfprint_src: &Path) -> Res<[u8; KEY_LEN]> {
    let path = libfprint_src.join(SOURCE);
    let text = fs::read_to_string(&path).map_err(|e| {
        format!(
            "cannot read {}: {e} (run `fetch-source` first)",
            path.display()
        )
    })?;
    parse(&text)
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        match rest[i..].find("*/") {
            Some(j) => rest = &rest[i + j + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

fn hex_bytes(text: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("0x") {
        let digits = &rest[i + 2..];
        if let Some(pair) = digits.get(..2).and_then(|d| u8::from_str_radix(d, 16).ok()) {
            bytes.push(pair);
        }
        rest = &rest[i + 2..];
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn array(bytes: &[u8], comment: &str) -> String {
        let items: Vec<String> = bytes.iter().map(|b| format!("0x{b:02x}")).collect();
        format!(
            "static const guint8 hmac_key[32] = {{\n  {}, /* {comment} */\n}};\n",
            items.join(", ")
        )
    }

    #[test]
    fn reads_the_array_and_ignores_comments() {
        let wanted: Vec<u8> = (0..32).collect();
        let source = array(&wanted, "text 0xff and 0x00 inside a comment");
        assert_eq!(parse(&source).unwrap().to_vec(), wanted);
    }

    #[test]
    fn rejects_wrong_length_or_missing_array() {
        assert!(parse(&array(&[1, 2, 3], "short")).is_err());
        assert!(parse(&array(&[0u8; 33], "long")).is_err());
        assert!(parse("int nothing_here;").is_err());
        assert!(parse("hmac_key[32] = { 0x01, 0x02").is_err());
    }

    #[test]
    fn a_missing_checkout_says_what_to_do() {
        let err = read(Path::new("/nonexistent-libfprint")).unwrap_err();
        assert!(err.contains("fetch-source"));
    }

    /// Reads the real libfprint checkout when `LIBFPRINT_REF` points at one.
    #[test]
    fn reads_the_real_source() {
        let Ok(dir) = std::env::var("LIBFPRINT_REF") else {
            return;
        };
        assert_eq!(read(Path::new(&dir)).unwrap().len(), KEY_LEN);
    }
}
