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
    pub fn from_value(value: i32) -> Self {
        usize::try_from(value)
            .ok()
            .and_then(|it| Self::ALL.get(it))
            .copied()
            .unwrap_or_default()
    }

    /// The position in [`RunHook::ALL`], which is what the `RunHook` key holds.
    pub fn value(self) -> i32 {
        Self::ALL.iter().position(|it| *it == self).unwrap_or_default() as i32
    }

    /// `GEnumValue.value_nick`.
    pub fn nick(self) -> &'static str {
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


#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn value_is_the_position_in_the_enum() {

        assert_eq!(RunHook::Login.value(), 0);
        assert_eq!(RunHook::Logout.value(), 1);
        assert_eq!(RunHook::Shutdown.value(), 2);
        assert_eq!(RunHook::Restart.value(), 3);
        assert_eq!(RunHook::Suspend.value(), 4);
        assert_eq!(RunHook::Hibernate.value(), 5);
        assert_eq!(RunHook::HybridSleep.value(), 6);
        assert_eq!(RunHook::SwitchUser.value(), 7);
    }

    /// What is written to the `RunHook` key has to read back as itself.
    #[test]
    fn from_value_and_value_round_trip() {

        for (index, hook) in RunHook::ALL.iter().enumerate() {
            assert_eq!(hook.value(), index as i32);
            assert!(RunHook::from_value(hook.value()) == *hook);
        }
    }

    /// A value no longer in the enum reads as the C default, `XFSM_RUN_HOOK_LOGIN`.
    #[test]
    fn from_value_falls_back_to_login() {

        for value in [-1, 8, 42, i32::MIN, i32::MAX] {
            assert!(RunHook::from_value(value) == RunHook::Login, "{value} should fall back");
        }
    }

    #[test]
    fn default_is_login() {
        assert!(RunHook::default() == RunHook::Login);
    }

    #[test]
    fn every_hook_has_its_own_nick() {

        let nicks: Vec<&str> = RunHook::ALL.iter().map(|it| it.nick()).collect();

        assert_eq!(nicks, [
            "on login",
            "on logout",
            "on shutdown",
            "on restart",
            "on suspend",
            "on hibernate",
            "on hybrid sleep",
            "on switch user",
        ]);
    }
}