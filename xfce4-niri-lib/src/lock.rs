/***************************************************************************
 *
 * xfce4-niri
 * Copyright (C) 2026 Antonio Salsi <passy.linux@zresa.it>
 *
 * This program is free software; you can redistribute it and/or
 * modify it under the terms of the GNU General Public License
 * as published by the Free Software Foundation; either version 2
 * of the License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program; if not, see <https://www.gnu.org/licenses/>.
 *
 ***************************************************************************/


use std::{ffi::c_int, io::Write};
use std::{fs, process};
use std::os::unix::prelude::AsRawFd;
use std::path::PathBuf;
use std::fs::{File, OpenOptions};
use std::io::{Error as IoError, Read};

use crate::Result;

pub struct Lock {
    path: PathBuf,
    /// The flock lives on this open descriptor: closing it releases the lock,
    /// so it must stay open as long as the `Lock` exists. `None` for
    /// `from_path`, which only wraps the path to check for the service's
    /// presence and never owns the lock.
    file: Option<File>,
}

mod ffi {
    use std::ffi::c_int;
    unsafe extern "C" { pub(super) fn flock(fd: c_int, operation: c_int) -> c_int; }
}

impl Lock {

    const LOCK_EX: c_int = 2;
    const LOCK_NB: c_int = 4;
    const LOCK_UN: c_int = 8;
    const EWOULDBLOCK: c_int = 11;
    
    pub const LOCK_FILE: &str = "xfce4-niri-service.lock";

    pub fn acquire(file_name: Option<&str>, locked_by: &mut Option<String>) -> Result<Self> {
        
        let lock_file = crate::get_safe_path(file_name)?;
        
        let mut file = OpenOptions::new().create(true).read(true).write(true).open(&lock_file)?;

        if unsafe { ffi::flock(file.as_raw_fd(), Self::LOCK_EX | Self::LOCK_NB) } != 0 {
            let mut locked_by_id = String::new();
            file.read_to_string(&mut locked_by_id)?;
            if let Some(locked_by) = locked_by {
                *locked_by = locked_by_id.clone();
            }
            return Err(format!("Another instance running, pid:{locked_by_id}").into());
        }

        file.set_len(0)?;
        file.write_fmt(format_args!("{pid}", pid = process::id()))?;

        Ok(
            Self {
                path: lock_file,
                file: Some(file),
            }
        )
    }

    pub fn is_locked(&self, locked_by: &mut Option<String>) -> Result<bool> {
        let lock_file = crate::get_safe_path(
            Some(self.path.to_str().unwrap_or(Self::LOCK_FILE))
        )?;

        let mut file = OpenOptions::new().read(true).write(true).open(&lock_file)?;

        if unsafe { ffi::flock(file.as_raw_fd(), Self::LOCK_EX | Self::LOCK_NB) } != 0 {
            let err = IoError::last_os_error();
            return match err.raw_os_error() {
                Some(Self::EWOULDBLOCK) => {
                    if let Some(locked_by) = locked_by {
                        file.read_to_string(locked_by)?;
                    }
                    Ok(true)
                }
                _ => Err(err.into()),
            };
        }
        
        let _ = unsafe { ffi::flock(file.as_raw_fd(), Self::LOCK_UN) };
        Ok(false)
    }

    pub fn from_path(path: &PathBuf) -> Self {
        Self {
            path: path.clone(),
            file: None,
        }
    }

}

impl Drop for Lock {
    fn drop(&mut self) {

        if self.file.is_none() {
            return;
        }

        if let Ok(lock_file) = crate::get_safe_path(
            Some(self.path.to_str().unwrap_or(Self::LOCK_FILE))
        ) {
            let _ = fs::remove_file(lock_file);
        }
    }
}


#[cfg(test)]
mod tests {

    use super::*;
    use crate::test_support::{EnvGuard, TempDir};

    /// Every test needs its own `XDG_RUNTIME_DIR` so `get_safe_path` never
    /// touches a real one, and the `TempDir` has to outlive the `EnvGuard`
    /// use, so both are returned together.
    fn runtime_dir() -> (EnvGuard, TempDir) {
        let dir = TempDir::new();
        let mut env = EnvGuard::new();
        env.set("XDG_RUNTIME_DIR", dir.path());
        (env, dir)
    }

    #[test]
    fn acquire_writes_the_current_pid_into_the_lock_file() {

        let (_env, _dir) = runtime_dir();

        let mut locked_by = None;
        let lock = Lock::acquire(Some("test.lock"), &mut locked_by).expect("cannot acquire the lock");

        assert!(lock.file.is_some());
        assert_eq!(fs::read_to_string(&lock.path).unwrap(), process::id().to_string());
        assert!(locked_by.is_none(), "acquiring is not being locked by someone else");
    }

    #[test]
    fn acquire_defaults_the_file_name_to_lock_file() {

        let (_env, _dir) = runtime_dir();

        let mut locked_by = None;
        let lock = Lock::acquire(None, &mut locked_by).unwrap();

        assert_eq!(lock.path.file_name().unwrap().to_str().unwrap(), Lock::LOCK_FILE);
    }

    /// A second instance must be refused, with the first one's pid reported.
    /// `locked_by` follows the same convention as the service's caller: it is
    /// only ever filled in, never turned from `None` into `Some` by `acquire`.
    #[test]
    fn acquire_refuses_a_second_instance_on_the_same_file() {

        let (_env, _dir) = runtime_dir();

        let mut owner = None;
        let _first = Lock::acquire(Some("test.lock"), &mut owner).unwrap();

        let mut locked_by = Some(String::new());
        let err = match Lock::acquire(Some("test.lock"), &mut locked_by) {
            Ok(_) => panic!("a second instance must not acquire the lock"),
            Err(e) => e,
        };

        assert!(err.to_string().contains("Another instance running"));
        assert_eq!(locked_by.as_deref(), Some(process::id().to_string().as_str()));
    }

    #[test]
    fn acquire_succeeds_again_once_the_previous_lock_is_dropped() {

        let (_env, _dir) = runtime_dir();

        let mut locked_by = None;
        let lock = Lock::acquire(Some("test.lock"), &mut locked_by).unwrap();
        drop(lock);

        let mut locked_by = None;
        assert!(Lock::acquire(Some("test.lock"), &mut locked_by).is_ok());
    }

    #[test]
    fn dropping_an_acquired_lock_removes_the_file() {

        let (_env, _dir) = runtime_dir();

        let mut locked_by = None;
        let lock = Lock::acquire(Some("test.lock"), &mut locked_by).unwrap();
        let path = lock.path.clone();
        assert!(path.exists());

        drop(lock);

        assert!(!path.exists());
    }

    #[test]
    fn from_path_does_not_own_the_lock_and_its_drop_is_a_no_op() {

        let (_env, _dir) = runtime_dir();

        let path = crate::get_safe_path(Some("test.lock")).unwrap();
        File::create(&path).unwrap();

        drop(Lock::from_path(&path));

        assert!(path.exists(), "from_path must not remove a file it never owned");
    }

    #[test]
    fn is_locked_is_false_when_the_file_exists_but_nobody_holds_it() {

        let (_env, _dir) = runtime_dir();

        let path = crate::get_safe_path(Some("test.lock")).unwrap();
        File::create(&path).unwrap();

        let mut locked_by = Some(String::new());
        assert_eq!(Lock::from_path(&path).is_locked(&mut locked_by).unwrap(), false);
        assert_eq!(locked_by.as_deref(), Some(""), "not locked, so nothing is filled in");
    }

    /// `is_locked` is how another process checks on the lock: it has to see
    /// the holder's pid without taking the lock itself.
    #[test]
    fn is_locked_is_true_while_another_lock_holds_it() {

        let (_env, _dir) = runtime_dir();

        let mut owner = None;
        let lock = Lock::acquire(Some("test.lock"), &mut owner).unwrap();

        let mut locked_by = Some(String::new());
        assert_eq!(Lock::from_path(&lock.path).is_locked(&mut locked_by).unwrap(), true);
        assert_eq!(locked_by.as_deref(), Some(process::id().to_string().as_str()));
    }
}