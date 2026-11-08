// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 handyman0

//! System helpers: plans that print by default and run only with `--apply`.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub type Res<T> = Result<T, String>;

pub fn shell_quote(arg: &str) -> String {
    if !arg.is_empty()
        && arg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./=:,+@%".contains(&b))
    {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

pub fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc_geteuid() == 0 }
}

unsafe extern "C" {
    #[link_name = "geteuid"]
    fn libc_geteuid() -> u32;
}

pub fn which(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        let p = PathBuf::from(program);
        return is_executable(&p).then_some(p);
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(program))
        .find(|p| is_executable(p))
}

fn is_executable(p: &Path) -> bool {
    fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Run a command and capture it. `None` when it could not be started.
pub fn capture(program: &str, args: &[&str]) -> Option<(i32, String, String)> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    Some((
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    ))
}

pub enum Action {
    /// Run a command; with `root` it is prefixed with `sudo` unless already root.
    Cmd {
        why: String,
        argv: Vec<String>,
        root: bool,
        cwd: Option<PathBuf>,
    },
    /// Install a file with the given mode (root-owned destination).
    WriteRoot {
        why: String,
        dest: PathBuf,
        mode: u32,
        content: Vec<u8>,
    },
}

pub fn cmd(why: &str, argv: &[&str], root: bool) -> Action {
    Action::Cmd {
        why: why.into(),
        argv: argv.iter().map(|s| s.to_string()).collect(),
        root,
        cwd: None,
    }
}

pub fn write_root(why: &str, dest: &Path, mode: u32, content: Vec<u8>) -> Action {
    Action::WriteRoot {
        why: why.into(),
        dest: dest.into(),
        mode,
        content,
    }
}

fn with_sudo(argv: &[String], root: bool) -> Vec<String> {
    if root && !is_root() {
        let mut v = vec!["sudo".to_string()];
        v.extend_from_slice(argv);
        v
    } else {
        argv.to_vec()
    }
}

fn private_temp_dir() -> Res<PathBuf> {
    let mut buf = [0u8; 8];
    if let Ok(mut f) = fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut buf);
    }
    let name: String = buf.iter().map(|b| format!("{b:02x}")).collect();
    let dir = std::env::temp_dir().join(format!("t480-setup-{name}"));
    fs::create_dir(&dir).map_err(|e| e.to_string())?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Print every action; execute them only when `apply` is set. Stops at the first failure.
pub fn execute(actions: &[Action], apply: bool) -> Res<()> {
    if !apply {
        println!("Dry run: nothing was changed. Review the plan, then repeat with --apply.\n");
    }
    for (i, action) in actions.iter().enumerate() {
        let n = i + 1;
        match action {
            Action::Cmd {
                why,
                argv,
                root,
                cwd,
            } => {
                let full = with_sudo(argv, *root);
                let line = full
                    .iter()
                    .map(|a| shell_quote(a))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("[{n}/{}] {why}\n      $ {line}", actions.len());
                if apply {
                    let mut c = Command::new(&full[0]);
                    c.args(&full[1..]);
                    if let Some(dir) = cwd {
                        c.current_dir(dir);
                    }
                    let status = c
                        .status()
                        .map_err(|e| format!("cannot run {}: {e}", full[0]))?;
                    if !status.success() {
                        return Err(format!("step {n} failed ({status}): {line}"));
                    }
                }
            }
            Action::WriteRoot {
                why,
                dest,
                mode,
                content,
            } => {
                println!(
                    "[{n}/{}] {why}\n      write {} (mode {mode:04o}, {} bytes)",
                    actions.len(),
                    dest.display(),
                    content.len()
                );
                if content.len() <= 600 && std::str::from_utf8(content).is_ok() {
                    for l in String::from_utf8_lossy(content).lines() {
                        println!("        | {l}");
                    }
                }
                if apply {
                    let dir = private_temp_dir()?;
                    let staged = dir.join("staged");
                    let mut f = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .mode(0o600)
                        .open(&staged)
                        .map_err(|e| e.to_string())?;
                    f.write_all(content).map_err(|e| e.to_string())?;
                    drop(f);
                    let argv = vec![
                        "install".to_string(),
                        "-D".into(),
                        "-m".into(),
                        format!("{mode:04o}"),
                        staged.to_string_lossy().into_owned(),
                        dest.to_string_lossy().into_owned(),
                    ];
                    let full = with_sudo(&argv, true);
                    let status = Command::new(&full[0])
                        .args(&full[1..])
                        .status()
                        .map_err(|e| e.to_string())?;
                    let _ = fs::remove_dir_all(&dir);
                    if !status.success() {
                        return Err(format!(
                            "step {n} failed ({status}): install {}",
                            dest.display()
                        ));
                    }
                }
            }
        }
    }
    if apply {
        println!("\nAll {} steps completed.", actions.len());
    }
    Ok(())
}

// ------------------------------------------------------------------ facts

pub struct UsbDevice {
    pub id: String,
    pub path: String,
    pub product: String,
}

/// Passive USB scan through sysfs (no device is opened).
pub fn usb_devices() -> Vec<UsbDevice> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/bus/usb/devices") else {
        return found;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let (Ok(v), Ok(d)) = (
            fs::read_to_string(p.join("idVendor")),
            fs::read_to_string(p.join("idProduct")),
        ) else {
            continue;
        };
        found.push(UsbDevice {
            id: format!("{}:{}", v.trim(), d.trim()),
            path: entry.file_name().to_string_lossy().into_owned(),
            product: fs::read_to_string(p.join("product"))
                .unwrap_or_default()
                .trim()
                .to_string(),
        });
    }
    found
}

pub fn os_pretty_name() -> String {
    fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|t| {
            t.lines().find_map(|l| {
                l.strip_prefix("PRETTY_NAME=")
                    .map(|v| v.trim_matches('"').to_string())
            })
        })
        .unwrap_or_else(|| "unknown".into())
}

pub fn kernel_release() -> String {
    fs::read_to_string("/proc/sys/kernel/osrelease")
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_is_shell_safe() {
        assert_eq!(shell_quote("plain-1.2/x"), "plain-1.2/x");
        assert_eq!(shell_quote("has space"), "'has space'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("$(rm -rf /)"), "'$(rm -rf /)'");
    }

    #[test]
    fn dry_run_runs_nothing() {
        let marker = std::env::temp_dir().join(format!("vs-dry-{}", std::process::id()));
        let actions = vec![cmd(
            "would create a file",
            &["touch", marker.to_str().unwrap()],
            false,
        )];
        execute(&actions, false).unwrap();
        assert!(!marker.exists());
        execute(&actions, true).unwrap();
        assert!(marker.exists());
        fs::remove_file(marker).unwrap();
    }

    #[test]
    fn a_failing_step_stops_the_plan() {
        let marker = std::env::temp_dir().join(format!("vs-stop-{}", std::process::id()));
        let actions = vec![
            cmd("fails", &["false"], false),
            cmd("must not run", &["touch", marker.to_str().unwrap()], false),
        ];
        assert!(execute(&actions, true).is_err());
        assert!(!marker.exists());
    }

    #[test]
    fn which_finds_sh() {
        assert!(which("sh").is_some());
        assert!(which("definitely-not-a-program-xyz").is_none());
    }
}
