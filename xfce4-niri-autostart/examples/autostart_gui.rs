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

//! Example: a GTK3 front end for the autostart entries `xfce4-niri-service`
//! launches at session start.
//!
//! It shows the same merge the service does - `/etc/xdg/autostart` overlaid by
//! `$XDG_CONFIG_HOME/autostart` - in a `TreeView` whose first column toggles
//! whether an entry is started, and lets one add, edit and remove entries.
//!
//! The system directory is never touched: disabling, editing or removing a
//! system wide entry writes an override into the user directory, which is what
//! the merge in `Autostart::start` picks up. The "Note" column mirrors
//! `DesktopEntry::should_autostart`, so an entry that would be skipped anyway
//! (wrong desktop, missing `TryExec` binary, ...) says so.
//!
//! Run with:
//!   cargo run -p xfce4-niri-autostart --example autostart_gui
//!
//! Needs GTK 3 (and its development files) plus a Wayland or X11 display.
//!
//! Note: this writes real files under `$XDG_CONFIG_HOME/autostart`
//! (`~/.config/autostart` by default), exactly like the Xfce session settings
//! dialog does.

use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use gtk::glib;
use gtk::prelude::*;
// `TreeViewColumn` inherits `pack_start`/`add_attribute` from both
// `CellLayoutExt` and `TreeViewColumnExt`: name the one we mean.
use gtk::prelude::TreeViewColumnExt as Column;

const APP_ID: &str = "org.xfce.niri.AutostartExample";

const DESKTOP_SUFFIX: &str = ".desktop";
const XDG_AUTOSTART: &str = "/etc/xdg/autostart";

const GROUP: &str = "[Desktop Entry]";
const ENABLED_KEY: &str = "X-GNOME-Autostart-enabled";

const FALLBACK_DESKTOPS: [&str; 2] = ["niri", "XFCE"];

const DEFAULT_ICON: &str = "application-x-executable";

/// Columns of the `ListStore` backing the view. `SEARCH`, `PATH` and `SYSTEM`
/// are not displayed: the first feeds the type ahead search, which cannot use
/// the marked up `NAME`, the other two carry what the callbacks need to touch
/// the right file.
mod col {
    pub const ENABLED: u32 = 0;
    pub const ICON: u32 = 1;
    pub const NAME: u32 = 2;
    pub const EXEC: u32 = 3;
    pub const SOURCE: u32 = 4;
    pub const NOTE: u32 = 5;
    pub const SEARCH: u32 = 6;
    pub const PATH: u32 = 7;
    pub const SYSTEM: u32 = 8;
}

//
// --- desktop entry ---------------------------------------------------------
//

/// A `.desktop` file reduced to its `[Desktop Entry]` group.
#[derive(Default, Clone, Debug)]
struct DesktopEntry {
    entries: HashMap<String, String>,
}

impl DesktopEntry {

    fn parse(content: &str) -> Self {

        let mut entries = HashMap::new();
        let mut in_group = false;

        for line in content.lines() {

            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with('[') {
                in_group = line == GROUP;
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };

            if in_group {
                entries.insert(key.trim_end().to_string(), value.trim_start().to_string());
            }
        }

        Self { entries }
    }

    fn raw(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    fn string(&self, key: &str) -> Option<String> {
        self.raw(key).map(unescape)
    }

    fn boolean(&self, key: &str) -> Option<bool> {
        match self.raw(key)? {
            "true" => Some(true),
            "false" => Some(false),
            _ => None
        }
    }

    fn list(&self, key: &str) -> Vec<String> {
        self.raw(key)
            .unwrap_or_default()
            .split(';')
            .filter(|it| !it.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// `key[lang_COUNTRY]`, then `key[lang]`, then the unlocalized `key`.
    fn localized(&self, key: &str, locale: &str) -> Option<String> {

        // lang_COUNTRY.ENCODING@MODIFIER -> lang_COUNTRY
        let base = locale.split(['.', '@']).next().unwrap_or(locale);
        let lang = base.split('_').next().unwrap_or(base);

        self.string(&format!("{key}[{base}]"))
            .or_else(|| self.string(&format!("{key}[{lang}]")))
            .or_else(|| self.string(key))
    }

    /// Same verdict as the service's `should_autostart`, minus the launching.
    fn skip_reason(&self, desktops: &[String]) -> Option<&'static str> {

        if self.raw("Type") != Some("Application") {
            return Some("Type is not Application")
        }

        let only_show_in = self.list("OnlyShowIn");
        if !only_show_in.is_empty() && !only_show_in.iter().any(|it| desktops.contains(it)) {
            return Some("not shown in this desktop")
        }

        if self.list("NotShowIn").iter().any(|it| desktops.contains(it)) {
            return Some("not shown in this desktop")
        }

        if let Some(try_exec) = self.raw("TryExec")
            && !binary_exists(try_exec) {
            return Some("TryExec binary not found")
        }

        if self.raw("Exec").unwrap_or_default().is_empty() {
            return Some("Exec key missing")
        }

        None
    }

    /// `Hidden` means "deleted by the user", `X-GNOME-Autostart-enabled` is the
    /// toggle Xfce and GNOME write. Either one keeps the entry from starting.
    fn is_enabled(&self) -> bool {
        !self.boolean("Hidden").unwrap_or(false) && self.boolean(ENABLED_KEY).unwrap_or(true)
    }
}

/// One row of the view: the merged entry plus where its file came from.
struct AutostartEntry {
    name: String,
    comment: String,
    icon: String,
    exec: String,
    note: String,
    enabled: bool,
    /// The file the entry was read from, system or user. Its file name is the
    /// key the two directories merge on.
    path: PathBuf,
    system: bool,
}

/// `$XDG_CURRENT_DESKTOP` split on `:`, as matched against `OnlyShowIn`.
fn current_desktops() -> Vec<String> {

    let desktops: Vec<String> = env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .filter(|it| !it.is_empty())
        .map(str::to_string)
        .collect();

    if desktops.is_empty() {
        FALLBACK_DESKTOPS.iter().map(|it| it.to_string()).collect()
    } else {
        desktops
    }
}

fn binary_exists(binary: &str) -> bool {

    let Ok(argv) = glib::shell_parse_argv(binary) else {
        return true // Unparsable: the C code leaves the entry alone.
    };

    let Some(program) = argv.first() else {
        return true
    };

    let path = Path::new(program);

    (path.exists() && path.is_file()) || glib::find_program_in_path(program).is_some()
}



fn user_autostart_dir() -> PathBuf {

    if let Ok(config) = env::var("XDG_CONFIG_HOME")
        && !config.is_empty() {
        return PathBuf::from(config).join("autostart")
    }

    PathBuf::from(env::var("HOME").unwrap_or_default())
        .join(".config")
        .join("autostart")
}

fn desktop_files(dir: &Path) -> Vec<PathBuf> {

    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new()
    };

    read.flatten()
        .map(|it| it.path())
        .filter(|it| it.to_string_lossy().ends_with(DESKTOP_SUFFIX))
        .collect()
}

/// The merge the service performs: system first, user on top, keyed by file
/// name so a user file shadows the system one it overrides.
fn load_entries() -> Vec<AutostartEntry> {

    let locale = env::var("LANG").unwrap_or_default();
    let desktops = current_desktops();

    let mut merged = BTreeMap::<String, AutostartEntry>::new();

    for (dir, system) in [(PathBuf::from(XDG_AUTOSTART), true), (user_autostart_dir(), false)] {
        for path in desktop_files(&dir) {

            let Ok(content) = fs::read_to_string(&path) else {
                continue
            };

            let entry = DesktopEntry::parse(&content);

            let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

            let name = entry
                .localized("Name", &locale)
                .unwrap_or_else(|| file_name.trim_end_matches(DESKTOP_SUFFIX).to_string());

            merged.insert(file_name, AutostartEntry {
                name,
                comment: entry.localized("Comment", &locale).unwrap_or_default(),
                icon: entry.string("Icon").unwrap_or_else(|| DEFAULT_ICON.to_string()),
                exec: entry.string("Exec").unwrap_or_default(),
                note: entry.skip_reason(&desktops).unwrap_or_default().to_string(),
                enabled: entry.is_enabled(),
                path,
                system,
            });
        }
    }

    let mut ret: Vec<AutostartEntry> = merged.into_values().collect();
    ret.sort_by_key(|it| it.name.to_lowercase());
    ret
}

//
// --- writing back ----------------------------------------------------------
//

/// Rewrites `key` inside the `[Desktop Entry]` group, leaving every other line,
/// comment and group untouched. Appends the key, and the group itself, when
/// they are missing.
fn set_key(content: &str, key: &str, value: &str) -> String {

    let mut ret = String::with_capacity(content.len() + key.len() + value.len() + 2);
    let mut in_group = false;
    let mut done = false;

    for line in content.lines() {

        let trimmed = line.trim();

        if trimmed.starts_with('[') {
            // The group ends here and the key never showed up: add it before
            // the next group starts, or it would land in the wrong one.
            if in_group && !done {
                ret.push_str(&format!("{key}={value}\n"));
                done = true;
            }
            in_group = trimmed == GROUP;
        } else if in_group
            && !done
            && let Some((found, _)) = trimmed.split_once('=')
            && found.trim_end() == key {
            ret.push_str(&format!("{key}={value}\n"));
            done = true;
            continue;
        }

        ret.push_str(line);
        ret.push('\n');
    }

    if !done {
        if !in_group {
            ret.push_str(GROUP);
            ret.push('\n');
        }
        ret.push_str(&format!("{key}={value}\n"));
    }

    ret
}

/// The file the user directory holds, or would hold, for `file_name`.
fn user_path(file_name: &str) -> PathBuf {
    user_autostart_dir().join(file_name)
}

/// Reads the entry's current content, applies `edit`, and writes the result to
/// the user directory. A system entry is copied on write, which is exactly how
/// an override is born.
fn write_override(entry_path: &Path, file_name: &str, edit: impl FnOnce(&str) -> String) -> Result<(), String> {

    let content = fs::read_to_string(entry_path).map_err(|e| format!("{}: {e}", entry_path.display()))?;

    let dir = user_autostart_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;

    let target = dir.join(file_name);
    fs::write(&target, edit(&content)).map_err(|e| format!("{}: {e}", target.display()))
}

fn set_enabled(entry_path: &Path, file_name: &str, enabled: bool) -> Result<(), String> {

    write_override(entry_path, file_name, |content| {
        // Re-enabling has to clear `Hidden` too: an entry removed through this
        // dialog, or through xfce4-session, is only marked as deleted.
        let content = set_key(content, ENABLED_KEY, if enabled { "true" } else { "false" });
        if enabled {
            set_key(&content, "Hidden", "false")
        } else {
            content
        }
    })
}

/// What the add/edit dialog collects. Kept to the keys the service actually
/// honours - it spawns `Exec` as is and ignores `Terminal`.
#[derive(Default, Clone)]
struct Fields {
    name: String,
    comment: String,
    exec: String,
    enabled: bool,
}

fn escape_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
}

fn save_fields(entry_path: Option<&Path>, file_name: &str, fields: &Fields) -> Result<(), String> {

    let apply = |content: &str| {
        let content = set_key(content, "Name", &escape_value(&fields.name));
        let content = set_key(&content, "Comment", &escape_value(&fields.comment));
        let content = set_key(&content, "Exec", &escape_value(&fields.exec));
        let content = set_key(&content, ENABLED_KEY, if fields.enabled { "true" } else { "false" });
        set_key(&content, "Hidden", if fields.enabled { "false" } else { "true" })
    };

    match entry_path {
        Some(path) => write_override(path, file_name, apply),
        None => {
            let dir = user_autostart_dir();
            fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;

            let target = dir.join(file_name);
            let skeleton = format!("{GROUP}\nType=Application\nVersion=1.0\n");

            fs::write(&target, apply(&skeleton)).map_err(|e| format!("{}: {e}", target.display()))
        }
    }
}

/// A free `<slug>.desktop` name in the user directory, derived from `name`.
fn free_file_name(name: &str) -> String {

    let slug: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();

    let slug = slug.trim_matches('-').to_string();
    let slug = if slug.is_empty() { "autostart-entry".to_string() } else { slug };

    let mut candidate = format!("{slug}{DESKTOP_SUFFIX}");
    let mut suffix = 1;

    while user_path(&candidate).exists() || Path::new(XDG_AUTOSTART).join(&candidate).exists() {
        candidate = format!("{slug}-{suffix}{DESKTOP_SUFFIX}");
        suffix += 1;
    }

    candidate
}

/// Removing a user entry deletes its file. Removing a system entry cannot, so
/// it leaves behind the `Hidden=true` tombstone the spec prescribes.
fn remove_entry(entry_path: &Path, file_name: &str, system: bool) -> Result<(), String> {

    if system {
        return write_override(entry_path, file_name, |content| set_key(content, "Hidden", "true"))
    }

    fs::remove_file(entry_path).map_err(|e| format!("{}: {e}", entry_path.display()))?;

    // A user file may have been shadowing a system one: drop the override and
    // the system entry comes back, so mark it as deleted instead.
    let system_path = Path::new(XDG_AUTOSTART).join(file_name);
    if system_path.exists() {
        return write_override(&system_path, file_name, |content| set_key(content, "Hidden", "true"))
    }

    Ok(())
}

fn unescape(value: &str) -> String {

    let mut ret = String::with_capacity(value.len());
    let mut chars = value.chars();

    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('s') => ret.push(' '),
                Some('n') => ret.push('\n'),
                Some('t') => ret.push('\t'),
                Some('r') => ret.push('\r'),
                Some('\\') => ret.push('\\'),
                Some(other) => {
                    ret.push('\\');
                    ret.push(other);
                }
                None => ret.push('\\')
            },
            _ => ret.push(c)
        }
    }

    ret
}

//
// --- ui --------------------------------------------------------------------
//

fn main() -> glib::ExitCode {

    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(build_ui);

    // The example takes no arguments of its own, and GTK would refuse the ones
    // cargo passes through.
    app.run_with_args::<&str>(&[])
}

fn build_ui(app: &gtk::Application) {

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Autostart applications")
        .default_width(820)
        .default_height(520)
        .build();

    let store = gtk::ListStore::new(&[
        bool::static_type(),   // ENABLED
        String::static_type(), // ICON
        String::static_type(), // NAME (markup)
        String::static_type(), // EXEC
        String::static_type(), // SOURCE
        String::static_type(), // NOTE
        String::static_type(), // SEARCH
        String::static_type(), // PATH
        bool::static_type(),   // SYSTEM
    ]);

    let tree = gtk::TreeView::builder()
        .model(&store)
        .headers_visible(true)
        .enable_search(true)
        .search_column(col::SEARCH as i32)
        .build();

    build_columns(&tree, &store, &window);

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .shadow_type(gtk::ShadowType::In)
        .expand(true)
        .build();
    scroller.add(&tree);

    let add = gtk::Button::with_mnemonic("_Add");
    let edit = gtk::Button::with_mnemonic("_Edit");
    let remove = gtk::Button::with_mnemonic("_Remove");

    let actions = gtk::ActionBar::new();
    actions.pack_start(&add);
    actions.pack_start(&edit);
    actions.pack_start(&remove);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.pack_start(&scroller, true, true, 0);
    content.pack_start(&actions, false, false, 0);

    window.add(&content);

    // Edit and remove only make sense on a selection.
    let selection = tree.selection();
    selection.connect_changed(glib::clone!(@weak edit, @weak remove => move |selection| {
        let has_selection = selection.selected().is_some();
        edit.set_sensitive(has_selection);
        remove.set_sensitive(has_selection);
    }));
    edit.set_sensitive(false);
    remove.set_sensitive(false);

    add.connect_clicked(glib::clone!(@weak window, @weak store => move |_| {

        let initial = Fields { enabled: true, ..Default::default() };

        let Some(fields) = edit_dialog(&window, "New autostart entry", &initial) else {
            return
        };

        let file_name = free_file_name(&fields.name);

        if let Err(e) = save_fields(None, &file_name, &fields) {
            show_error(&window, &e);
        }

        populate(&store);
    }));

    edit.connect_clicked(glib::clone!(@weak window, @weak store, @weak selection => move |_| {

        let Some((model, iter)) = selection.selected() else {
            return
        };

        let path = PathBuf::from(cell_string(&model, &iter, col::PATH));
        let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

        let Ok(content) = fs::read_to_string(&path) else {
            show_error(&window, &format!("Cannot read {}", path.display()));
            return
        };

        let entry = DesktopEntry::parse(&content);
        let locale = env::var("LANG").unwrap_or_default();

        let initial = Fields {
            name: entry.localized("Name", &locale).unwrap_or_default(),
            comment: entry.localized("Comment", &locale).unwrap_or_default(),
            exec: entry.string("Exec").unwrap_or_default(),
            enabled: entry.is_enabled(),
        };

        let Some(fields) = edit_dialog(&window, "Edit autostart entry", &initial) else {
            return
        };

        if let Err(e) = save_fields(Some(&path), &file_name, &fields) {
            show_error(&window, &e);
        }

        populate(&store);
    }));

    remove.connect_clicked(glib::clone!(@weak window, @weak store, @weak selection => move |_| {

        let Some((model, iter)) = selection.selected() else {
            return
        };

        let path = PathBuf::from(cell_string(&model, &iter, col::PATH));
        let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let system = cell_bool(&model, &iter, col::SYSTEM);

        let question = if system {
            "This entry is installed system wide and cannot be deleted.\n\
             It will be marked as removed for your session only. Continue?"
        } else {
            "Delete this autostart entry?"
        };

        if !confirm(&window, question) {
            return
        }

        if let Err(e) = remove_entry(&path, &file_name, system) {
            show_error(&window, &e);
        }

        populate(&store);
    }));

    populate(&store);

    window.show_all();
}

fn build_columns(tree: &gtk::TreeView, store: &gtk::ListStore, window: &gtk::ApplicationWindow) {

    // Enabled: the toggle writes the file straight away, the way the Xfce
    // session dialog does, so there is no apply button to forget about.
    let toggle = gtk::CellRendererToggle::new();
    toggle.set_activatable(true);
    toggle.connect_toggled(glib::clone!(@weak store, @weak window => move |_, path| {

        let Some(iter) = store.iter(&path) else {
            return
        };

        let model: gtk::TreeModel = store.clone().upcast();
        let entry_path = PathBuf::from(cell_string(&model, &iter, col::PATH));
        let file_name = entry_path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let enabled = cell_bool(&model, &iter, col::ENABLED);

        if let Err(e) = set_enabled(&entry_path, &file_name, !enabled) {
            show_error(&window, &e);
        }

        // A system entry has just grown a user override, so the source column
        // changed too: reload rather than patch the single cell.
        populate(&store);
    }));

    let column = gtk::TreeViewColumn::new();
    column.set_title("On");
    Column::pack_start(&column, &toggle, false);
    Column::add_attribute(&column, &toggle, "active", col::ENABLED as i32);
    tree.append_column(&column);

    // Name: icon and the markup built in populate().
    let column = gtk::TreeViewColumn::new();
    column.set_title("Application");
    column.set_expand(true);
    column.set_resizable(true);

    let icon = gtk::CellRendererPixbuf::new();
    Column::pack_start(&column, &icon, false);
    Column::add_attribute(&column, &icon, "icon-name", col::ICON as i32);

    let text = gtk::CellRendererText::new();
    Column::pack_start(&column, &text, true);
    Column::add_attribute(&column, &text, "markup", col::NAME as i32);
    tree.append_column(&column);

    for (title, index, ellipsize) in [
        ("Command", col::EXEC, true),
        ("Source", col::SOURCE, false),
        ("Note", col::NOTE, false),
    ] {
        let renderer = gtk::CellRendererText::new();
        if ellipsize {
            renderer.set_ellipsize(gtk::pango::EllipsizeMode::End);
            renderer.set_width_chars(28);
        }

        let column = gtk::TreeViewColumn::new();
        column.set_title(title);
        column.set_resizable(true);
        Column::pack_start(&column, &renderer, true);
        Column::add_attribute(&column, &renderer, "text", index as i32);
        tree.append_column(&column);
    }
}

fn populate(store: &gtk::ListStore) {

    store.clear();

    for entry in load_entries() {

        let name = glib::markup_escape_text(&entry.name);

        let markup = if entry.comment.is_empty() {
            format!("<b>{name}</b>")
        } else {
            format!("<b>{name}</b>\n<small>{}</small>", glib::markup_escape_text(&entry.comment))
        };

        let source = if entry.system { "system" } else { "user" };

        store.set(&store.append(), &[
            (col::ENABLED, &entry.enabled),
            (col::ICON, &entry.icon),
            (col::NAME, &markup),
            (col::EXEC, &entry.exec),
            (col::SOURCE, &source.to_string()),
            (col::NOTE, &entry.note),
            (col::SEARCH, &entry.name),
            (col::PATH, &entry.path.to_string_lossy().to_string()),
            (col::SYSTEM, &entry.system),
        ]);
    }
}

fn cell_string(model: &gtk::TreeModel, iter: &gtk::TreeIter, column: u32) -> String {
    model.value(iter, column as i32).get::<String>().unwrap_or_default()
}

fn cell_bool(model: &gtk::TreeModel, iter: &gtk::TreeIter, column: u32) -> bool {
    model.value(iter, column as i32).get::<bool>().unwrap_or_default()
}

/// The add/edit form. Returns `None` when the dialog is dismissed, and keeps
/// asking while name or command are empty.
fn edit_dialog(parent: &impl IsA<gtk::Window>, title: &str, initial: &Fields) -> Option<Fields> {

    let dialog = gtk::Dialog::with_buttons(
        Some(title),
        Some(parent),
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        &[("_Cancel", gtk::ResponseType::Cancel), ("_Save", gtk::ResponseType::Accept)]
    );
    dialog.set_default_response(gtk::ResponseType::Accept);

    let grid = gtk::Grid::builder()
        .row_spacing(6)
        .column_spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    let name = entry_field(&initial.name);
    let comment = entry_field(&initial.comment);
    let exec = entry_field(&initial.exec);

    let enabled = gtk::CheckButton::with_label("Start with the session");
    enabled.set_active(initial.enabled);

    for (row, (label, widget)) in [
        ("Name", &name),
        ("Description", &comment),
        ("Command", &exec),
    ].into_iter().enumerate() {

        let label = gtk::Label::builder()
            .label(label)
            .halign(gtk::Align::End)
            .build();

        grid.attach(&label, 0, row as i32, 1, 1);
        grid.attach(widget, 1, row as i32, 1, 1);
    }

    grid.attach(&enabled, 1, 3, 1, 1);

    dialog.content_area().add(&grid);
    dialog.show_all();

    loop {
        if dialog.run() != gtk::ResponseType::Accept {
            unsafe { dialog.destroy() };
            return None
        }

        let fields = Fields {
            name: name.text().trim().to_string(),
            comment: comment.text().trim().to_string(),
            exec: exec.text().trim().to_string(),
            enabled: enabled.is_active(),
        };

        if fields.name.is_empty() || fields.exec.is_empty() {
            show_error(&dialog, "Name and command cannot be empty.");
            continue
        }

        unsafe { dialog.destroy() };
        return Some(fields)
    }
}

fn entry_field(text: &str) -> gtk::Entry {
    gtk::Entry::builder()
        .text(text)
        .hexpand(true)
        .width_chars(40)
        .activates_default(true)
        .build()
}

fn show_error(parent: &impl IsA<gtk::Window>, message: &str) {

    let dialog = gtk::MessageDialog::new(
        Some(parent),
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        gtk::MessageType::Error,
        gtk::ButtonsType::Ok,
        message
    );

    dialog.run();
    unsafe { dialog.destroy() };
}

fn confirm(parent: &impl IsA<gtk::Window>, message: &str) -> bool {

    let dialog = gtk::MessageDialog::new(
        Some(parent),
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        gtk::MessageType::Question,
        gtk::ButtonsType::OkCancel,
        message
    );

    let response = dialog.run();
    unsafe { dialog.destroy() };

    response == gtk::ResponseType::Ok
}


#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# a comment
[Desktop Entry]
Type=Application
Name=Power Manager
Exec=xfce4-power-manager --no-daemon
X-GNOME-Autostart-enabled=false

[Desktop Action Foo]
Exec=should-be-ignored
";

    #[test]
    fn reads_the_enabled_flag() {
        assert!(!DesktopEntry::parse(SAMPLE).is_enabled());
        assert!(DesktopEntry::parse("[Desktop Entry]\nName=Foo\n").is_enabled());
        assert!(!DesktopEntry::parse("[Desktop Entry]\nHidden=true\n").is_enabled());
    }

    #[test]
    fn rewrites_an_existing_key_in_place() {
        let updated = set_key(SAMPLE, ENABLED_KEY, "true");

        assert!(updated.contains("X-GNOME-Autostart-enabled=true"));
        assert!(!updated.contains("X-GNOME-Autostart-enabled=false"));
        assert!(DesktopEntry::parse(&updated).is_enabled());
    }

    #[test]
    fn adds_a_missing_key_before_the_next_group() {
        let updated = set_key(SAMPLE, "Hidden", "true");

        let hidden = updated.find("Hidden=true").expect("key added");
        let action = updated.find("[Desktop Action Foo]").expect("group kept");

        assert!(hidden < action, "the key landed in the wrong group:\n{updated}");
    }

    #[test]
    fn keeps_the_other_groups_and_comments() {
        let updated = set_key(SAMPLE, "Exec", "true");

        assert!(updated.contains("# a comment"));
        assert!(updated.contains("Exec=should-be-ignored"));
        assert_eq!(DesktopEntry::parse(&updated).raw("Exec"), Some("true"));
    }

    #[test]
    fn creates_the_group_when_missing() {
        let updated = set_key("", "Name", "Foo");

        assert_eq!(DesktopEntry::parse(&updated).raw("Name"), Some("Foo"));
    }

    #[test]
    fn round_trips_escaped_values() {
        let updated = set_key("[Desktop Entry]\n", "Comment", &escape_value("a b\nc"));

        assert_eq!(DesktopEntry::parse(&updated).string("Comment").as_deref(), Some("a b\nc"));
    }
}
