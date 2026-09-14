//! `PATH` lookup without shelling out to `sh -c 'command -v …'`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Resolve `name` against `path_var` the way a shell would: a name containing a
/// separator is used as-is, otherwise every `PATH` entry is probed for an
/// executable file.
pub fn lookup_in(name: &str, path_var: Option<&OsStr>) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    if name.contains(std::path::MAIN_SEPARATOR) {
        let direct = PathBuf::from(name);
        return is_executable_file(&direct).then_some(direct);
    }
    std::env::split_paths(path_var?)
        .filter(|dir| !dir.as_os_str().is_empty())
        .flat_map(|dir| {
            std::iter::once(dir.join(name))
                .chain(exe_suffixes().map(move |ext| dir.join(format!("{name}{ext}"))))
        })
        .find(|candidate| is_executable_file(candidate))
}

/// Extensions Windows appends to a bare command name (`PATHEXT`); empty on Unix,
/// where the name on `PATH` is already the whole file name.
fn exe_suffixes() -> impl Iterator<Item = String> {
    let raw = if cfg!(windows) {
        std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
    } else {
        String::new()
    };
    raw.split(';')
        .filter(|ext| !ext.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>()
        .into_iter()
}

/// [`lookup_in`] against the process `PATH`.
pub fn lookup(name: &str) -> Option<PathBuf> {
    lookup_in(name, std::env::var_os("PATH").as_deref())
}

/// Whether `name` resolves to an executable on `PATH`.
pub fn on_path(name: &str) -> bool {
    lookup(name).is_some()
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[cfg(unix)]
    fn touch_exe(dir: &Path, name: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("optionmusic-which-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    #[cfg(unix)]
    fn finds_first_executable_match_across_path_entries() {
        let empty = scratch("empty");
        let real = scratch("real");
        let exe = touch_exe(&real, "msc-fake-tool");
        // A non-executable shadow earlier on PATH must not win.
        std::fs::write(empty.join("msc-fake-tool"), b"data").unwrap();

        let path = OsString::from(format!("{}:{}", empty.display(), real.display()));
        assert_eq!(lookup_in("msc-fake-tool", Some(&path)), Some(exe));
        assert_eq!(lookup_in("msc-missing-tool", Some(&path)), None);
    }

    #[test]
    fn empty_name_and_missing_path_var_resolve_to_none() {
        assert_eq!(lookup_in("", None), None);
        assert_eq!(lookup_in("sh", None), None);
    }

    #[test]
    #[cfg(unix)]
    fn explicit_path_bypasses_path_var() {
        let dir = scratch("direct");
        let exe = touch_exe(&dir, "msc-direct-tool");
        assert_eq!(lookup_in(exe.to_str().unwrap(), None), Some(exe));
        assert_eq!(lookup_in(&format!("{}/nope", dir.display()), None), None);
    }
}
