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

use std::collections::HashMap;
use std::env::{self};
use std::ffi::c_int;
// use std::ffi::c_int;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use osal_rs::os::{Mutex, MutexFn, System, Thread, ThreadFn};
use osal_rs::utils::{Error, Result};

use crate::data::{Data, XDG_AUTOSTART};
use crate::desktop_entry::{DESKTOP_SUFFIX, DesktopEntry, current_desktops};
use xfce4_niri_lib::syslog::{Options, Priority, SysLog};

macro_rules! merge_autostart {
    ($locale:expr, $merge:expr, $autostart:expr) => {{
        for (_, path) in $autostart {
            let entry = DesktopEntry::read(&path)?;
            
            let name = entry.localized("Name", $locale);
            let name = match name {
                Some(name) => name,
                None => continue
            };

            $merge.remove(&name);
            $merge.insert(name, entry);
        }
    }};
}



pub(crate) struct AutostartData {
    xdg_autostart:  HashMap<String, String>,
    local_autostart: HashMap<String, String>,
    children: Vec<Child>
}

impl Default for AutostartData {
    fn default() -> Self {
        Self { 
            xdg_autostart: Default::default(),
            local_autostart: Default::default(), 
            children: Default::default() 
        }
    }
}


pub(crate) struct Autostart {
    thread: Thread,
    data: Arc<Mutex<AutostartData>>,
}

impl Autostart {

    const APP_TAG: &str = "Autostart";

    const REAP_INTERVAL: Duration = Duration::from_secs(30);

    pub(super) fn new() -> Self {
        Self {
            thread: Thread::new("autostart_thd", 0, 0),
            data: Mutex::new_arc(Default::default()),
        }
    }

    fn read_autostart(dir: &str) -> Result<HashMap<String, String>> {

        if !Path::new(dir).is_dir() {
            return Ok(HashMap::new())
        }

        Ok(Data::read_directory(dir)?
            .into_iter()
            .map(|it| (
                it
                .file_name()
                .to_string_lossy()
                .to_string(),

                it.path()
                .to_string_lossy()
                .to_string()
            )
        )
        .filter(|(file_name, _)| file_name.ends_with(DESKTOP_SUFFIX))
        .collect())
    }

    fn execute(entry: &DesktopEntry, desktops: &Vec<String>) -> Result<Child> {
    
        let argv = entry.exec_argv();

        if let Err(_) = entry.should_autostart(desktops) {
            return Err(Error::NotFound)
        }

        let Some((program, args)) = argv.split_first() else {
            return Err(Error::Unhandled("Exec key missing or empty"))
        };

        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| Error::UnhandledOwned(e.to_string()))
    }

    pub(super) fn start(&mut self) -> Result<()>{

        
        self.data.lock()?.xdg_autostart = Self::read_autostart(XDG_AUTOSTART)?;
        self.data.lock()?.local_autostart = Self::read_autostart(&Data::share().xdg_home_autostart)?;
    
        let data = self.data.clone();
        self.thread.spawn_simple(move || {

            let xdg_autostart = data.lock()?.xdg_autostart.clone();
            let local_autostart = data.lock()?.local_autostart.clone();
            

            let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);

            let locale = env::var("LANG").map_err(|e| Error::UnhandledOwned(e.to_string()))?;

            let mut merge = HashMap::<String, DesktopEntry>::new(); 

            merge_autostart!(&locale, &mut merge, &xdg_autostart);
            merge_autostart!(&locale, &mut merge, &local_autostart);

            let desktops = current_desktops();

            data.lock()?.children.clear();
            for (name, entry) in merge {                
                
                let child = Self::execute(&entry, &desktops);
                if let Err(_e  @ Error::NotFound) = child {
                    log.syslog_with_tag(Self::APP_TAG, Priority::LogDebug, &format!("Start: {name} - {:?} - skip", &entry.exec_argv()));
                    continue
                } if let Err(e) = child {
                    return Err(e)
                } 
                log.syslog_with_tag(Self::APP_TAG, Priority::LogDebug, &format!("Start: {name} - {:?} - ok", &entry.exec_argv()));
                data.lock()?.children.push(child.unwrap());
            }

            loop {
                System::delay_with_to_tick(Self::REAP_INTERVAL);

                let Ok(mut data) = data.lock() else {
                    continue
                };

                data.children.retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));
            }
        })?;

        Ok(())
    }
}