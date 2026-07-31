/***************************************************************************
 *
 * xfce-niri
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

use std::fs::DirEntry;
use std::{fs, path::Path};
use std::env;
use std::str::FromStr;
use std::ffi::c_int;

use osal_rs::utils::{Error, Result};

use crate::os::syslog::{Options, Priority, SysLog};


static mut DATA: Option<Data> = None;
pub(crate) const XDG_AUTOSTART: &str = "/etc/xdg/autostart";



#[derive(Default, Clone)]
pub(crate) struct Data {
    #[allow(dead_code)]
    user_home: String,
    pub(crate) niri_file: String,
    pub(crate) niri_d_folder: String,
    pub(crate) xdg_home_autostart: String,
    pub(crate) brightness_file: String
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

                        log.syslog(Priority::LogCrit, error);

                        panic!("{error}");
                    }
                };

                let mut data = Self { 
                    user_home: home.clone(), 
                    niri_file: home.clone(), 
                    niri_d_folder: home.clone(), 
                    xdg_home_autostart: home.clone(), 
                    brightness_file: home.clone() 
                };

                data.niri_file.push_str("/.config/niri/config.kdl");
                data.niri_d_folder.push_str("/.config/niri/niri.d");
                data.xdg_home_autostart.push_str("/.config/autostart/");
                data.brightness_file.push_str("/.local/state");
            
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
        let folders = [
            (self.niri_file.clone(), true, format!("Niri config file not found: {}", self.niri_file)),
            (self.niri_d_folder.clone(), true,format!("Niri config folder not found: {}", self.niri_d_folder)),
            (String::from_str(XDG_AUTOSTART).unwrap_or_default(), true, format!("XDG autostart folder not found: {XDG_AUTOSTART}")),
            (self.xdg_home_autostart.clone(), false, format!("XDG home autostart folder not found: {}", self.xdg_home_autostart)),
        ];

        let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);

        for (file_or_folder, mandatory, error) in folders {
            
            let file_or_folder = Path::new(&file_or_folder);


            if !file_or_folder.exists()  {
                if mandatory {
                    return Err(error);
                } else {
                    log.syslog_with_tag(Self::APP_TAG, Priority::LogInfo, &error);
                }
            }
        };

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

    

}