//! Resolves paths to platform-tool binaries (e.g. libimobiledevice, adb)
//! from the system PATH. The tools are not bundled with the app; if one is
//! missing, the error message tells the user how to install it via Homebrew.

use std::path::{Path, PathBuf};

/// Returns the path to an executable `name` found on the system PATH.
pub fn resolve(name: &str) -> Result<PathBuf, String> {
    if let Some(found) = find_in_path(name) {
        return Ok(found);
    }

    Err(format!(
        "未找到可执行文件 '{}'，请先安装 Homebrew（https://brew.sh），然后执行：{}",
        name,
        brew_hint(name)
    ))
}

/// First executable `name` found in the PATH environment variable.
fn find_in_path(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    find_in_paths(name, std::env::split_paths(&paths).map(|p| p.to_string_lossy().to_string()))
}

fn find_in_paths<I: Iterator<Item = String>>(name: &str, dirs: I) -> Option<PathBuf> {
    dirs.map(|d| PathBuf::from(d).join(name))
        .find(|candidate| is_executable_file(candidate))
}

#[cfg(unix)]
fn is_executable_file(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(p) {
        Ok(m) => m.is_file() && m.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable_file(p: &Path) -> bool {
    p.is_file()
}

/// Homebrew install command for each tool this app shells out to.
fn brew_hint(name: &str) -> &'static str {
    match name {
        "adb" => "brew install --cask android-platform-tools",
        "ideviceinstaller" => "brew install ideviceinstaller",
        // idevice_id, ideviceinfo, afcclient all ship in the libimobiledevice formula
        _ => "brew install libimobiledevice",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_error_includes_brew_hint() {
        let err = resolve("nonexistent_binary_xyz").unwrap_err();
        assert!(err.contains("brew install"), "error should suggest install: {err}");
    }

    #[test]
    fn test_resolve_finds_tool_on_path() {
        // adb/afcclient etc. are dev-machine prerequisites; at least one of
        // these must be resolvable in a dev environment.
        let found = resolve("afcclient").or_else(|_| resolve("adb"));
        assert!(found.is_ok(), "expected afcclient or adb on PATH");
    }

    #[test]
    fn test_find_in_paths_locates_executable() {
        let dir = std::env::temp_dir().join("bin_path_test_exec");
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("fake_test_tool");
        std::fs::write(&bin, "#!/bin/sh\n").unwrap();
        make_executable(&bin);
        let found = find_in_paths("fake_test_tool", ["/nonexistent/dir".to_string(), dir.to_string_lossy().to_string()].into_iter());
        assert_eq!(found, Some(bin));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_find_in_paths_skips_non_executable_file() {
        let dir = std::env::temp_dir().join("bin_path_test_noexec");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("plain_file"), "data").unwrap();
        let found = find_in_paths("plain_file", [dir.to_string_lossy().to_string()].into_iter());
        assert_eq!(found, None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_brew_hint_maps_binary_to_formula() {
        assert_eq!(brew_hint("adb"), "brew install --cask android-platform-tools");
        assert_eq!(brew_hint("ideviceinstaller"), "brew install ideviceinstaller");
        assert_eq!(brew_hint("afcclient"), "brew install libimobiledevice");
        assert_eq!(brew_hint("ideviceinfo"), "brew install libimobiledevice");
    }

    #[cfg(unix)]
    fn make_executable(p: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(p).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(p, perms).unwrap();
    }
}
