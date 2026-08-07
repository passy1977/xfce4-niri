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

use std::sync::Arc;

use osal_rs::os::{Mutex, Thread, ThreadFn, ThreadParam};
use osal_rs::utils::Result;

use crate::brightness::BrightnessData;


pub(crate) struct AutostartData {
    device: String,
    value: i32
}

impl Default for AutostartData {
    fn default() -> Self {
        Self { device: Default::default(), value: -2 }
    }
}


pub(crate) struct Autostart {
    thread: Thread,
    data: Arc<Mutex<BrightnessData>>,
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

        let param: Option<ThreadParam> = Some(self.data.clone());

        self.thread.spawn(param, |_, param| {

    

            Ok(Arc::new(()))
        })?;

        Ok(())
    }
}