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

#![cfg(not(feature = "disable_autostart"))]
#![allow(dead_code)]

use std::str::FromStr;
use std::{collections::HashMap};
use std::env;
use std::fs::read_to_string;
use std::path::Path;

use osal_rs::utils::{Error, Result};
use xfce4_niri_lib::models::run_hook::RunHook;

pub(crate) const DESKTOP_SUFFIX: &str = ".desktop";

const FALLBACK_DESKTOPS: [&str; 2] = ["niri", "XFCE"];

/// `$XDG_CURRENT_DESKTOP` split on `:`, as matched against `OnlyShowIn` and
/// `NotShowIn`.
pub(crate) fn current_desktops() -> Vec<String> {
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

#[derive(Default, Clone, Debug)]
pub(crate) struct DesktopEntry {
    entries: HashMap<String, String>,
}

impl DesktopEntry {

    const GROUP: &str = "[Desktop Entry]";

    pub(crate) fn read(file: &str) -> Result<Self> {
        let content = read_to_string(file).map_err(|e| Error::UnhandledOwned(format!("{file}: {e}")))?;

        Ok(Self::parse(&content))
    }

    pub(crate) fn parse(content: &str) -> Self {

        let mut entries = HashMap::new();
        let mut in_group = false;

        for line in content.lines() {

            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with('[') {
                in_group = line == Self::GROUP;
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

    /// Raw value, escape sequences left untouched.
    pub(crate) fn raw(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    pub(crate) fn string(&self, key: &str) -> Option<String> {
        self.raw(key).map(unescape)
    }

    pub(crate) fn boolean(&self, key: &str) -> Option<bool> {
        match self.raw(key)? {
            "true" => Some(true),
            "false" => Some(false),
            _ => None
        }
    }

    pub(crate) fn integer(&self, key: &str) -> Option<i32> {
        String::from(self.raw(key)?).parse().ok()
    }

    pub(crate) fn contains_key(&self, key: &str) -> bool {
        self.entries.contains_key(key) && self.integer(key).is_some()
    }

    /// Splits a `;` separated list, honouring `\;` as a literal semicolon.
    pub(crate) fn list(&self, key: &str) -> Vec<String> {

        let Some(value) = self.raw(key) else {
            return Vec::new()
        };

        let mut ret = Vec::<String>::new();
        let mut current = String::new();
        let mut chars = value.chars();

        while let Some(c) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some(escaped) => current.push_str(&unescape_char(escaped)),
                    None => current.push('\\')
                },
                ';' => ret.push(core::mem::take(&mut current)),
                _ => current.push(c)
            }
        }

        if !current.is_empty() {
            ret.push(current);
        }

        ret
    }

    /// Localized value of `key`, looked up along the whole fallback chain the
    /// spec prescribes, down to the unlocalized `key`.
    ///
    /// `locale` is meant to be fed straight from `$LANG`, encoding included.
    #[allow(dead_code)]
    pub(crate) fn localized(&self, key: &str, locale: &str) -> Option<String> {

        locale_candidates(locale)
            .into_iter()
            .find_map(
                |candidate| 
                    self
                    .string(
                        &format!("{key}[{candidate}]")
                    )
                )
            .or_else(|| self.string(key))
    }

    /// `Exec` split into an argv, quoting resolved and field codes dropped.
    ///
    /// `%f %F %u %U` expand to nothing: an autostart entry is never handed a
    /// file. `%i %c %k` and the deprecated codes are dropped as well.
    pub(crate) fn exec_argv(&self) -> Vec<String> {

        let Some(exec) = self.raw("Exec") else {
            return Vec::new()
        };

        let mut ret = Vec::<String>::new();
        let mut current = String::new();
        let mut quoted = false;
        let mut chars = exec.chars();

        while let Some(c) = chars.next() {
            match c {
                '"' => quoted = !quoted,
                '\\' => if let Some(escaped) = chars.next() {
                    // Inside quotes the shell escapes \\ \" \` \$ survive as is,
                    // outside them the format level escapes apply.
                    if quoted {
                        current.push(escaped);
                    } else {
                        current.push_str(&unescape_char(escaped));
                    }
                },
                '%' => if let Some('%') = chars.next() {
                    current.push('%');
                },
                c if c.is_whitespace() && !quoted => if !current.is_empty() {
                    ret.push(core::mem::take(&mut current));
                },
                _ => current.push(c)
            }
        }

        if !current.is_empty() {
            ret.push(current);
        }

        ret
    }

    /// Tells whether this entry has to be launched, or why it has not.
    pub(crate) fn should_autostart(&self, current_desktops: &[String]) -> Result<(), &'static str> {

        if self.raw("Type") != Some("Application") {
            return Err("Type is not Application")
        }

        // Hidden means "deleted by the user", not "do not show".
        if self.boolean("Hidden").unwrap_or(false) {
            return Err("Hidden is true")
        }

        // Not in the spec, but honoured by Xfce and GNOME alike.
        if !self.boolean("X-XFCE-Autostart-Override").unwrap_or(false) && !self.boolean("X-GNOME-Autostart-enabled").unwrap_or(true) {
            return Err("X-GNOME-Autostart-enabled is false")
        }

        if self.contains_key("RunHook") {
            let run_hook = self.integer("RunHook");
            if run_hook.is_none() {
                return Err("RunHook parse error");
            }

            let run_hook = run_hook.unwrap_or_default();
            if run_hook != RunHook::Login.into() {
                return Ok(());
            }
        }

        let current_desktops: Vec<_> = current_desktops
            .to_owned()
            .iter()
            .map(|it| it.to_lowercase() )
            .collect();

        let only_show_in = self.list("OnlyShowIn");
        if !only_show_in.is_empty() && !only_show_in.iter().any(|it| current_desktops.contains(&it.to_lowercase())) {
            return Err("OnlyShowIn does not match the current desktop")
        }

        if self.list("NotShowIn").iter().any(|it| current_desktops.contains(&it.to_lowercase())) {
            return Err("NotShowIn matches the current desktop")
        }

        if let Some(try_exec) = self.raw("TryExec")
            && !binary_exists(try_exec) {
            return Err("TryExec binary not found")
        }

        Ok(())
    }
}

/// Resolves a `TryExec` value: an absolute or relative path as is, a bare name
/// against `$PATH`.
fn binary_exists(binary: &str) -> bool {

    if binary.contains('/') {
        return Path::new(binary).is_file()
    }

    env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|dir| Path::new(dir).join(binary).is_file())
}

/// Reduces a `lang_COUNTRY.ENCODING@MODIFIER` locale to the lookup order the
/// spec prescribes: `lang_COUNTRY@MODIFIER`, `lang_COUNTRY`, `lang@MODIFIER`,
/// `lang`. The encoding never takes part in the match.
fn locale_candidates(locale: &str) -> Vec<String> {

    let (base, modifier) = match locale.split_once('@') {
        Some((base, modifier)) => (base, Some(modifier)),
        None => (locale, None)
    };

    // lang_COUNTRY.ENCODING -> lang_COUNTRY
    let base = base.split('.').next().unwrap_or(base);
    let lang = base.split('_').next().unwrap_or(base);

    let mut ret = Vec::<String>::new();

    if base != lang {
        if let Some(modifier) = modifier {
            ret.push(format!("{base}@{modifier}"));
        }
        ret.push(base.to_string());
    }

    if let Some(modifier) = modifier {
        ret.push(format!("{lang}@{modifier}"));
    }

    if !lang.is_empty() {
        ret.push(lang.to_string());
    }

    ret
}

fn unescape_char(c: char) -> String {
    match c {
        's' => " ".to_string(),
        'n' => "\n".to_string(),
        't' => "\t".to_string(),
        'r' => "\r".to_string(),
        '\\' => "\\".to_string(),
        ';' => ";".to_string(),
        _ => format!("\\{c}")
    }
}

fn unescape(value: &str) -> String {

    let mut ret = String::with_capacity(value.len());
    let mut chars = value.chars();

    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(escaped) => ret.push_str(&unescape_char(escaped)),
                None => ret.push('\\')
            },
            _ => ret.push(c)
        }
    }

    ret
}


#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# a comment

[Desktop Entry]
Type=Application
Name=Power Manager
Name[it]=Power management
Comment=Keeps a\\syear\\nof stats
Exec=xfce4-power-manager --no-daemon %U
TryExec=/bin/sh
OnlyShowIn=XFCE;niri;
Categories=System;Settings\\;Extra;

[Desktop Action Foo]
Exec=should-be-ignored
Name=Foo
";

    #[test]
    fn parses_only_the_desktop_entry_group() {
        let entry = DesktopEntry::parse(SAMPLE);

        assert_eq!(entry.string("Name").as_deref(), Some("Power Manager"));
        assert_eq!(entry.raw("Exec"), Some("xfce4-power-manager --no-daemon %U"));
    }

    #[test]
    fn unescapes_string_values() {
        let entry = DesktopEntry::parse(SAMPLE);

        assert_eq!(entry.string("Comment").as_deref(), Some("Keeps a year\nof stats"));
    }

    #[test]
    fn splits_lists_before_unescaping() {
        let entry = DesktopEntry::parse(SAMPLE);

        assert_eq!(entry.list("OnlyShowIn"), vec!["XFCE", "niri"]);
        assert_eq!(entry.list("Categories"), vec!["System", "Settings;Extra"]);
    }

    #[test]
    fn resolves_localized_keys() {
        let entry = DesktopEntry::parse(SAMPLE);

        assert_eq!(entry.localized("Name", "it_IT.UTF-8").as_deref(), Some("Power management"));
        assert_eq!(entry.localized("Name", "de").as_deref(), Some("Power Manager"));
    }

    #[test]
    fn prefers_the_most_specific_locale() {
        let entry = DesktopEntry::parse("[Desktop Entry]\nName=Generic\nName[pt]=Portugues\nName[pt_BR]=Brasileiro\n");

        // The country variant wins over the bare language, encoding ignored.
        assert_eq!(entry.localized("Name", "pt_BR.UTF-8").as_deref(), Some("Brasileiro"));
        assert_eq!(entry.localized("Name", "pt_PT.UTF-8").as_deref(), Some("Portugues"));
        assert_eq!(entry.localized("Name", "fr").as_deref(), Some("Generic"));
    }

    #[test]
    fn builds_the_spec_locale_fallback_chain() {
        assert_eq!(locale_candidates("sr_RS.UTF-8@latin"), ["sr_RS@latin", "sr_RS", "sr@latin", "sr"]);
        assert_eq!(locale_candidates("it_IT.UTF-8"), ["it_IT", "it"]);
        assert_eq!(locale_candidates("it"), ["it"]);
    }

    #[test]
    fn drops_field_codes_from_exec() {
        let entry = DesktopEntry::parse(SAMPLE);

        assert_eq!(entry.exec_argv(), vec!["xfce4-power-manager", "--no-daemon"]);
    }

    #[test]
    fn keeps_quoted_exec_arguments_together() {
        let entry = DesktopEntry::parse("[Desktop Entry]\nExec=/bin/sh -c \"echo 100%% ok\"\n");

        assert_eq!(entry.exec_argv(), vec!["/bin/sh", "-c", "echo 100% ok"]);
    }

    #[test]
    fn honours_only_show_in() {
        let entry = DesktopEntry::parse(SAMPLE);

        assert!(entry.should_autostart(&["niri".to_string()]).is_ok());
        assert!(entry.should_autostart(&["GNOME".to_string()]).is_err());
    }

    #[test]
    fn skips_hidden_and_disabled_entries() {
        let hidden = DesktopEntry::parse("[Desktop Entry]\nType=Application\nExec=foo\nHidden=true\n");
        assert!(hidden.should_autostart(&["niri".to_string()]).is_err());

        let disabled = DesktopEntry::parse("[Desktop Entry]\nType=Application\nExec=foo\nX-GNOME-Autostart-enabled=false\n");
        assert!(disabled.should_autostart(&["niri".to_string()]).is_err());

        let missing = DesktopEntry::parse("[Desktop Entry]\nType=Application\nExec=foo\nTryExec=/nope/nope\n");
        assert!(missing.should_autostart(&["niri".to_string()]).is_err());
    }
}


