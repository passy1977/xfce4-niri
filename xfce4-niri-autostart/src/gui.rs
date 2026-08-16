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

use gtk::gio::Icon;
use gtk::gdk;
use gtk::{ApplicationWindow, Button, CellRendererCombo, CellRendererMode, CellRendererPixbuf, CellRendererText, CellRendererToggle, IconSize, Image, ListStore, Orientation, PolicyType, ScrolledWindow, SelectionMode, ShadowType, TreePath, TreeView, TreeViewColumn, Box};
use gtk::glib::{self, Propagation, StaticType};
use gtk::traits::{BoxExt, ButtonExt, CellRendererComboExt, CellRendererToggleExt, ContainerExt, GtkWindowExt, StyleContextExt, TreeSelectionExt, TreeViewColumnExt, TreeViewExt, WidgetExt};
use gtk::prelude::{GtkListStoreExtManual, GtkListStoreExt, TreeViewColumnExt as Column};

use osal_rs::os::{Mutex, MutexFn};
use xfce4_niri_lib::fxce::resource_match;
use xfce4_niri_lib::models::item::Item;


/// `xfce_rc_read_entry (rc, "Icon", "application-x-executable")`.
pub(crate) const DEFAULT_ICON: &str = "application-x-executable";

/// The desktop the C code filters `OnlyShowIn` / `NotShowIn` against.
pub(crate) const DESKTOP: &str = "XFCE";

mod col {
    pub const ICON: u32 = 0;
    pub const NAME: u32 = 1;
    pub const ENABLED: u32 = 2;
    pub const REMOVABLE: u32 = 3;
    pub const TOOLTIP: u32 = 4;
    pub const RUN_HOOK: u32 = 5;
    pub const RELPATH: u32 = 6;
}

pub(crate) struct Gui;

impl Gui {
    pub(crate) fn window_new(window: Arc<Mutex<ApplicationWindow>>, 
        on_item_toggled: Arc<Mutex<fn(&ListStore, &TreePath) -> ()>>,
        on_right_click: Arc<Mutex<fn(&TreeView, &gdk::EventButton) -> Propagation>>
    ) -> Box {

        let vbox = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(0)
            .border_width(12)
            .build();

        let swin = ScrolledWindow::builder()
            .shadow_type(ShadowType::In)
            .vscrollbar_policy(PolicyType::Automatic)
            .hscrollbar_policy(PolicyType::Automatic)
            .build();
        vbox.pack_start(&swin, true, true, 0);

        let model = Self::model_new();

        let tree_view = gtk::TreeView::builder()
            .model(&model)
            .headers_visible(true)
            .tooltip_column(col::TOOLTIP as i32)
            .build();
        swin.add(&tree_view);

        let on_right_click_cb = on_right_click.clone();
        tree_view.connect_button_press_event(*on_right_click_cb.lock().expect("Failed to lock on_right_click_cb mutex"));
        tree_view.connect_realize(|tree_view| tree_view.columns_autosize());

        let selection = tree_view.selection();
        selection.set_mode(SelectionMode::Single);

        let column = TreeViewColumn::builder().reorderable(false).resizable(false).build();
        let renderer = CellRendererToggle::new();
        
        let on_item_toggled_cb = on_item_toggled.clone();
        renderer.connect_toggled(glib::clone!(@weak model => move |_, path| {
            let callback = on_item_toggled_cb.lock().expect("Failed to lock on_item_toggled mutex");
            (*callback)(&model, &path);
        }));
        Column::pack_start(&column, &renderer, false);
        Column::add_attribute(&column, &renderer, "active", col::ENABLED as i32);
        column.set_sort_column_id(col::ENABLED as i32);
        tree_view.append_column(&column);

        // Column: icon and name of the program.
        let column = TreeViewColumn::builder()
            .title("Program")
            .reorderable(false)
            .resizable(false)
            .expand(true)
            .build();
        let renderer = CellRendererPixbuf::new();
        Column::pack_start(&column, &renderer, false);
        Column::add_attribute(&column, &renderer, "gicon", col::ICON as i32);
        let renderer = CellRendererText::builder().ellipsize(gtk::pango::EllipsizeMode::End).build();
        Column::pack_start(&column, &renderer, true);
        Column::add_attribute(&column, &renderer, "markup", col::NAME as i32);
        column.set_sort_column_id(col::NAME as i32);
        tree_view.append_column(&column);

        // Column: the trigger, editable through a combo inside the cell.
        let column = TreeViewColumn::builder()
            .title("Trigger")
            .reorderable(false)
            .resizable(false)
            .build();
        let renderer = CellRendererCombo::builder()
            .has_entry(false)
            //TODO: to handle
            //.model(&xfae_window_create_run_hooks_combo_model())
            .text_column(0)
            .editable(true)
            .mode(CellRendererMode::Editable)
            .build();
        renderer.connect_changed(glib::clone!(@weak model => move |_combo, _path, _combo_iter| {
            //TODO: to handle
            //run_hook_changed(&model, combo, &path, combo_iter);
        }));
        Column::pack_start(&column, &renderer, false);
        Column::add_attribute(&column, &renderer, "text", col::RUN_HOOK as i32);
        column.set_sort_column_id(col::RUN_HOOK as i32);
        tree_view.append_column(&column);

        // The inline toolbar.
        let bbox = Box::new(Orientation::Horizontal, 0);
        bbox.style_context().add_class("inline-toolbar");
        vbox.pack_start(&bbox, false, true, 0);

        let button = |label, icon, tooltip| {
            let button = Button::with_label(label);
            button.set_image(Some(&Image::from_icon_name(Some(icon), IconSize::Button)));
            button.set_tooltip_text(Some(tooltip));
            button
        };

        let add = button("Add", "list-add-symbolic", "Add application");
        //TODO: to handle
        // add.connect_clicked(glib::clone!(@weak tree_view => move |_| xfae_window_add(&tree_view)));
        bbox.pack_start(&add, false, false, 0);

        let remove = button("Remove", "list-remove-symbolic", "Remove application");
        //TODO: to handle
        // remove.connect_clicked(glib::clone!(@weak tree_view => move |_| xfae_window_remove(&tree_view)));
        bbox.pack_start(&remove, false, false, 0);

        let edit = button("Edit", "document-edit-symbolic", "Edit application");
        //TODO: to handle
        // edit.connect_clicked(glib::clone!(@weak tree_view => move |_| xfae_window_edit(&tree_view)));
        bbox.pack_start(&edit, false, false, 0);

        // Both buttons follow the selection, as in `xfae_window_init`.
        selection.connect_changed(glib::clone!(@weak remove, @weak edit => move |_selection| {
            //TODO: to handle
            // xfae_window_selection_changed(selection, &remove, &edit);
        }));
        //TODO: to handle
        // xfae_window_selection_changed(&selection, &remove, &edit);


        let v_close_box = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(0)
            .build();

        let window = window.clone();
        let close = button("Close", "window-close-symbolic", "Close application");
        close.connect_clicked(move |_| {
            window.lock().unwrap().close();
        });
        close.set_margin_top(12);
        v_close_box.pack_end(&close, false, false, 0);
        vbox.pack_start(&v_close_box, false, false, 0);
        

        vbox

    }

    fn model_new() -> ListStore {

        let model = ListStore::new(&[
            Icon::static_type(),        // ICON
            String::static_type(),      // NAME (markup)
            bool::static_type(),        // ENABLED
            bool::static_type(),        // REMOVABLE
            String::static_type(),      // TOOLTIP (markup)
            String::static_type(),      // RUN_HOOK (nick)
            String::static_type(),      // RELPATH
        ]);

        let mut items = resource_match("autostart/*.desktop", true)
            .iter()
            .filter_map(|rel_path| Item::new(rel_path, DESKTOP, DEFAULT_ICON))
            .collect::<Vec<_>>();

        items.sort_by(Item::sort_default);

        for item in &items {
            model.set(&model.append(), &[
                (col::ICON, &item.icon),
                (col::NAME, &item.markup()),
                (col::ENABLED, &item.is_enabled()),
                (col::REMOVABLE, &item.is_removable()),
                (col::TOOLTIP, &item.tooltip),
                (col::RUN_HOOK, &item.run_hook.nick()),
                (col::RELPATH, &item.rel_path),
            ]);
        }

        model
    }

    fn create_combo_model() -> gtk::ListStore {

        let model = gtk::ListStore::new(&[String::static_type()]);

        for hook in RunHook::ALL {
            model.set(&model.append(), &[(0, &hook.nick())]);
        }

        model
    }

}