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

use std::env;
use std::ffi::c_int;
1
use crate::os::syslog::{Options, Priority, close_log, open_log, sys_log};


static mut DATA: Option<Data> = None;
const NIRI_CONFIG_FILE: &str = ".config/niri/config.kdl";
const NIRI_CONFIG_D_FOLDER: &str = ".config/niri.d";
const BRIGHTNESS_FILE: &str = "/.local/state/niri-brightness";


#[derive(Default, Clone)]
pub(crate) struct Data {
    user_home: String,
    niri_file: String,
    niri_d_folder: String,
    brightness_file: String
}


impl Data {
    pub(crate) const fn new() -> Self {
        Self { 
            user_home: String::new(),
            niri_file: String::new(),
            niri_d_folder: String::new(),
            brightness_file: String::new()
        }
    }

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

                        open_log(Options::LogPid as c_int | Options::LogNDelay as c_int);

                        sys_log(Priority::LogCrit, error);

                        close_log();

                        panic!("{error}");
                    }
                };

                let mut data = Self { 
                    user_home: home.clone(), 
                    niri_file: home.clone(), 
                    niri_d_folder: home.clone(), 
                    brightness_file: home.clone() 
                };

                data.niri_file.push_str(NIRI_CONFIG_FILE);
                data.niri_d_folder.push_str(NIRI_CONFIG_D_FOLDER);
                data.brightness_file.push_str(BRIGHTNESS_FILE);
            
                unsafe {
                    (*&raw mut DATA) = Some(data.clone())
                };

                data
            },
            Some(data) => data.clone(),
            
        }

    }

}