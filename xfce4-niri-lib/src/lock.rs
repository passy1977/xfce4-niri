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

use std::{ffi::c_int, io::Write};
use std::{fs, process};
use std::os::unix::prelude::AsRawFd;
use std::path::PathBuf;
use std::fs::{File, OpenOptions};
use std::io::{Error as IoError, Read};

use osal_rs::utils::{Error, Result};

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
        
        let mut file = OpenOptions::new().create(true).read(true).write(true).open(&lock_file)
            .map_err(|e| Error::UnhandledOwned(e.to_string()))?;

        if unsafe { ffi::flock(file.as_raw_fd(), Self::LOCK_EX | Self::LOCK_NB) } != 0 {
            let mut locked_by_id = String::new();
            file.read_to_string(&mut  locked_by_id).map_err(|e| Error::UnhandledOwned(e.to_string()))?;
            if let Some(locked_by) = locked_by {
                *locked_by = locked_by_id.clone();
            }
            return Err(Error::UnhandledOwned(format!("Another instance running, pid:{locked_by_id}")));
        }

        file.set_len(0).map_err(|e| Error::UnhandledOwned(e.to_string()))?;
        file.write_fmt(format_args!("{pid}", pid = process::id())).map_err(|e| Error::UnhandledOwned(e.to_string()))?;

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

        let mut file = OpenOptions::new().read(true).write(true).open(&lock_file)
            .map_err(|e| Error::UnhandledOwned(e.to_string()))?;

        if unsafe { ffi::flock(file.as_raw_fd(), Self::LOCK_EX | Self::LOCK_NB) } != 0 {
            let err = IoError::last_os_error();
            return match err.raw_os_error() {
                Some(Self::EWOULDBLOCK) => {
                    if let Some(locked_by) = locked_by {
                        file.read_to_string(locked_by).map_err(|e| Error::UnhandledOwned(e.to_string()))?;
                    }
                    Ok(true)
                }
                _ => Err(Error::UnhandledOwned(err.to_string())),
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