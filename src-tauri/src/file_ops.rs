//! file_ops provides path-normalization shared by ios_client and android_client so that
//! callers can join a base path (mount point, app-data root, /sdcard) with a
//! user-supplied relative path without producing double slashes or missing leading slashes.
//!
//! `join_path` and `normalize_path` originated as minimal helpers added early (during
//! Task 3) because `android_client` needed them to build path strings for app-container
//! and external-storage access. Task 7 is the authoritative implementation: it also
//! collapses internal double slashes (a superset of the original spec's literal
//! behavior, kept because android_client's tests/usage rely on it) and adds `..`/`.`
//! traversal hardening (see `collapse_dot_segments` below).

/// Normalize a remote path: ensure it starts with / and has no trailing slash
/// (except for the root path "/", which is left as-is). Also collapses any
/// internal duplicate slashes (e.g. "/sdcard//DCIM///" -> "/sdcard/DCIM").
pub fn normalize_path(path: &str) -> String {
    let p = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{}", path)
    };
    let collapsed: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
    if collapsed.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", collapsed.join("/"))
    }
}

/// Join a base path and a relative child path, normalizing the result so that
/// callers never manually concatenate strings (which caused double-slash bugs
/// when `child` already started with "/").
pub fn join_path(base: &str, child: &str) -> String {
    let base = normalize_path(base);
    let child = child.trim_start_matches('/');
    if child.is_empty() {
        base
    } else if base == "/" {
        normalize_path(&format!("/{}", child))
    } else {
        normalize_path(&format!("{}/{}", base, child))
    }
}

// --- Security hardening beyond the literal spec (Task 4 code-review follow-up) ---
//
// The base normalize_path/join_path above do not reject or collapse `..`/`.`
// segments. A caller-supplied relative path containing `..` could otherwise walk
// outside the intended base directory once joined, e.g.
// join_path("/data/data/com.example", "../../etc/passwd") would literally produce
// "/data/data/com.example/../../etc/passwd" — which, when later resolved by the
// OS/adb/ifuse, could escape the app's sandboxed root.
//
// `sanitize_relative_path` collapses `.` segments and rejects (rather than
// silently collapsing) any `..` segment, since a legitimate caller-supplied
// relative child path should never need to reference a parent of the base it is
// being joined to; there is no known real use case in this codebase for that.
// Returns None if the path is unsafe (contains `..`).
//
// This is intentionally a separate, explicit function rather than baked into
// join_path/normalize_path, so existing callers are unaffected unless they
// opt in, and so the two spec-mandated functions keep exactly the documented
// literal behavior (traceable to the plan) while security-sensitive call sites
// can additionally call this guard on caller-supplied relative paths.
pub fn sanitize_relative_path(child: &str) -> Option<String> {
    let mut segments = Vec::new();
    for seg in child.split('/') {
        match seg {
            "" | "." => continue,
            ".." => return None,
            s => segments.push(s),
        }
    }
    Some(segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Task 3's original coverage, kept for continuity ---

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

    // --- Task 7 spec-mandated tests (exact names for traceability with the plan) ---

    #[test]
    fn test_normalize_path_adds_leading_slash() {
        assert_eq!(normalize_path("sdcard/DCIM"), "/sdcard/DCIM");
    }

    #[test]
    fn test_normalize_path_removes_trailing_slash() {
        assert_eq!(normalize_path("/sdcard/DCIM/"), "/sdcard/DCIM");
    }

    #[test]
    fn test_normalize_path_already_normalized() {
        assert_eq!(normalize_path("/sdcard/DCIM"), "/sdcard/DCIM");
    }

    #[test]
    fn test_normalize_path_root_stays_root() {
        assert_eq!(normalize_path("/"), "/");
    }

    #[test]
    fn test_join_path_avoids_double_slash() {
        assert_eq!(join_path("/data/data/com.example", "/"), "/data/data/com.example");
        assert_eq!(join_path("/data/data/com.example", "/foo/bar"), "/data/data/com.example/foo/bar");
    }

    #[test]
    fn test_join_path_with_relative_child() {
        assert_eq!(join_path("/sdcard", "DCIM/photo.jpg"), "/sdcard/DCIM/photo.jpg");
    }

    // --- Security hardening tests (beyond literal spec, see module doc comment) ---

    #[test]
    fn test_sanitize_relative_path_rejects_parent_traversal() {
        assert_eq!(sanitize_relative_path("../../etc/passwd"), None);
        assert_eq!(sanitize_relative_path("foo/../bar"), None);
    }

    #[test]
    fn test_sanitize_relative_path_collapses_dot_segments() {
        assert_eq!(sanitize_relative_path("./DCIM/./photo.jpg"), Some("DCIM/photo.jpg".to_string()));
    }

    #[test]
    fn test_sanitize_relative_path_allows_normal_relative_path() {
        assert_eq!(sanitize_relative_path("DCIM/photo.jpg"), Some("DCIM/photo.jpg".to_string()));
    }
}
