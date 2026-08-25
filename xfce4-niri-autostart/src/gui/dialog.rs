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


//
// --- XfaeDialog ------------------------------------------------------------
//
use gtk::glib;
use gtk::traits::{BoxExt, ButtonExt, ComboBoxTextExt, ContainerExt, DialogExt, EntryExt, FileChooserExt, GridExt, GtkWindowExt, WidgetExt};
use gtk::prelude::{ComboBoxExtManual, WidgetExtManual};
use xfce4_niri_lib::models::run_hook::RunHook;

/// Port of `XfaeDialog`: name, description, command and trigger of one entry.
pub(super) struct Dialog {
    dialog: gtk::Dialog,
    name_entry: gtk::Entry,
    descr_entry: gtk::Entry,
    command_entry: gtk::Entry,
    run_hook_combo: gtk::ComboBoxText,
}

impl Dialog {

    /// Port of `xfae_dialog_init` and `xfae_dialog_new`: passing any of the
    /// values turns the "Add application" dialog into "Edit application".
    pub(super) fn new(
        parent: Option<&gtk::Window>,
        name: Option<&str>,
        descr: Option<&str>,
        command: Option<&str>,
        run_hook: RunHook,
    ) -> Self {

        let dialog = gtk::Dialog::with_buttons(
            Some("Add application"),
            parent,
            gtk::DialogFlags::DESTROY_WITH_PARENT,
            &[("_Cancel", gtk::ResponseType::Cancel), ("_OK", gtk::ResponseType::Ok)],
        );

        dialog.set_default_response(gtk::ResponseType::Ok);
        dialog.set_response_sensitive(gtk::ResponseType::Ok, false);

        let content_area = dialog.content_area();
        content_area.set_border_width(6);

        let grid = gtk::Grid::builder()
            .row_spacing(6)
            .column_spacing(12)
            .border_width(6)
            .build();
        content_area.add(&grid);

        let entry = || gtk::Entry::builder().activates_default(true).hexpand(true).build();
        let label = |text| gtk::Label::builder().label(text).xalign(0.0).build();

        let name_entry = entry();
        grid.attach(&label("Name:"), 0, 0, 1, 1);
        grid.attach(&name_entry, 1, 0, 1, 1);

        let descr_entry = entry();
        grid.attach(&label("Description:"), 0, 1, 1, 1);
        grid.attach(&descr_entry, 1, 1, 1, 1);

        // The command entry carries a browse button next to it.
        let command_entry = entry();
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let browse = gtk::Button::builder().can_default(false).build();
        browse.add(&gtk::Image::from_icon_name(Some("document-open"), gtk::IconSize::Button));
        hbox.pack_start(&command_entry, true, true, 0);
        hbox.pack_start(&browse, false, false, 0);
        grid.attach(&label("Command:"), 0, 2, 1, 1);
        grid.attach(&hbox, 1, 2, 1, 1);

        let run_hook_combo = gtk::ComboBoxText::new();
        run_hook_combo.set_margin_bottom(5);
        for hook in RunHook::ALL {
            run_hook_combo.append_text(hook.nick());
        }
        #[cfg(not(feature = "enable-trigger"))]
        {
            run_hook_combo.set_active(Some(run_hook.value() as u32));
            grid.attach(&label("Trigger:"), 0, 3, 1, 1);
            grid.attach(&run_hook_combo, 1, 3, 1, 1);
        }


        browse.connect_clicked(
            glib::clone!(@weak dialog, @weak command_entry => move |_| 
                Self::browse(&dialog, &command_entry)
            )
        );

        // `xfae_dialog_update`: OK stays insensitive until both are filled in.
        name_entry.connect_text_notify(
            glib::clone!(@weak dialog, @weak command_entry => move |name_entry|                 
                Self::update(&dialog, name_entry, &command_entry)
            )
        );
        command_entry.connect_text_notify(
            glib::clone!(@weak dialog, @weak name_entry => move |command_entry| 
                Self::update(&dialog, &name_entry, command_entry)
            )
        );

        if let Some(name) = name {
            name_entry.set_text(name);
        }
        if let Some(descr) = descr {
            descr_entry.set_text(descr);
        }
        if let Some(command) = command {
            command_entry.set_text(command);
        }
        if name.is_some() || descr.is_some() || command.is_some() {
            dialog.set_title("Edit application");
        }

        dialog.show_all();

        Self { dialog, name_entry, descr_entry, command_entry, run_hook_combo }
    }

    /// Port of `xfae_dialog_update`.
    pub(super) fn update(dialog: &gtk::Dialog, name_entry: &gtk::Entry, command_entry: &gtk::Entry) {
        dialog.set_response_sensitive(
            gtk::ResponseType::Ok,
            !name_entry.text().is_empty() && !command_entry.text().is_empty(),
        );
    }

    /// Port of `xfae_dialog_browse`: picks the command off the file system.
    pub(super) fn browse(dialog: &gtk::Dialog, command_entry: &gtk::Entry) {

        let chooser = gtk::FileChooserDialog::with_buttons(
            Some("Select a command"),
            Some(dialog),
            gtk::FileChooserAction::Open,
            &[("Cancel", gtk::ResponseType::Cancel), ("OK", gtk::ResponseType::Accept)],
        );

        chooser.set_local_only(true);

        let command = command_entry.text();
        if command.starts_with('/') {
            chooser.set_filename(command.as_str());
        }

        if chooser.run() == gtk::ResponseType::Accept
            && let Some(filename) = chooser.filename() {
            command_entry.set_text(&filename.to_string_lossy());
        }

        unsafe { chooser.destroy() };
    }

    /// Port of `xfae_dialog_get`.
    pub(super) fn get(&self) -> (String, String, String, RunHook) {
        (
            self.name_entry.text().trim().to_string(),
            self.descr_entry.text().trim().to_string(),
            self.command_entry.text().trim().to_string(),
            RunHook::from_value(self.run_hook_combo.active().unwrap_or_default() as i32),
        )
    }

    pub(super) fn run(&self) -> gtk::ResponseType {
        self.dialog.run()
    }

    pub(super) fn hide(&self) {
        self.dialog.hide();
    }

    pub(super) fn destroy(&self) {
        unsafe { self.dialog.destroy() };
    }
}