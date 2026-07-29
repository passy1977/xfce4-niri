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

use std::{env, ffi::c_int};

use osal_rs::os::Mutex;

use crate::os::syslog::{Options, close_log, open_log};

static mut DATA: Option<Data> = None;

#[derive(Default, Clone)]
pub(crate) struct Data {
    niri_folder: String,
    brightness_file: String
}


impl Data {
    pub(crate) const fn new() -> Self {
        Self { 
            niri_folder: String::new(),
            brightness_file: String::new()
        }
    }

    pub(crate) fn share() -> &'static Self {

        let data = unsafe {
            &mut *&raw mut DATA   
        };


        
        match data {
            None => {
                unsafe {
                    (*&raw mut DATA) = Some(Data::new())
                };

                let data = unsafe {
                    (*&raw mut DATA).as_ref().unwrap()
                };

                let home = match env::var("HOME") {
                    Ok(home) => home,
                    Err(_) => {


                        open_log(Options::LogPid as c_int | Options::LogNDelay as c_int);



                        close_log();

                        panic!("");
                    }
                };

                data
            },
            Some(data) => data,
            
        }
    }

}