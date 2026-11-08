// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 handyman0

//! Stress tests: garbage input must never panic, and the invariants the installer
//! relies on must hold for thousands of random cases. Deterministic (fixed seeds).

use crate::data::{fw_pubkey, int_le32, literal, unhex, with_trailer};
use crate::doctor::pam_fprintd_args;
use crate::sha::{Sha256, hex, hmac_sha256, sha256};
use crate::steps::{clear_retry, set_retry};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*: small, fast, good enough for test inputs.
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }

    /// Text biased toward the characters the parsers care about.
    fn tricky_text(&mut self, len: usize) -> String {
        const POOL: &[&str] = &[
            "=",
            "'",
            "'''",
            "(",
            ")",
            "unhex",
            "unhexlify",
            "0x",
            "\n",
            " ",
            "\t",
            "[",
            "]",
            "#",
            "auth",
            "required",
            "pam_fprintd.so",
            "max-tries=",
            "fwpub = ec.EllipticCurvePublicNumbers(",
            "name",
            "ff",
            "a",
            "Z",
            "é",
            "\u{0}",
            "\r",
        ];
        (0..len).map(|_| POOL[self.below(POOL.len())]).collect()
    }
}

#[test]
fn parsers_never_panic_on_random_bytes() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for round in 0..4000 {
        let len = rng.below(400);
        let text = String::from_utf8_lossy(&rng.bytes(len)).into_owned();
        let _ = unhex(&text);
        let _ = literal(&text, "name");
        let _ = fw_pubkey(&text);
        let _ = int_le32(&text);
        let _ = pam_fprintd_args(&text);
        let _ = set_retry(&text, 1000, 600);
        let _ = clear_retry(&text);
        assert!(round < 4000);
    }
}

#[test]
fn parsers_never_panic_on_tricky_text() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    for _ in 0..20_000 {
        let n = rng.below(40);
        let text = rng.tricky_text(n);
        let _ = unhex(&text);
        let _ = literal(&text, "name");
        let _ = fw_pubkey(&text);
        let _ = int_le32(&text);
        let _ = pam_fprintd_args(&text);
        let _ = set_retry(&text, 7, 7);
        let _ = clear_retry(&text);
    }
}

#[test]
fn unhex_inverts_hex_for_random_buffers() {
    let mut rng = Rng(42);
    for _ in 0..2000 {
        let len = rng.below(600);
        let data = rng.bytes(len);
        assert_eq!(unhex(&hex(&data)).unwrap(), data);
    }
}

#[test]
fn literal_round_trips_embedded_hex() {
    let mut rng = Rng(7);
    for _ in 0..500 {
        let len = rng.below(300);
        let data = rng.bytes(len);
        // Wrap the hex over several lines like python-validity does.
        let wrapped: Vec<String> = hex(&data)
            .as_bytes()
            .chunks(100)
            .map(|c| String::from_utf8_lossy(c).into_owned())
            .collect();
        let source = format!(
            "junk = 1\nname = unhex('''\n{}\n''')\nafter = 2\n",
            wrapped.join("\n")
        );
        assert_eq!(literal(&source, "name").unwrap(), data);
    }
}

#[test]
fn incremental_sha256_matches_one_shot_for_any_split() {
    let mut rng = Rng(99);
    let data = rng.bytes(300_000);
    let expected = sha256(&data);
    for _ in 0..40 {
        let mut hasher = Sha256::new();
        let mut rest = data.as_slice();
        while !rest.is_empty() {
            let take = (rng.below(5000) + 1).min(rest.len());
            hasher.update(&rest[..take]);
            rest = &rest[take..];
        }
        assert_eq!(hasher.finish(), expected);
    }
}

#[test]
fn trailer_detects_any_single_bit_flip() {
    let mut rng = Rng(5);
    for _ in 0..300 {
        let len = rng.below(200) + 1;
        let data = rng.bytes(len);
        let key = rng.bytes(32);
        let sealed = with_trailer(&data, &key);
        let (body, tag) = sealed.split_at(data.len());
        assert_eq!(tag, hmac_sha256(&key, body));
        let mut tampered = body.to_vec();
        let i = rng.below(tampered.len());
        tampered[i] ^= 1 << rng.below(8);
        assert_ne!(tag, hmac_sha256(&key, &tampered));
    }
}

#[test]
fn pam_rewrite_is_idempotent_and_reversible_on_random_configs() {
    const LINES: &[&str] = &[
        "#%PAM-1.0",
        "auth\trequisite\tpam_nologin.so",
        "auth\toptional\tpam_gnome_keyring.so",
        "password required       pam_fprintd.so",
        "@include common-account",
        "session required pam_limits.so",
        "# auth required pam_fprintd.so",
        "",
    ];
    let mut rng = Rng(2024);
    for _ in 0..2000 {
        let mut lines: Vec<String> = (0..rng.below(12))
            .map(|_| LINES[rng.below(LINES.len())].to_string())
            .collect();
        let at = rng.below(lines.len() + 1);
        lines.insert(at, "auth\trequired\tpam_fprintd.so".to_string());
        let stock = lines.join("\n") + "\n";
        let tuned = set_retry(&stock, 1000, 600).unwrap();
        assert_eq!(
            set_retry(&tuned, 1000, 600).unwrap(),
            tuned,
            "not idempotent"
        );
        assert_eq!(clear_retry(&tuned).unwrap(), stock, "not reversible");
        assert_eq!(
            tuned.matches("max-tries").count(),
            1,
            "only one line may change"
        );
    }
}

#[test]
fn many_dry_run_plans_leave_nothing_behind() {
    use crate::sys::{cmd, execute};
    let marker = std::env::temp_dir().join(format!("vs-stress-{}", std::process::id()));
    let actions = vec![cmd(
        "never runs",
        &["touch", marker.to_str().unwrap()],
        false,
    )];
    for _ in 0..3000 {
        execute(&actions, false).unwrap();
    }
    assert!(!marker.exists());
}
