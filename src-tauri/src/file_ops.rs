//! Shared local filesystem helper operations used by the transfer pipeline.
//!
//! STUB: This module is a placeholder created in Task 1 so the project compiles
//! standalone. The real implementation is added in Task 7.
//!
//! `join_path` and `normalize_path` below are minimal, functional implementations
//! added early (during Task 3) because `android_client` needs them to build path
//! strings for app-container and external-storage access. Task 7 remains the
//! authoritative place to fully implement/test path handling for this module;
//! these two helpers may be revisited/hardened there.

/// Joins a base directory and a (possibly relative) sub-path into a single
/// normalized path, using `/` separators (adb/device paths are always POSIX-style
/// regardless of host OS).
pub fn join_path(base: &str, sub: &str) -> String {
    let base = base.trim_end_matches('/');
    let sub = sub.trim_start_matches('/');
    if sub.is_empty() {
        normalize_path(base)
    } else {
        normalize_path(&format!("{}/{}", base, sub))
    }
}

/// Normalizes a path by collapsing duplicate slashes and stripping a trailing
/// slash (except for the root path itself).
pub fn normalize_path(path: &str) -> String {
    let is_absolute = path.starts_with('/');
    let collapsed: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let joined = collapsed.join("/");
    if is_absolute {
        format!("/{}", joined)
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_path_basic() {
        assert_eq!(join_path("/data/data/com.example.app", "/files"), "/data/data/com.example.app/files");
    }

    #[test]
    fn test_join_path_empty_sub() {
        assert_eq!(join_path("/data/data/com.example.app", "/"), "/data/data/com.example.app");
        assert_eq!(join_path("/data/data/com.example.app", ""), "/data/data/com.example.app");
    }

    #[test]
    fn test_normalize_path_collapses_slashes() {
        assert_eq!(normalize_path("/sdcard//DCIM///"), "/sdcard/DCIM");
    }
}
