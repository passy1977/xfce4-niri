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

use std::fs;
use std::num::ParseIntError;
use std::path::PathBuf;
use std::time::Duration;

use osal_rs::os::{System, Thread, ThreadFn};
use osal_rs::utils::{Error, Result};

use crate::data::Data;

#[allow(unused)]
pub(crate) struct Brightness {
    thread: Thread
}

impl Brightness {


    pub(super) fn new() -> Self {
        Self {
            thread: Thread::new("brightness_thd", 0, 0),
        }
    }

    pub(super) fn start(&mut self) -> Result<()>{

        self.thread.spawn(None, |_, _| {

            let devices = Data::read_directory("/sys/class/backlight")?;
            let Some(device) = devices.iter().next() else {
                return Err(Error::Unhandled("Brightness device not found"))
            };

            let brightness_path = device.path().join("brightness");


            loop {

                let actual_brightness_value = Self::read_brightness(&brightness_path)?;

                println!("actual_brightness_value:{actual_brightness_value}");

                System::delay_with_to_tick(Duration::from_secs(1));
            }

        })?;

        Ok(())
    }

    fn read_brightness(brightness_path: &PathBuf) -> Result<i32> {

        let brightness_path = fs::read_to_string(&brightness_path).map_err(|e| Error::UnhandledOwned(e.to_string()))?;
        
        Ok(
            brightness_path
            .trim()
            .parse()
            .map_err(
                |e: ParseIntError| Error::UnhandledOwned(e.to_string()) 
            )?
        )
    }


    fn write_brightness(brightness_path: &PathBuf, value: i32) -> Result<()> {

        let brightness_path = fs::read_to_string(&brightness_path).map_err(|e| Error::UnhandledOwned(e.to_string()))?;
        
        Ok(())
    }

}