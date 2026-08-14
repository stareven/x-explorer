//! Resolves paths to bundled platform-tool binaries (e.g. libimobiledevice, adb).

use std::path::PathBuf;

/// Returns the path to a bundled binary by name.
/// In development, looks in `src-tauri/binaries/`.
/// In production, looks inside the .app bundle Resources.
pub fn resolve(name: &str) -> Result<PathBuf, String> {
    // Try development path first
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(name);
    if dev_path.exists() {
        return Ok(dev_path);
    }

    // Try production bundle path
    if let Ok(exe) = std::env::current_exe() {
        let bundle_path = exe
            .parent()
            .unwrap_or(&exe)
            .join(name);
        if bundle_path.exists() {
            return Ok(bundle_path);
        }
    }

    Err(format!("Binary '{}' not found in bundle or dev path", name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_returns_error_for_missing_binary() {
        let result = resolve("nonexistent_binary_xyz");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }
}
