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

#![allow(unused)]

//
// --- XfsmRunHook -----------------------------------------------------------
//

/// Port of `XfsmRunHook`, the trigger an entry is started on.
///
/// The C side registers it as a `GEnum` only to get the labels and the integer
/// written to the `RunHook` key; the discriminants below are those integers and
/// [`RunHook::nick`] returns the `value_nick` strings shown in the UI.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum RunHook {
    #[default]
    Login,
    Logout,
    Shutdown,
    Restart,
    Suspend,
    Hibernate,
    HybridSleep,
    SwitchUser,
}

impl RunHook {

    /// In `GEnumValue` order, the order the combo boxes list them in.
    const ALL: [Self; 8] = [
        Self::Login,
        Self::Logout,
        Self::Shutdown,
        Self::Restart,
        Self::Suspend,
        Self::Hibernate,
        Self::HybridSleep,
        Self::SwitchUser,
    ];

    /// `g_enum_get_value (klass, value)`, falling back to the C default of
    /// `XFSM_RUN_HOOK_LOGIN` for a value no longer in the enum.
    pub(crate) fn from_value(value: i32) -> Self {
        usize::try_from(value)
            .ok()
            .and_then(|it| Self::ALL.get(it))
            .copied()
            .unwrap_or_default()
    }

    /// The position in [`RunHook::ALL`], which is what the `RunHook` key holds.
    pub(crate) fn value(self) -> i32 {
        Self::ALL.iter().position(|it| *it == self).unwrap_or_default() as i32
    }

    /// `GEnumValue.value_nick`.
    pub(crate) fn nick(self) -> &'static str {
        match self {
            Self::Login => "on login",
            Self::Logout => "on logout",
            Self::Shutdown => "on shutdown",
            Self::Restart => "on restart",
            Self::Suspend => "on suspend",
            Self::Hibernate => "on hibernate",
            Self::HybridSleep => "on hybrid sleep",
            Self::SwitchUser => "on switch user",
        }
    }
}