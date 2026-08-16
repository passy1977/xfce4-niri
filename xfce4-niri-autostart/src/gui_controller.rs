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


use gtk::{ListStore, TreePath, TreeView};
use gtk::gdk;
use gtk::glib;

pub(crate) fn on_item_toggled(
    list_store: &ListStore, 
    tree_path: &TreePath
) {
    println!("on_item_toggled toggled: {:?} {:?}", list_store, tree_path);
}

pub(crate) fn on_right_click(
    tree_view: &TreeView,
    event: &gdk::EventButton,
) -> glib::Propagation {
    println!("on_right_click: {:?} {:?}", tree_view, event);

    glib::Propagation::Stop
}


