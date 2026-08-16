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

#![allow(dead_code)]

//! Helpers shared by the unit tests: the process wide state the library reads
//! (`PATH`, `NIRI_SOCKET`, the working directory) is one thing for the whole
//! test binary, so every test touching it goes through [`EnvGuard`].

use std::ffi::{OsStr, OsString};
use std::fs::{self, File, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, Once};
use std::{env, process};

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

/// The lock every [`EnvGuard`] holds, so two tests never see each other's
/// environment. Poisoning is ignored: a panicking test leaves the environment
/// restored by the guard's `Drop` anyway.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|it| it.into_inner())
}

/// Serialises the test against every other one using a guard, and puts back
/// what it changed once it goes out of scope.
pub struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    vars: Vec<(OsString, Option<OsString>)>,
    cwd: Option<PathBuf>,
}

impl EnvGuard {

    pub fn new() -> Self {
        Self { _lock: env_lock(), vars: Vec::new(), cwd: None }
    }

    pub fn set(&mut self, key: &str, value: impl AsRef<OsStr>) -> &mut Self {
        self.save(key);
        // Safe: no other thread reads the environment while the lock is held.
        unsafe { env::set_var(key, value) };
        self
    }

    pub fn unset(&mut self, key: &str) -> &mut Self {
        self.save(key);
        unsafe { env::remove_var(key) };
        self
    }

    pub fn chdir(&mut self, dir: &Path) -> &mut Self {
        if self.cwd.is_none() {
            self.cwd = env::current_dir().ok();
        }
        env::set_current_dir(dir).expect("cannot change the working directory");
        self
    }

    /// Remembers the first value seen for `key`, which is the one to restore.
    fn save(&mut self, key: &str) {
        if !self.vars.iter().any(|(it, _)| it == key) {
            self.vars.push((key.into(), env::var_os(key)));
        }
    }
}

impl Drop for EnvGuard {

    fn drop(&mut self) {

        for (key, value) in &self.vars {
            match value {
                Some(value) => unsafe { env::set_var(key, value) },
                None => unsafe { env::remove_var(key) },
            }
        }

        if let Some(cwd) = &self.cwd {
            let _ = env::set_current_dir(cwd);
        }
    }
}

/// A directory under `$TMPDIR`, removed on drop.
pub struct TempDir(PathBuf);

impl TempDir {

    pub fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);

        let path = env::temp_dir().join(format!(
            "xfce4-niri-lib-{}-{}",
            process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
        ));

        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("cannot create the temporary directory");

        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// An empty file named `name` with permissions `mode`.
    pub fn file(&self, name: impl AsRef<Path>, mode: u32) -> PathBuf {

        let path = self.0.join(name);

        File::create(&path).expect("cannot create the file");
        fs::set_permissions(&path, Permissions::from_mode(mode)).expect("cannot set the mode");

        path
    }

    pub fn dir(&self, name: impl AsRef<Path>) -> PathBuf {

        let path = self.0.join(name);
        fs::create_dir_all(&path).expect("cannot create the directory");

        path
    }
}

impl Drop for TempDir {

    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
