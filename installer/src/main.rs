// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 handyman0

//! t480-setup: bring up the Validity/Synaptics 06cb:009a reader on Debian.
//!
//! Commands that change the system print their plan and run only with `--apply`.
//! Writing to the sensor's flash (pairing, firmware upload) is not automated.

mod consts;
mod data;
mod doctor;
mod integrity;
mod sha;
mod steps;
#[cfg(test)]
mod stress;
mod sys;

use std::path::PathBuf;
use std::process::ExitCode;

const HELP: &str = "\
t480-setup: set up the Validity/Synaptics 06cb:009a fingerprint reader on Debian

USAGE: t480-setup <command> [options]

READ-ONLY
  doctor [--probe]                         Diagnose this machine; --probe also runs fprintd-list

USER-LEVEL (no root)
  data --python-validity DIR --libfprint-src DIR --out DIR
                                           Generate the driver data files (needs fetch-source first)
  firmware --out DIR [--apply]             Download, verify and extract the firmware extension
  fetch-source --dir DIR [--apply]         Get libfprint at the commit the patch targets
  build --libfprint-src DIR [--patch FILE] [--allow-other-base] [--apply]
                                           Apply the patch, compile, run libfprint's tests

SYSTEM (sudo; prints a plan unless --apply)
  install --libfprint-src DIR --data DIR --firmware FILE [--apply]
  lock-screen [--apply]                    Keep the reader active on the lock screen until a match
  uninstall [--libfprint-src DIR] [--apply]

Provisioning the sensor (pairing, firmware upload) writes to its flash and is NOT automated:
follow docs/t480-debian-notes.md deliberately, once.
";

struct Args {
    rest: Vec<String>,
}

impl Args {
    fn flag(&mut self, name: &str) -> bool {
        let before = self.rest.len();
        self.rest.retain(|a| a != name);
        self.rest.len() != before
    }

    fn opt(&mut self, name: &str) -> Result<Option<String>, String> {
        let Some(i) = self.rest.iter().position(|a| a == name) else {
            return Ok(None);
        };
        if i + 1 >= self.rest.len() {
            return Err(format!("{name} needs a value"));
        }
        let value = self.rest.remove(i + 1);
        self.rest.remove(i);
        Ok(Some(value))
    }

    fn required(&mut self, name: &str) -> Result<PathBuf, String> {
        self.opt(name)?
            .map(PathBuf::from)
            .ok_or_else(|| format!("{name} is required"))
    }

    fn done(&self) -> Result<(), String> {
        match self.rest.first() {
            None => Ok(()),
            Some(extra) => Err(format!("unexpected argument: {extra}")),
        }
    }
}

fn run(mut argv: Vec<String>) -> Result<(), String> {
    if argv.is_empty() {
        print!("{HELP}");
        return Ok(());
    }
    let command = argv.remove(0);
    let mut a = Args { rest: argv };
    match command.as_str() {
        "help" | "--help" | "-h" => {
            print!("{HELP}");
            Ok(())
        }
        "--version" | "version" => {
            println!("t480-setup {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "doctor" => {
            let probe = a.flag("--probe");
            a.done()?;
            doctor::print(&doctor::run(probe));
            Ok(())
        }
        "data" => {
            let reference = a.required("--python-validity")?;
            let libfprint = a.required("--libfprint-src")?;
            let out = a.required("--out")?;
            a.done()?;
            let key = integrity::read(&libfprint)?;
            let generated = data::build(&reference, consts::VID, consts::PID)?;
            for (path, len, digest) in
                data::write_all(&out, consts::VID, consts::PID, &generated, &key)?
            {
                println!("  {}  {len} bytes  sha256={digest}", path.display());
            }
            Ok(())
        }
        "firmware" => {
            let out = a.required("--out")?;
            let apply = a.flag("--apply");
            a.done()?;
            steps::firmware(&out, apply)
        }
        "fetch-source" => {
            let dir = a.required("--dir")?;
            let apply = a.flag("--apply");
            a.done()?;
            steps::fetch_source(&dir, apply)
        }
        "build" => {
            let src = a.required("--libfprint-src")?;
            let patch = a
                .opt("--patch")?
                .map(PathBuf::from)
                .unwrap_or_else(steps::default_patch);
            let other = a.flag("--allow-other-base");
            let apply = a.flag("--apply");
            a.done()?;
            steps::build(&src, &patch, other, apply)
        }
        "install" => {
            let src = a.required("--libfprint-src")?;
            let data = a.required("--data")?;
            let firmware = a.required("--firmware")?;
            let apply = a.flag("--apply");
            a.done()?;
            steps::install(&src, &data, &firmware, apply)
        }
        "lock-screen" => {
            let apply = a.flag("--apply");
            a.done()?;
            steps::lock_screen(apply)
        }
        "uninstall" => {
            let src = a.opt("--libfprint-src")?.map(PathBuf::from);
            let apply = a.flag("--apply");
            a.done()?;
            steps::uninstall(src.as_deref(), apply)
        }
        other => Err(format!("unknown command: {other} (try `t480-setup help`)")),
    }
}

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Args {
        Args {
            rest: items.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn parses_options_and_flags() {
        let mut a = args(&["--out", "x", "--apply", "--probe"]);
        assert_eq!(a.opt("--out").unwrap().as_deref(), Some("x"));
        assert!(a.flag("--apply"));
        assert!(!a.flag("--apply"));
        assert!(a.done().is_err());
        assert!(a.flag("--probe"));
        assert!(a.done().is_ok());
    }

    #[test]
    fn missing_values_and_unknown_arguments_are_errors() {
        assert!(args(&["--out"]).opt("--out").is_err());
        assert!(args(&[]).required("--out").is_err());
        assert!(run(vec!["nonsense".into()]).is_err());
        assert!(run(vec!["doctor".into(), "--bogus".into()]).is_err());
    }

    #[test]
    fn help_and_version_succeed() {
        assert!(run(vec![]).is_ok());
        assert!(run(vec!["help".into()]).is_ok());
        assert!(run(vec!["--version".into()]).is_ok());
    }
}
