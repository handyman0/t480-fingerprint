// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 handyman0

//! Passive diagnosis. Reads sysfs, package state and config files; changes nothing and
//! never opens the sensor (unless `--probe` asks `fprintd-list` to).

use crate::consts::*;
use crate::data::{COMMON_FILES, DEVICE_FILES};
use crate::sys::{capture, kernel_release, os_pretty_name, usb_devices, which};
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Level {
    Ok,
    Missing,
    Info,
}

pub struct Line {
    pub level: Level,
    pub text: String,
}

fn line(level: Level, text: impl Into<String>) -> Line {
    Line {
        level,
        text: text.into(),
    }
}

/// Build-time requirements of the libfprint this project patches (checked against its meson.build).
const MESON_MIN: &str = "0.62.0";
const LIBRARIES: [(&str, &str); 8] = [
    ("glib-2.0", "2.68"),
    ("gio-unix-2.0", "2.68"),
    ("gusb", "0.2.0"),
    ("udev", ""),
    ("openssl", "3.0"),
    ("gudev-1.0", ""),
    ("pixman-1", ""),
    ("nss", ""),
];

/// The apt package that provides a pkg-config module. `udev.pc` is the odd one: it comes from
/// the `udev` package up to Debian 12 and from `systemd-dev` on Debian 13.
pub fn install_hint(module: &str) -> String {
    match module {
        "udev" => "apt: install `udev` (Debian 12 and older) or `systemd-dev` (Debian 13)".into(),
        "glib-2.0" | "gio-unix-2.0" => "apt: install libglib2.0-dev".into(),
        "gusb" => "apt: install libgusb-dev".into(),
        "openssl" => "apt: install libssl-dev".into(),
        "gudev-1.0" => "apt: install libgudev-1.0-dev".into(),
        "pixman-1" => "apt: install libpixman-1-dev".into(),
        "nss" => "apt: install libnss3-dev".into(),
        other => format!("apt: install the -dev package that provides {other}.pc"),
    }
}

/// True when `found` (e.g. `3.0.17`, `1.1.1w`, `2.84.4`) is at least `min`. Each dot-separated
/// component is compared by its leading digits; missing components count as 0.
pub fn version_at_least(found: &str, min: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.trim()
            .split('.')
            .map(|p| {
                p.chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0)
            })
            .collect()
    };
    let (a, b) = (parts(found), parts(min));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x > y;
        }
    }
    true
}

fn version_line(what: &str, found: &str, min: &str) -> Line {
    if version_at_least(found, min) {
        line(Level::Ok, format!("{what} {found} (needs >= {min})"))
    } else {
        line(
            Level::Missing,
            format!(
                "{what} {found} is too old: this driver needs >= {min}, so this release cannot build it"
            ),
        )
    }
}

pub fn package_version(name: &str) -> Option<String> {
    let (code, out, _) = capture("dpkg-query", &["-W", "-f=${Version}", name])?;
    (code == 0 && !out.is_empty()).then_some(out)
}

/// The `pam_fprintd.so` options on the first matching `auth` line, if the line exists.
pub fn pam_fprintd_args(config: &str) -> Option<String> {
    config.lines().map(str::trim).find_map(|l| {
        if l.starts_with('#') {
            return None;
        }
        let mut parts = l.split_whitespace();
        (parts.next()? == "auth").then_some(())?;
        let control = parts.next()?;
        // Bracketed controls such as `[success=2 default=ignore]` span several tokens.
        if control.starts_with('[') && !control.ends_with(']') {
            for token in parts.by_ref() {
                if token.ends_with(']') {
                    break;
                }
            }
        }
        (parts.next()? == "pam_fprintd.so").then(|| {
            parts
                .take_while(|t| !t.starts_with('#'))
                .collect::<Vec<_>>()
                .join(" ")
        })
    })
}

pub fn run(probe: bool) -> Vec<Line> {
    let mut out = Vec::new();
    let arch = multiarch();

    out.push(line(
        Level::Info,
        format!("System: {} (kernel {})", os_pretty_name(), kernel_release()),
    ));
    match usb_devices().into_iter().find(|d| d.id == USB_ID) {
        Some(d) => {
            let name = if d.product.is_empty() {
                String::new()
            } else {
                format!(" ({})", d.product)
            };
            out.push(line(
                Level::Ok,
                format!("Sensor {USB_ID} present on USB port {}{name}", d.path),
            ))
        }
        None => out.push(line(
            Level::Missing,
            format!("Sensor {USB_ID} not found on USB: this tool does not apply to your reader"),
        )),
    }

    for pkg in ["fprintd", "libfprint-2-2", "libpam-fprintd"] {
        match package_version(pkg) {
            Some(v) => out.push(line(Level::Ok, format!("package {pkg} {v}"))),
            None => out.push(line(Level::Missing, format!("package {pkg} not installed"))),
        }
    }

    for tool in [
        "git",
        "meson",
        "ninja",
        "cc",
        "pkg-config",
        "curl",
        "innoextract",
        "sha512sum",
        "sudo",
    ] {
        match which(tool) {
            Some(p) => out.push(line(Level::Ok, format!("tool {tool} ({})", p.display()))),
            None => out.push(line(Level::Missing, format!("tool {tool} not found"))),
        }
    }
    // Minimum versions the patched libfprint (and its validity driver) needs. The driver
    // uses the OpenSSL 3 API, so releases that only ship OpenSSL 1.1 cannot build it.
    if let Some((0, text, _)) = capture("meson", &["--version"]) {
        out.push(version_line("tool meson", &text, MESON_MIN));
    }
    if which("pkg-config").is_some() {
        for (module, min) in LIBRARIES {
            let found = capture("pkg-config", &["--modversion", module])
                .filter(|(c, _, _)| *c == 0)
                .map(|(_, v, _)| v);
            out.push(match found {
                None => line(
                    Level::Missing,
                    format!(
                        "build library {module} not found ({})",
                        install_hint(module)
                    ),
                ),
                Some(v) if min.is_empty() => line(Level::Ok, format!("build library {module} {v}")),
                Some(v) => version_line(&format!("build library {module}"), &v, min),
            });
        }
    }

    let lib = format!("/usr/local/lib/{arch}/libfprint-2.so.2");
    out.push(if Path::new(&lib).exists() {
        line(Level::Ok, format!("patched libfprint installed ({lib})"))
    } else {
        line(
            Level::Missing,
            format!("patched libfprint not installed ({lib})"),
        )
    });

    let missing: Vec<String> = COMMON_FILES
        .iter()
        .map(|f| format!("{SYSTEM_DATA_DIR}/{f}"))
        .chain(
            DEVICE_FILES
                .iter()
                .map(|f| format!("{SYSTEM_DATA_DIR}/{:04x}_{:04x}/{f}", VID, PID)),
        )
        .filter(|p| !Path::new(p).is_file())
        .collect();
    out.push(if missing.is_empty() {
        line(
            Level::Ok,
            format!(
                "all {} data files present in {SYSTEM_DATA_DIR}",
                COMMON_FILES.len() + DEVICE_FILES.len()
            ),
        )
    } else {
        line(
            Level::Missing,
            format!(
                "{} data files missing under {SYSTEM_DATA_DIR}",
                missing.len()
            ),
        )
    });

    let fwext = format!("{SYSTEM_DATA_DIR}/{FWEXT_NAME}");
    out.push(if Path::new(&fwext).is_file() {
        line(
            Level::Ok,
            format!("firmware extension file present ({fwext})"),
        )
    } else {
        line(
            Level::Missing,
            format!("firmware extension file missing ({fwext})"),
        )
    });

    out.push(match fs::read_to_string(DROPIN_FILE) {
        Ok(t) if t.contains("LIBFPRINT_VALIDITY_USE_FALLBACK_IDENTITY=1") => line(
            Level::Ok,
            "fprintd uses the fallback host identity (drop-in present)",
        ),
        _ => line(
            Level::Missing,
            "fprintd drop-in for the host identity missing",
        ),
    });

    match fs::read_to_string(PAM_GDM_FINGERPRINT)
        .ok()
        .and_then(|c| pam_fprintd_args(&c))
    {
        Some(args) if args.contains("max-tries=") => out.push(line(
            Level::Ok,
            format!("lock screen PAM: pam_fprintd {args}"),
        )),
        Some(_) => out.push(line(
            Level::Info,
            "lock screen PAM accepts only the default number of attempts (run lock-screen)",
        )),
        None => out.push(line(
            Level::Info,
            "no fingerprint PAM service for the GNOME lock screen found",
        )),
    }
    if let Ok(common) = fs::read_to_string(PAM_COMMON_AUTH)
        && let Some(args) = pam_fprintd_args(&common)
    {
        out.push(line(
            Level::Info,
            format!("common-auth still asks pam_fprintd ({args}); it competes with the lock screen for the reader"),
        ));
    }

    if probe {
        match capture(
            "fprintd-list",
            &[&std::env::var("USER").unwrap_or_default()],
        ) {
            Some((_, text, err)) => {
                let shown = if text.is_empty() { err } else { text };
                out.push(line(
                    Level::Info,
                    format!("fprintd-list: {}", shown.replace('\n', " | ")),
                ));
            }
            None => out.push(line(Level::Missing, "fprintd-list not available")),
        }
    } else {
        out.push(line(
            Level::Info,
            "the sensor was not opened (add --probe to run fprintd-list)",
        ));
    }
    out
}

pub fn print(lines: &[Line]) {
    for l in lines {
        let mark = match l.level {
            Level::Ok => " ok ",
            Level::Missing => "MISS",
            Level::Info => "info",
        };
        println!("[{mark}] {}", l.text);
    }
    let missing = lines.iter().filter(|l| l.level == Level::Missing).count();
    println!("\n{missing} item(s) missing. Nothing was changed.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pam_fprintd_options() {
        let cfg = "#%PAM-1.0\nauth\trequisite\tpam_nologin.so\nauth required pam_fprintd.so max-tries=3 timeout=30\n";
        assert_eq!(
            pam_fprintd_args(cfg).as_deref(),
            Some("max-tries=3 timeout=30")
        );
        assert_eq!(
            pam_fprintd_args("auth required pam_fprintd.so").as_deref(),
            Some("")
        );
        assert_eq!(
            pam_fprintd_args("auth [success=2 default=ignore] pam_unix.so"),
            None
        );
        assert_eq!(pam_fprintd_args("# auth required pam_fprintd.so"), None);
        // Debian's common-auth line: bracketed control and a trailing comment.
        assert_eq!(
            pam_fprintd_args(
                "auth\t[success=2 default=ignore]\tpam_fprintd.so max-tries=1 timeout=10 # debug"
            )
            .as_deref(),
            Some("max-tries=1 timeout=10")
        );
        assert_eq!(pam_fprintd_args("password required pam_fprintd.so"), None);
    }

    #[test]
    fn version_comparison_handles_real_world_strings() {
        assert!(version_at_least("3.5.7", "3.0"));
        assert!(version_at_least("3.0", "3.0"));
        assert!(version_at_least("2.84.4", "2.68"));
        assert!(version_at_least("0.62.0", "0.62.0"));
        assert!(version_at_least("1.7.0", "0.62.0"));
        // Debian 11 ships OpenSSL 1.1.1 with a letter suffix: too old for the OpenSSL 3 API.
        assert!(!version_at_least("1.1.1w", "3.0"));
        assert!(!version_at_least("2.66.8", "2.68"));
        assert!(!version_at_least("0.56.2", "0.62.0"));
        assert!(!version_at_least("2.67.9", "2.68"));
        assert!(
            version_at_least("10.0", "9.9"),
            "components are numbers, not text"
        );
        assert!(version_at_least("", "") && !version_at_least("", "1"));
    }

    #[test]
    fn install_hints_name_the_right_package() {
        assert!(install_hint("udev").contains("udev"));
        assert!(install_hint("udev").contains("systemd-dev"));
        assert!(install_hint("openssl").contains("libssl-dev"));
        assert!(install_hint("something-else").contains("something-else.pc"));
    }

    #[test]
    fn too_old_libraries_are_reported_as_missing() {
        assert_eq!(
            version_line("openssl", "1.1.1w", "3.0").level,
            Level::Missing
        );
        assert_eq!(version_line("openssl", "3.0.17", "3.0").level, Level::Ok);
    }

    #[test]
    fn doctor_never_fails_and_always_reports_the_system() {
        let lines = run(false);
        assert!(lines.iter().any(|l| l.text.starts_with("System:")));
        assert!(lines.iter().any(|l| l.text.contains("was not opened")));
    }
}
