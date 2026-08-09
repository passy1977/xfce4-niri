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

use std::ffi::c_int;
use std::fs;
use std::num::ParseIntError;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use osal_rs::os::{Mutex, MutexFn, System, Thread, ThreadFn, ThreadParam};
use osal_rs::utils::{Error, Result};
use osal_rs_serde::{Deserialize, Serialize};

use crate::data::Data;
use xfce4_niri_lib::syslog::{Options, Priority, SysLog};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct BrightnessData {
    device: String,
    value: i32
}

impl Default for BrightnessData {
    fn default() -> Self {
        Self { device: Default::default(), value: -2 }
    }
}

#[derive(Clone)]
pub(crate) struct Brightness {
    thread: Thread,
    current_brightness: Arc<Mutex<i32>>,
}


impl Brightness {

    const APP_TAG: &str = "Brightness";

    pub(super) fn new() -> Self {
        Self {
            thread: Thread::new("brightness_thd", 0, 0),
            current_brightness: Mutex::new_arc(-1),
        }
    }

    pub(super) fn start(&mut self) -> Result<()>{

        let param: Option<ThreadParam> = Some(self.current_brightness.clone());

        self.thread.spawn(param, |_, param| {

            let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);


            let devices = Data::read_directory("/sys/class/backlight")?;
            let Some(device) = devices.iter().next() else {
                return Err(Error::Unhandled("Brightness device not found"))
            };

            let brightness_path = device.path().join("brightness");


            let current_brightness = param
                .and_then(|p| p.downcast::<Mutex<i32>>().ok())
                .ok_or(Error::Unhandled("Missing current_brightness parameter"))?;


            
            let mut binding = current_brightness.lock();
            let Ok(current_brightness_ref) = binding.as_deref_mut() else {
                return Err(Error::Unhandled("Missing current_brightness parameter"))
            };

            let data: Box<BrightnessData> = Data::share().read_brightness()?;
            if data.value > 0 {
                *current_brightness_ref = data.value;
                let Err(_) = Self::set_brightness(&brightness_path, data) else {
                    log.syslog_with_tag(Self::APP_TAG, Priority::LogWarning, &format!("No found device: {}", &brightness_path.to_string_lossy()));
                    return Ok(Arc::new(()))
                };
                log.syslog_with_tag(Self::APP_TAG, Priority::LogInfo, &format!("Found device: {}", &brightness_path.to_string_lossy()));
            } else {
                *current_brightness_ref = 0;
                drop(data)
            }
            
            loop {

                let value = Self::get_brightness(&brightness_path)?;

                if *current_brightness_ref != value {
                    *current_brightness_ref =  value;
                    Data::share().write_brightness(BrightnessData { 
                        device: brightness_path.to_string_lossy().to_string(), 
                        value
                    })?;
                }
                
                System::delay_with_to_tick(Duration::from_secs(1));
            }

        })?;

        Ok(())
    }

    fn get_brightness(brightness_path: &PathBuf) -> Result<i32> {

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

    fn set_brightness(brightness_path: &PathBuf, value: Box<BrightnessData>) -> Result<()> {
        if brightness_path.to_string_lossy() != value.device {
            return Ok(())
        }

        fs::write(brightness_path, value.value.to_le_bytes()).map_err(|e| Error::UnhandledOwned(e.to_string()))?;
        Ok(())
    }

}