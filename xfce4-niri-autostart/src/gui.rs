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

mod dialog;

use std::rc::Rc;
use std::sync::Arc;

use gtk::gio::{Icon, ThemedIcon};
use gtk::{ApplicationWindow, Button, CellRendererCombo, CellRendererMode, CellRendererPixbuf, CellRendererText, CellRendererToggle, IconSize, Image, ListStore, Orientation, PolicyType, ScrolledWindow, SelectionMode, ShadowType, TreeView, TreeViewColumn, Box};
use gtk::glib::{self, Cast, IsA, Propagation, StaticType, ToValue};
use gtk::traits::{BoxExt, ButtonExt, CellRendererComboExt, CellRendererToggleExt, ContainerExt, GtkMenuExt, GtkMenuItemExt, GtkWindowExt, MenuShellExt, StyleContextExt, TreeModelExt, TreeSelectionExt, TreeViewColumnExt, TreeViewExt, WidgetExt};
use gtk::prelude::{GtkListStoreExt, GtkListStoreExtManual, TreeSortableExtManual, TreeViewColumnExt as Column};

use osal_rs::os::{Mutex, MutexFn};
use crate::xfce::{self, resource_match};
use crate::models::item::Item;
use crate::models::run_hook::RunHook;

use crate::gui::dialog::Dialog;


/// `xfce_rc_read_entry (rc, "Icon", "application-x-executable")`.
pub(crate) const DEFAULT_ICON: &str = "application-x-executable";

/// The desktop the C code filters `OnlyShowIn` / `NotShowIn` against.
pub(crate) const DESKTOP: &str = "XFCE";

mod col {
    pub(crate) const ICON: u32 = 0;
    pub(crate) const NAME: u32 = 1;
    pub(crate) const ENABLED: u32 = 2;
    pub(crate) const REMOVABLE: u32 = 3;
    pub(crate) const TOOLTIP: u32 = 4;
    pub(crate) const RUN_HOOK: u32 = 5;
    pub(crate) const RELPATH: u32 = 6;
}

pub(crate) struct Gui {
    tree_view: TreeView
}

impl Gui {

    fn cell_string(model: &impl IsA<gtk::TreeModel>, iter: &gtk::TreeIter, column: u32) -> String {
        model.value(iter, column as i32).get::<String>().unwrap_or_default()
    }

    fn cell_bool(model: &impl IsA<gtk::TreeModel>, iter: &gtk::TreeIter, column: u32) -> bool {
        model.value(iter, column as i32).get::<bool>().unwrap_or_default()
    }

    /// `gtk_widget_get_toplevel`, as far as it is a window: the parent the dialogs
    /// and the error dialog are transient for.
    fn toplevel(widget: &impl IsA<gtk::Widget>) -> Option<gtk::Window> {
        widget.toplevel().and_then(|it| it.downcast::<gtk::Window>().ok())
    }

    pub(crate) fn window_new(window: Arc<Mutex<ApplicationWindow>>) -> (Rc<Self>, Box) {

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

        let model = Self::tree_view_model_new();

        let tree_view = gtk::TreeView::builder()
            .model(&model)
            .headers_visible(true)
            .tooltip_column(col::TOOLTIP as i32)
            .build();
        swin.add(&tree_view);

        // Every handler that needs the `Gui` gets it through this handle: the
        // closures GTK owns hold a weak reference, so the tree view keeping them
        // alive does not keep `Gui` alive in turn.
        let this = Rc::new(Self {
            tree_view: tree_view.clone()
        });

        tree_view.connect_button_press_event(glib::clone!(
            @weak this => @default-return Propagation::Proceed,
            move |tree_view, event| this.on_mouse_right_click(tree_view, event)
        ));
        tree_view.connect_realize(|tree_view| tree_view.columns_autosize());

        let selection = tree_view.selection();
        selection.set_mode(SelectionMode::Single);

        let column = TreeViewColumn::builder().reorderable(false).resizable(false).build();
        let renderer = CellRendererToggle::new();
        // let on_item_toggled_cb = on_item_toggled.clone();
        // renderer.connect_toggled(glib::clone!(@weak model => move |_, path| {
        //     let callback = on_item_toggled_cb.lock().expect("Failed to lock on_item_toggled mutex");
        //     (*callback)(&model, &path);
        // }));
        renderer.connect_toggled(glib::clone!(@weak model => move |_, path| {
            Self::on_tree_cell_item_toggled(&model, &path);
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
            .model(&Self::combo_model_new())
            .text_column(0)
            .editable(true)
            .mode(CellRendererMode::Editable)
            .build();
        renderer.connect_changed(glib::clone!(@weak model => move |combo, path, combo_iter| {
            Self::on_combo_changed(&model, combo, &path, combo_iter);
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
        add.connect_clicked(glib::clone!(@weak this => move |_| this.on_button_add_clicked()));
        bbox.pack_start(&add, false, false, 0);

        let remove = button("Remove", "list-remove-symbolic", "Remove application");
        remove.connect_clicked(glib::clone!(@weak this => move |_| this.on_button_remove_clicked()));
        bbox.pack_start(&remove, false, false, 0);

        let edit = button("Edit", "document-edit-symbolic", "Edit application");
        edit.connect_clicked(glib::clone!(@weak this => move |_| this.on_button_edit_clicked()));
        bbox.pack_start(&edit, false, false, 0);

        selection.connect_changed(glib::clone!(@weak remove, @weak edit => move |selection| {
            Self::on_tree_view_selection_changed(selection, &remove, &edit);
        }));
        Self::on_tree_view_selection_changed(&selection, &remove, &edit);


        let v_close_box = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(0)
            .build();

        let window = window.clone();
        let close = button("Close", "window-close-symbolic", "Close application");
        close.connect_clicked(move |_| {
            window.lock().unwrap().close();
        });
        // close.set_margin_top(12);
        v_close_box.pack_end(&close, false, false, 0);
        vbox.pack_start(&v_close_box, false, false, 0);
        

        (this, vbox)

    }

    fn combo_model_new() -> gtk::ListStore {

        let model = gtk::ListStore::new(&[String::static_type()]);

        for hook in RunHook::ALL {
            model.set(&model.append(), &[(0, &hook.nick())]);
        }

        model
    }

    /// Port of `xfae_model_get`: reads back the entry behind a row, which is what
    /// the edit dialog is filled with.
    fn tree_view_model_get(rel_path: &str) -> Result<(String, String, String, RunHook), glib::Error> {

        let Some(rc) = xfce::Rc::config_open(rel_path, true) else {
            return Err(glib::Error::new(
                glib::FileError::Io,
                &format!("Failed to open {rel_path} for reading"),
            ))
        };

        rc.set_group(xfce::DESKTOP_ENTRY);

        Ok((
            rc.read_entry(c"Name").unwrap_or_default(),
            rc.read_entry(c"Comment").unwrap_or_default(),
            rc.read_entry(c"Exec").unwrap_or_default(),
            RunHook::from_value(rc.read_int_entry(c"RunHook", RunHook::Login.value())),
        ))
    }


    fn tree_view_model_new() -> ListStore {

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

    fn tree_view_model_add(&self, name: String, descr: String, command: String, run_hook: RunHook) -> ListStore {

        let model = self.tree_view
            .model().expect("")
            .downcast::<ListStore>()
            .ok().expect("");

        let icon = ThemedIcon::with_default_fallbacks(DEFAULT_ICON).upcast::<Icon>();
        model.set(&model.append(), &[
            (col::ICON, &icon as &dyn ToValue),
            (col::NAME, &name),
            (col::ENABLED, &true),
            (col::REMOVABLE, &true),
            (col::TOOLTIP, &descr),
            (col::RUN_HOOK, &run_hook.nick()),
            (col::RELPATH, &command),
        ]);


        self.tree_view.set_model(Some(&model));
        model.set_sort_column_id(gtk::SortColumn::Index(1), gtk::SortType::Ascending);

        model
    }

    fn on_tree_cell_item_toggled(
        model: &gtk::ListStore, 
        path: &gtk::TreePath
    ) {

        let Some(iter) = model.iter(path) else {
            return
        };

        let enabled = Self::cell_bool(model, &iter, col::ENABLED);
        model.set_value(&iter, col::ENABLED, &(!enabled).to_value());
    }


    fn on_menu_add_clicked(&self) {

        let parent = Self::toplevel(&self.tree_view);
        let dialog = Dialog::new(parent.as_ref(), None, None, None, RunHook::Login);

        if dialog.run() == gtk::ResponseType::Ok {
            dialog.hide();
            let (name, descr, command, run_hook) = dialog.get();
            self.tree_view_model_add(name, descr, command, run_hook);
        }

        dialog.destroy();
    }

    fn on_menu_remove_clicked(&self) {

        let Some((model, iter)) = self.tree_view.selection().selected() else {
            return
        };

        let Ok(model) = model.downcast::<gtk::ListStore>() else {
            return
        };

        model.remove(&iter);
    }


    fn on_mouse_right_click(
        self: &Rc<Self>,
        tree_view: &gtk::TreeView,
        event: &gtk::gdk::EventButton,
    ) -> glib::Propagation {

        if event.button() != 3 || event.event_type() != gtk::gdk::EventType::ButtonPress {
            return glib::Propagation::Proceed
        }

        let (x, y) = event.position();
        let Some((Some(path), ..)) = tree_view.path_at_pos(x as i32, y as i32) else {
            return glib::Propagation::Proceed
        };

        let selection = tree_view.selection();
        selection.select_path(&path);

        let removable = selection
            .selected()
            .is_some_and(|(model, iter)| Self::cell_bool(&model, &iter, col::REMOVABLE));

        let menu = gtk::Menu::new();

        let add = gtk::MenuItem::with_label("Add");
        add.connect_activate(glib::clone!(@weak self as this => move |_| this.on_menu_add_clicked()));
        menu.append(&add);

        let remove = gtk::MenuItem::with_label("Remove");
        remove.connect_activate(glib::clone!(@weak self as this => move |_| this.on_menu_remove_clicked()));
        remove.set_sensitive(removable);
        menu.append(&remove);

        // Attaching also puts the menu on the screen of the tree view, which is
        // what `gtk_menu_set_screen` does in the C code. `None` pops the menu up on
        // the event being handled, the one the C code passes explicitly.
        menu.set_attach_widget(Some(tree_view));
        menu.show_all();
        menu.popup_at_pointer(None);

        glib::Propagation::Stop
    }

    fn on_combo_changed(
        model: &gtk::ListStore,
        combo: &gtk::CellRendererCombo,
        path: &gtk::TreePath,
        combo_iter: &gtk::TreeIter,
    ) {

        let Some(iter) = model.iter(path) else {
            return
        };

        let Some(combo_model) = combo.model() else {
            return
        };

        let nick = Self::cell_string(&combo_model, combo_iter, 0);
        model.set_value(&iter, col::RUN_HOOK, &nick.to_value());
    }

    fn on_button_add_clicked(&self) {

        let parent = Self::toplevel(&self.tree_view);
        let dialog = Dialog::new(parent.as_ref(), None, None, None, RunHook::Login);

        if dialog.run() == gtk::ResponseType::Ok {
            dialog.hide();
            let (_name, _descr, _command, _run_hook) = dialog.get();
        }

        dialog.destroy();
    }

    fn on_button_remove_clicked(&self) {

        let Some((model, iter)) = self.tree_view.selection().selected() else {
            return
        };

        let Ok(model) = model.downcast::<gtk::ListStore>() else {
            return
        };

        model.remove(&iter);
    }

    fn on_button_edit_clicked(&self) {

        let parent = Self::toplevel(&self.tree_view);

        let Some((model, iter)) = self.tree_view.selection().selected() else {
            return
        };

        let rel_path = Self::cell_string(&model, &iter, col::RELPATH);

        let (name, descr, command, run_hook) = match Self::tree_view_model_get(&rel_path) {
            Ok(entry) => entry,
            Err(error) => {
                xfce::show_error(parent.as_ref(), Some(&error), "Failed to edit item");
                return
            }
        };

        let dialog = Dialog::new(
            parent.as_ref(),
            Some(&name),
            Some(&descr),
            Some(&command),
            run_hook,
        );

        if dialog.run() == gtk::ResponseType::Ok {
            dialog.hide();
            let (_name, _descr, _command, _run_hook) = dialog.get();
        }

        dialog.destroy();
    }

    /// Port of `xfae_window_selection_changed`: Remove asks for an entry every copy
    /// of which sits in a writable directory, which in practice means one the user
    /// owns under `~/.config/autostart`.
    ///
    /// The C code connects this handler to the Edit button as well, so upstream
    /// Edit is greyed out on every system wide entry too — the vast majority of the
    /// list. Here Edit only asks for a selected row: the entry is opened read only
    /// anyway, and its `Exec` is worth looking at even when the file cannot be
    /// rewritten in place.
    fn on_tree_view_selection_changed(
        selection: &gtk::TreeSelection,
        remove: &gtk::Button,
        edit: &gtk::Button,
    ) {

        let selected = selection.selected();

        let removable = selected
            .as_ref()
            .is_some_and(|(model, iter)| Self::cell_bool(model, iter, col::REMOVABLE));

        remove.set_sensitive(removable);
        edit.set_sensitive(selected.is_some());
    }

}