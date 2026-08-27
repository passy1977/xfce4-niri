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

use std::fs::{DirEntry, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::{fs, path::Path};
use std::env;
use std::str::FromStr;
use std::ffi::c_int;

use osal_rs::utils::{Error, Result};
use osal_rs_serde::{Deserialize, Serialize};

use crate::brightness::BrightnessData;
use crate::data::ffi::getuid;
use xfce4_niri_lib::syslog::{Options, Priority, SysLog};

mod ffi {
    use std::ffi::{c_int, c_uint};
    unsafe extern "C" { pub(super) fn flock(fd: c_int, operation: c_int) -> c_int; }
    unsafe extern "C" { pub(super) fn getuid() -> c_uint; }
}

static mut DATA: Option<Data> = None;
pub(crate) const XDG_AUTOSTART: &str = "/etc/xdg/autostart";

macro_rules! get_env_full_path {
    ($home:expr, $env_var:literal, $real_path:literal) => {{
        let mut home_tmp = $home.clone();
        let var = env::var($env_var).unwrap_or_else(move |_| {
            home_tmp.push_str($real_path);
            home_tmp.to_string()
        });

        format!("{}/", var)
    }};
}


#[derive(Default, Clone)]
pub(crate) struct Data {
    #[allow(dead_code)]
    pub(crate) niri_file: String,
    pub(crate) xdg_home_autostart: String,
    pub(crate) brightness_file: String,
    pub(crate) lock_screen_file: String,
}


impl Data {

    const APP_TAG: &str = "Data";
    const IO_BUFFER_SIZE: usize = 256;

    const LOCK_EX: c_int = 2;
    const LOCK_NB: c_int = 4;

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

                        log.syslog(Priority::LogCrit, error);

                        panic!("{error}");
                    }
                };
                
                let mut local = home.clone();
                local.push_str("/.local/");

                let config = get_env_full_path!(home, "XDG_CONFIG_HOME", "/.config");
                let state = get_env_full_path!(home, "XDG_STATE_HOME", "/.local/state");

                let mut niri_file = config.clone();
                niri_file.push_str("niri/config.kdl");

                let mut xdg_home_autostart = home.clone();
                xdg_home_autostart.push_str("/.config/autostart");

                let mut brightness_file = state;
                brightness_file.push_str("xfce4_niri_brightness");

                let mut lock_screen_file = config.clone();
                lock_screen_file.push_str("niri/bin/lock_screen");

                let data = Self { 
                    niri_file, 
                    xdg_home_autostart, 
                    brightness_file,
                    lock_screen_file
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
            (self.niri_file.clone(), true, format!("Niri config file not found: {}", self.niri_file)),
            (String::from_str(XDG_AUTOSTART).unwrap_or_default(), true, format!("XDG autostart folder not found: {XDG_AUTOSTART}")),
            (self.xdg_home_autostart.clone(), false, format!("XDG home autostart folder not found: {}", self.xdg_home_autostart)),
            (self.lock_screen_file.clone(), false, format!("Lock screen file not found: {}", self.lock_screen_file)),
        ];

        let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);

        for (file_or_folder, mandatory, error) in elements {
            
            let file_or_folder = Path::new(&file_or_folder);


            if !file_or_folder.exists()  {
                if mandatory {
                    return Err(error);
                } else {
                    log.syslog_with_tag(Self::APP_TAG, Priority::LogInfo, &error);
                }
            }
        };

        if !fs::metadata(self.lock_screen_file.clone())
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false) {
                return Err(Error::UnhandledOwned(format!("Lock screen file is not executable: {}", self.lock_screen_file)).to_string())
            }

        Ok(())
    }

    pub(crate) fn read_directory(dir: &str) -> Result<Vec<DirEntry>> {

        let mut ret = Vec::<DirEntry>::new();
        let entries = fs::read_dir(dir).map_err(|e| Error::UnhandledOwned(e.to_string()))?;

        for entry in entries {
            let entry = entry.map_err(|e| Error::UnhandledOwned(e.to_string()))?;
            ret.push(entry);
        }

        Ok(ret)
    }

    fn write_file(file: &String, value: &impl Serialize) -> Result<()> {
        let full_path = Path::new(file);

        let Some(parent) = full_path.parent() else {
            let msg = format!("Strange! The file no has parent: {file}", file = full_path.display());
            return Err(Error::UnhandledOwned(msg))
        };

        if !parent.exists() {
            let msg = format!("Folder not exist: {}", parent.display());
            return Err(Error::UnhandledOwned(msg))
        }

        if full_path.is_dir() {
            let msg = format!("This is a folder: {}", full_path.display());
            return Err(Error::UnhandledOwned(msg))
        }

        let mut buffer= [0u8; Self::IO_BUFFER_SIZE];
        let len_conversion = osal_rs_serde::to_bytes(value, &mut buffer).map_err(|e| Error::UnhandledOwned(e.to_string()))?;
        if len_conversion == 0 {
            return Err(Error::WriteError("Invalid binary conversion"))
        }
        
        let mut file = File::create(file).map_err(|e| Error::UnhandledOwned(e.to_string()))?;
        let len_written = file.write(&buffer[0 .. len_conversion]).map_err(|e| Error::UnhandledOwned(e.to_string()))?;

        if len_conversion != len_written {
            return Err(Error::WriteError("Invalid binary conversion"))
        }

        Ok(())
    }


    fn read_file<T>(file: &String) -> Result<Box<T>>
        where T: Deserialize + Default
    {
        let full_path = Path::new(file);

        let Some(parent) = full_path.parent() else {
            let msg = format!("Strange! The file no has parent: {file}", file = full_path.display());
            return Err(Error::UnhandledOwned(msg))
        };

        if !parent.exists() {
            let msg = format!("Folder not exist: {}", parent.display());
            return Err(Error::UnhandledOwned(msg))
        }

        if full_path.is_dir() {
            let msg = format!("This is a folder: {}", full_path.display());
            return Err(Error::UnhandledOwned(msg))
        }

        if !full_path.exists() {
            return Ok(Box::new(T::default()))
        }
        
        let mut file = File::open(file).map_err(|e| Error::UnhandledOwned(e.to_string()))?;
        let mut buffer= [0u8; Self::IO_BUFFER_SIZE];

        file.read(&mut buffer).map_err(|e| Error::UnhandledOwned(e.to_string()))?;

        let value: T = osal_rs_serde::from_bytes(&mut buffer).map_err(|_| Error::ReadError("Impossible read file"))?;

        Ok(Box::new(value))
    }

    #[inline]
    pub(crate) fn write_brightness(&self, value: BrightnessData) -> Result<()> {
        Self::write_file(&self.brightness_file, &value)
    }

    #[inline]
    pub(crate) fn read_brightness(&self) -> Result<Box<BrightnessData>>  {
        Self::read_file::<BrightnessData>(&self.brightness_file)
    }
    
    pub(crate) fn acquire_single_instance_lock() -> Result<File> {
        

        let path = Path::new(
                &env::var("XDG_RUNTIME_DIR")
                .unwrap_or_else(|_| format!("/tmp"))
            )
            .join(format!("xfce4-niri-{}", unsafe { getuid() }));

        if !path.exists() {
            fs::create_dir(&path).map_err( |e| Error::UnhandledOwned(e.to_string()))?;
        }

        let lock_file = path.join("xfce4-niri-service.lock");
        
        let file = OpenOptions::new().create(true).read(true).write(true).open(&lock_file)
            .map_err(|e| Error::UnhandledOwned(e.to_string()))?;

        if unsafe { ffi::flock(file.as_raw_fd(), Self::LOCK_EX | Self::LOCK_NB) } != 0 {
            return Err(Error::UnhandledOwned("another instance is already running".into()));
        }

        Ok(file)
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