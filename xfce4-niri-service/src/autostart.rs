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
use std::fs::DirEntry;
use std::ptr::null_mut;
use std::sync::Arc;

use osal_rs::os::{Mutex, MutexFn, Thread, ThreadFn, ThreadParam};
use osal_rs::utils::{DoublePtr, Ptr, Result};

use crate::data::{Data, XDG_AUTOSTART};

pub(crate) struct AutostartData {
    xdg_autostart:  HashMap<String, DirEntry>,
    local_autostart: HashMap<String, DirEntry>
}

impl Default for AutostartData {
    fn default() -> Self {
        Self { xdg_autostart: Default::default(), local_autostart: Default::default() }
    }
}


pub(crate) struct Autostart {
    thread: Thread,
    data: Arc<Mutex<AutostartData>>,
}

impl Autostart {

    const APP_TAG: &str = "Autostart";

    pub(super) fn new() -> Self {
        Self {
            thread: Thread::new("autostart_thd", 0, 0),
            data: Mutex::new_arc(Default::default()),
        }
    }


    pub(super) fn start(&mut self) -> Result<()>{

        let mut data = self.data.lock()?;

        data.xdg_autostart = Data::read_directory(XDG_AUTOSTART)?.iter().map(|it| (it.file_name().to_string_lossy().to_string(), it.clone())).collect();
        data.local_autostart = Data::read_directory(&data.xdg_autostart.values().next().unwrap().path().parent().unwrap().to_string_lossy())?.iter().map(|it| (it.file_name().to_string_lossy().to_string(), it.clone())).collect();

        let param: Option<ThreadParam> = Some(self.data.clone());

        let thread_local = self.thread.spawn(param, |_, param| {


    

            Ok(Arc::new(()))
        })?;

        let mut ret: Ptr = null_mut();
        thread_local.join(&raw mut ret)?;

        Ok(())
    }
}