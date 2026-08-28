/***************************************************************************
 *
 * xfce4-niri
 * Copyright (C) 2026 Antonio Salsi <passy.linux@zresa.it>
 *
 * This library is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Lesser General Public
 * License as published by the Free Software Foundation; either
 * version 2.1 of the License, or (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
 * Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public
 * License along with this library; if not, see <https://www.gnu.org/licenses/>.
 *
 ***************************************************************************/

pub mod models;
pub mod lock;
pub mod niri;
pub mod socket;
pub mod syslog;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::{env, fs};

fn is_program(path: impl AsRef<Path>) -> bool {

    let Ok(metadata) = fs::metadata(path) else {
        return false
    };

    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
}


pub fn binary_exists(binary: &str) -> bool {
    const DEFAULT_PATH: &str = "/bin:/usr/bin:.";
    
    let Ok(argv) = glib::shell_parse_argv(binary) else {
        return true // Unparsable: the C code leaves the entry alone.
    };

    let Some(program) = argv.first() else {
        return true // Parsed to no words at all: same.
    };

    // A `/` anywhere makes it a path. `to_string_lossy` cannot lose one on the
    // way: `/` never appears inside a multi byte sequence.
    if program.to_string_lossy().contains('/') {
        return is_program(program)
    }

    let path = env::var_os("PATH")
        .filter(|it| !it.is_empty())
        .unwrap_or_else(|| DEFAULT_PATH.into());

    // Bytes, not `str`: a `PATH` element is not required to be UTF-8.
    path.as_bytes()
        .split(|it| *it == b':')
        .map(|dir| if dir.is_empty() { Path::new(".") } else { Path::new(OsStr::from_bytes(dir)) })
        .any(|dir| is_program(dir.join(program)))
}


pub fn current_uid() -> u32 {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::test_support::{EnvGuard, TempDir};

    const EXECUTABLE: u32 = 0o755;
    const READABLE: u32 = 0o644;

    #[test]
    fn is_program_wants_an_executable_file() {

        let dir = TempDir::new();

        assert!(is_program(dir.file("runnable", EXECUTABLE)));
        assert!(!is_program(dir.file("plain", READABLE)));
        assert!(!is_program(dir.path()), "a directory is not a program");
        assert!(!is_program(dir.path().join("missing")));
    }

    /// Any of the three `x` bits is enough, the same test the C code runs.
    #[test]
    fn is_program_accepts_any_execute_bit() {

        let dir = TempDir::new();

        assert!(is_program(dir.file("owner", 0o100)));
        assert!(is_program(dir.file("group", 0o010)));
        assert!(is_program(dir.file("other", 0o001)));
    }

    #[test]
    fn binary_exists_follows_a_path_without_looking_at_path_var() {

        let dir = TempDir::new();
        let runnable = dir.file("runnable", EXECUTABLE);
        let plain = dir.file("plain", READABLE);

        let mut env = EnvGuard::new();
        env.set("PATH", dir.path());

        assert!(binary_exists(runnable.to_str().unwrap()));
        assert!(!binary_exists(plain.to_str().unwrap()));
        assert!(!binary_exists(dir.path().join("missing").to_str().unwrap()));

        // A relative name holding a `/` is a path too, so `PATH` is not searched.
        assert!(!binary_exists("./runnable"));
    }

    #[test]
    fn binary_exists_searches_every_path_element() {

        let dir = TempDir::new();
        let first = dir.dir("first");
        let second = dir.dir("second");

        dir.file("second/runnable", EXECUTABLE);

        let mut env = EnvGuard::new();

        env.set("PATH", format!("{}:{}", first.display(), second.display()));
        assert!(binary_exists("runnable"));

        env.set("PATH", first.display().to_string());
        assert!(!binary_exists("runnable"));
    }

    /// Only the first word is the program: the arguments are not looked up.
    #[test]
    fn binary_exists_looks_at_the_parsed_program_only() {

        let dir = TempDir::new();
        dir.file("runnable", EXECUTABLE);
        dir.file("with space", EXECUTABLE);

        let mut env = EnvGuard::new();
        env.set("PATH", dir.path());

        assert!(binary_exists("runnable --flag /missing/argument"));
        assert!(binary_exists("\"with space\" --flag"));
        assert!(!binary_exists("missing runnable"));
    }

    /// An unparsable or empty command line is left alone, like the C code does.
    #[test]
    fn binary_exists_keeps_what_it_cannot_parse() {

        let dir = TempDir::new();

        let mut env = EnvGuard::new();
        env.set("PATH", dir.path());

        assert!(binary_exists("\"unterminated"), "unbalanced quote");
        assert!(binary_exists("'"), "unbalanced quote");
        assert!(binary_exists(""), "no words at all");
        assert!(binary_exists("   "), "no words at all");
    }

    /// An empty `PATH` element means the working directory.
    #[test]
    fn binary_exists_reads_an_empty_path_element_as_the_working_dir() {

        let dir = TempDir::new();
        dir.file("runnable", EXECUTABLE);

        let mut env = EnvGuard::new();
        env.set("PATH", ":/nowhere").chdir(dir.path());

        assert!(binary_exists("runnable"));
    }

    /// An unset or empty `PATH` falls back to `/bin:/usr/bin:.`, which is where
    /// the shell lives on any POSIX system.
    #[test]
    fn binary_exists_falls_back_to_a_default_path() {

        let mut env = EnvGuard::new();

        env.unset("PATH");
        assert!(binary_exists("sh"));

        env.set("PATH", "");
        assert!(binary_exists("sh"));

        assert!(!binary_exists("xfce4-niri-no-such-binary"));
    }
}

