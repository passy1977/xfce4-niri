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

extern crate osal_rs;
extern crate xfce4_niri_lib;

mod data;
mod gui_controller;
mod gui;
mod models;
mod xfce;

use std::error::Error;
use std::ffi::c_int;

use gtk::Application;
use gtk::gio::prelude::ApplicationExtManual;
use gtk::gio::traits::ApplicationExt;
use gtk::traits::{WidgetExt, ContainerExt};
use osal_rs::os::{Mutex, MutexFn};
use xfce4_niri_lib::niri_check::NiriCheck;
use xfce4_niri_lib::syslog::{Options, Priority, SysLog};

use crate::data::Data;
use crate::gui::Gui;
use crate::gui_controller::{on_item_toggled, on_right_click};

const APP_ID: &str = "it.salsi.xfce-niri.AutoStart";

fn main() -> Result<(), Box<dyn Error>> {

    let log = SysLog::open(Options::LogPid as c_int | Options::LogNDelay as c_int);


    let version = match NiriCheck::version() {
        Some(version) => version,
        None => {
            let msg = "Niri not ruining";
            log.syslog(Priority::LogCrit, &msg);
            return Err(msg.into())
        }
    };
    log.syslog(Priority::LogDebug, &format!("Version {}", env!("CARGO_PKG_VERSION")));
    log.syslog(Priority::LogDebug, &format!("Niri {} running", version));

    if let Err(e) = Data::share().check_persistence() {
        let msg = e.to_string();
        log.syslog(Priority::LogCrit, &msg);
        return Err(msg.into());
    }

    let app = Application::builder()
        .application_id(APP_ID)
        .build();


    app.connect_activate(&build_ui);

    app.run_with_args::<&str>(&[]);

    Ok(())
}

fn build_ui(app: &gtk::Application) {
    let window = Mutex::new_arc(
        gtk::ApplicationWindow::builder()
        .application(app)
        .title("Xfce4-niri Autostart")
        .default_width(600)
        .default_height(450)
        .build()
    );

    let window_clone = window.clone();    
    let window = window.lock().expect("Failed to lock window mutex");
    window.add(&Gui::window_new(
        window_clone.clone(), 
        Mutex::new_arc(on_item_toggled),
    Mutex::new_arc(on_right_click)
    ));
    window.show_all();
}
