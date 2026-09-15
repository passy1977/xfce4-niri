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

use std::fs::{DirEntry, File};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::{fs, path::Path};
use std::env;
use std::str::FromStr;
use std::ffi::c_int;

use osal_rs::utils::{Error, Result};
use osal_rs_serde::{Deserialize, Serialize};

use crate::brightness::BrightnessData;
use xfce4_niri_lib::syslog::{Options, Priority, SysLog};


static mut DATA: Option<Data> = None;
pub(crate) const XDG_AUTOSTART: &str = "/etc/xdg/autostart";
pub(crate) const LOCK_SCREEN_LOCAL_FILE: &str = "/usr/local/libexec/lock_screen";
pub(crate) const LOCK_SCREEN_GLOBAL_FILE: &str = "/usr/libexec/lock_screen";

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
    // pub(crate) niri_file: String,
    pub(crate) xdg_home_autostart: String,
    pub(crate) brightness_file: String,
    pub(crate) lock_screen_file: String,
}


impl Data {

    const APP_TAG: &str = "Data";

    const IO_BUFFER_SIZE: usize = 256;

    pub(crate) fn share() -> Self {

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

                let config = get_env_full_path!(home, "XDG_CONFIG_HOME", "/.config");
                let state = get_env_full_path!(home, "XDG_STATE_HOME", "/.local/state");

                // let mut niri_file = config.clone();
                // niri_file.push_str("niri/config.kdl");

                let mut xdg_home_autostart = home.clone();
                xdg_home_autostart.push_str("/.config/autostart");

                let mut brightness_file = state;
                brightness_file.push_str("xfce4_niri_brightness");

                let mut lock_screen_file = config.clone();
                lock_screen_file.push_str("niri/bin/lock_screen");

                let data = Self { 
                    // niri_file, 
                    xdg_home_autostart, 
                    brightness_file,
                    lock_screen_file
                };

                unsafe {
                    (*&raw mut DATA) = Some(data)
                };

                unsafe {
                    (*&raw const DATA).clone().unwrap()
                }
                
            },
            Some(data) => data.clone(),
            
        }

    }

    pub(crate) fn check_persistence(&mut self) -> Result<()> {
        let elements = [
            (String::from_str(XDG_AUTOSTART).unwrap_or_default(), true, format!("XDG autostart folder not found: {XDG_AUTOSTART}")),
            (self.xdg_home_autostart.clone(), false, format!("XDG home autostart folder not found: {}", self.xdg_home_autostart)),
        ];

        let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);

        for (file_or_folder, mandatory, error) in elements {
            
            let file_or_folder = Path::new(&file_or_folder);


            if !file_or_folder.exists()  {
                if mandatory {
                    return Err(Error::UnhandledOwned(error));
                } else {
                    log.syslog(Self::APP_TAG, Priority::LogInfo, &error);
                }
            }
        };


        let lock_screen_file;
        if Path::new(&self.lock_screen_file).exists() {
            lock_screen_file = self.lock_screen_file.clone();
        } else if Path::new(LOCK_SCREEN_LOCAL_FILE).exists() {
            lock_screen_file = LOCK_SCREEN_LOCAL_FILE.to_owned();
        } else if Path::new(LOCK_SCREEN_GLOBAL_FILE).exists() {
            lock_screen_file = LOCK_SCREEN_GLOBAL_FILE.to_owned();
        } else {
            return Err(Error::UnhandledOwned(format!("Local lock screen file not found: {}", LOCK_SCREEN_LOCAL_FILE)));
        }
        

        if !fs::metadata(&lock_screen_file)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false) {
                return Err(Error::UnhandledOwned(format!("Lock screen file is not executable: {}", self.lock_screen_file)))
            }

        self.lock_screen_file = lock_screen_file;

        unsafe {
            (*&raw mut DATA) = Some(self.clone());
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

}


#[cfg(test)]
mod tests {

    use super::*;
    use xfce4_niri_lib::test_support::TempDir;

    /// A `Data` whose files live under `dir`, without ever touching the
    /// process-wide `Data::share()` singleton.
    fn data_in(dir: &TempDir) -> Data {
        Data { brightness_file: dir.path().join("brightness").to_string_lossy().to_string(), ..Default::default() }
    }

    #[test]
    fn read_directory_lists_every_entry() {

        let dir = TempDir::new();
        dir.file("a", 0o644);
        dir.file("b", 0o644);

        let mut names: Vec<String> = Data::read_directory(dir.path().to_str().unwrap())
            .unwrap()
            .into_iter()
            .map(|it| it.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();

        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn read_directory_fails_for_a_missing_directory() {

        let dir = TempDir::new();
        assert!(Data::read_directory(dir.path().join("nope").to_str().unwrap()).is_err());
    }

    /// No file yet is not an error: it reads back as the type's default.
    #[test]
    fn read_brightness_does_not_fail_when_the_file_is_missing() {

        let dir = TempDir::new();
        let data = data_in(&dir);

        assert!(data.read_brightness().is_ok());
    }

    #[test]
    fn write_brightness_fails_when_the_parent_folder_is_missing() {

        let dir = TempDir::new();
        let mut data = data_in(&dir);
        data.brightness_file = dir.path().join("missing-dir").join("brightness").to_string_lossy().to_string();

        assert!(data.write_brightness(BrightnessData::default()).is_err());
    }

    #[test]
    fn write_brightness_fails_when_the_target_is_a_directory() {

        let dir = TempDir::new();
        let mut data = data_in(&dir);
        data.brightness_file = dir.dir("brightness").to_string_lossy().to_string();

        assert!(data.write_brightness(BrightnessData::default()).is_err());
    }
}
