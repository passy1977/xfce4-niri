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

use std::path::Path;
use std::{env, str::FromStr};
use std::ffi::c_int;
use std::fs;

use crate::data::ffi::getuid;
use xfce4_niri_lib::syslog::{Options, Priority, SysLog};

mod ffi {
    use std::ffi::c_uint;
    unsafe extern "C" { pub(super) fn getuid() -> c_uint; }
}

static mut DATA: Option<Data> = None;
pub(crate) const XDG_AUTOSTART: &str = "/etc/xdg/autostart";

#[derive(Default, Clone)]
pub(crate) struct Data {
    #[allow(dead_code)]
    pub(crate) xdg_home_autostart: String,
}


impl Data {

    const APP_TAG: &str = "Data";


    pub(crate) fn share() -> &'static Self {

        let data = unsafe {
            &mut *&raw mut DATA   
        };   

        match data {
            None => {
            
                let home = match env::var("HOME") {
                    Ok(home) => home,
                    Err(_) => {

                        let error = "No HOME environment variable is set.";

                        let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);

                        log.syslog(Self::APP_TAG, Priority::LogCrit, error);

                        panic!("{error}");
                    }
                };
                
                let mut local = home.clone();
                local.push_str("/.local/");

                let mut xdg_home_autostart = home.clone();
                xdg_home_autostart.push_str("/.config/autostart");

                let data = Self { 
                    xdg_home_autostart, 
                };

                unsafe {
                    (*&raw mut DATA) = Some(data)
                };

                unsafe {
                    (*&raw const DATA).as_ref().unwrap()
                }
                
            },
            Some(data) => data,
            
        }

    }

pub(crate) fn check_persistence(&self) -> Result<(), String> {
        let elements = [
            (String::from_str(XDG_AUTOSTART).unwrap_or_default(), true, format!("XDG autostart folder not found: {XDG_AUTOSTART}")),
            (self.xdg_home_autostart.clone(), false, format!("XDG home autostart folder not found: {}", self.xdg_home_autostart)),
        ];

        let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);

        for (file_or_folder, mandatory, error) in elements {
            
            let file_or_folder = Path::new(&file_or_folder);


            if !file_or_folder.exists()  {
                if mandatory {
                    return Err(error);
                } else {
                    log.syslog(Self::APP_TAG, Priority::LogInfo, &error);
                }
            }
        };

        Ok(())
    }

}

impl Drop for Data {
    fn drop(&mut self) {

        let dir = env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| format!("/tmp/xfce4-niri-{}", unsafe { getuid() }));
        let path = format!("{dir}/xfce4-niri-service.lock");

        let _ = fs::remove_file(path);
    }
}


#[cfg(test)]
mod tests {

    use super::*;
    use crate::test_support::{EnvGuard, TempDir};

    #[test]
    fn dropping_data_removes_the_service_lock_file() {

        let dir = TempDir::new();
        let mut env = EnvGuard::new();
        env.set("XDG_RUNTIME_DIR", dir.path());

        let lock_path = dir.path().join("xfce4-niri-service.lock");
        fs::write(&lock_path, "123").unwrap();

        drop(Data { xdg_home_autostart: String::new() });

        assert!(!lock_path.exists());
    }

    /// Nothing to remove is not an error: the GUI can start with the service
    /// not running at all.
    #[test]
    fn dropping_data_is_a_no_op_when_there_is_no_lock_file() {

        let dir = TempDir::new();
        let mut env = EnvGuard::new();
        env.set("XDG_RUNTIME_DIR", dir.path());

        let lock_path = dir.path().join("xfce4-niri-service.lock");
        drop(Data { xdg_home_autostart: String::new() });

        assert!(!lock_path.exists(), "still nothing to remove, and no panic either");
    }

    /// The mandatory, system wide autostart folder is a hard coded real path:
    /// this test only makes a claim about the optional, per-user one, and is
    /// skipped outright if that assumption does not hold on this machine.
    #[test]
    fn check_persistence_does_not_fail_when_only_the_local_autostart_dir_is_missing() {

        if !Path::new(XDG_AUTOSTART).exists() {
            return
        }

        let dir = TempDir::new();
        let data = Data { xdg_home_autostart: dir.path().join("missing").to_string_lossy().to_string() };

        assert!(data.check_persistence().is_ok());
    }

    #[test]
    fn check_persistence_ok_when_both_autostart_dirs_exist() {

        if !Path::new(XDG_AUTOSTART).exists() {
            return
        }

        let dir = TempDir::new();
        let data = Data { xdg_home_autostart: dir.path().to_string_lossy().to_string() };

        assert!(data.check_persistence().is_ok());
    }
}