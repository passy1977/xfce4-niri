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
                let Ok(_) = Self::set_brightness(&brightness_path, data) else {
                    log.syslog(Self::APP_TAG, Priority::LogWarning, &format!("No found device: {}", &brightness_path.to_string_lossy()));
                    return Ok(Arc::new(()))
                };
                log.syslog(Self::APP_TAG, Priority::LogInfo, &format!("Found device: {}", &brightness_path.to_string_lossy()));
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

        let brightness_path = fs::read_to_string(&brightness_path)?;

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

        fs::write(brightness_path, value.value.to_string())?;
        Ok(())
    }

}


#[cfg(test)]
mod tests {

    use super::*;
    use xfce4_niri_lib::test_support::TempDir;
    use crate::data::Data;

    /// A `Data` whose brightness file lives under `dir`, without ever
    /// touching the process-wide `Data::share()` singleton.
    fn data_in(dir: &TempDir) -> Data {
        Data { brightness_file: dir.path().join("brightness.bin").to_string_lossy().to_string(), ..Default::default() }
    }

    #[test]
    fn write_then_read_brightness_round_trips_through_data() {

        let dir = TempDir::new();
        let data = data_in(&dir);

        data.write_brightness(BrightnessData { device: "eDP-1".to_string(), value: 42 }).unwrap();

        let read = data.read_brightness().unwrap();
        assert_eq!(read.device, "eDP-1");
        assert_eq!(read.value, 42);
    }

    /// No file yet is not an error: it reads back as the type's default.
    #[test]
    fn read_brightness_defaults_when_the_file_is_missing() {

        let dir = TempDir::new();
        let data = data_in(&dir);

        let read = data.read_brightness().unwrap();
        assert_eq!(read.value, BrightnessData::default().value);
    }

    fn brightness_data(device: &PathBuf, value: i32) -> Box<BrightnessData> {
        Box::new(BrightnessData { device: device.to_string_lossy().to_string(), value })
    }

    #[test]
    fn get_brightness_parses_the_trimmed_file_content() {

        let dir = TempDir::new();
        let path = dir.path().join("brightness");
        fs::write(&path, "  42\n").unwrap();

        assert_eq!(Brightness::get_brightness(&path).unwrap(), 42);
    }

    #[test]
    fn get_brightness_fails_on_non_numeric_content() {

        let dir = TempDir::new();
        let path = dir.path().join("brightness");
        fs::write(&path, "not a number").unwrap();

        assert!(Brightness::get_brightness(&path).is_err());
    }

    #[test]
    fn set_brightness_writes_the_value_when_the_device_matches() {

        let dir = TempDir::new();
        let path = dir.path().join("brightness");
        fs::write(&path, "0").unwrap();

        Brightness::set_brightness(&path, brightness_data(&path, 77)).unwrap();

        assert_eq!(Brightness::get_brightness(&path).unwrap(), 77);
    }

    /// The stored device may no longer be the one under `brightness_path`
    /// (unplugged, renumbered): writing to the wrong device is refused, not
    /// silently redirected.
    #[test]
    fn set_brightness_is_a_no_op_when_the_device_does_not_match() {

        let dir = TempDir::new();
        let path = dir.path().join("brightness");
        fs::write(&path, "0").unwrap();

        let other_device = dir.path().join("other-device");
        Brightness::set_brightness(&path, brightness_data(&other_device, 77)).unwrap();

        assert_eq!(Brightness::get_brightness(&path).unwrap(), 0);
    }
}