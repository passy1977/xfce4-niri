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

#![allow(unused_imports)]

//! Helpers for the unit tests of this crate: the ones working on process wide
//! state come from the library, next to them lives what only the `xfce` module
//! needs.

use std::sync::Once;

pub use xfce4_niri_lib::test_support::{EnvGuard, TempDir};


/// libxfce4util builds its resource directory list on the first call, without
/// a lock: two tests getting there at once see a half built one. Every test
/// touching `XFCE_RESOURCE_CONFIG` calls this first, so that init runs alone;
/// the reads after it are safe from any number of threads.
pub fn xfce_resource_ready() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        crate::xfce::resource_save_location("xfce4-niri-resource-warm-up", false);
    });
}
