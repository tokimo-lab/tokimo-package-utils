//! 跨 OS 本地文件路径工具。
//!
//! 设计原则：
//! - **内部格式始终为 Unix 风格**（`/` 分隔符），前端无需感知 server OS。
//! - Windows：盘符映射为一级目录 —— `/c/Users/...` ↔ `C:\Users\...`。
//! - `internal_to_native` / `native_to_internal` 在文件系统边界做转换，Linux 上为恒等函数。
//! - 远程协议（SMB / FTP / S3 等 VFS）不走这里，由
//!   `rust-server::services::media::source::path::normalize_source_path` 处理。

use std::fmt;

use serde::{Deserialize, Serialize};

/// 描述本机本地文件系统的根入口，供前端浏览。
///
/// - Linux: `["/"]`
/// - Windows: `["/c", "/d", ...]`（实际可访问的盘符）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalPathInfo {
    pub roots: Vec<String>,
}

impl LocalPathInfo {
    #[must_use]
    pub fn current() -> Self {
        Self { roots: list_roots() }
    }
}

/// 路径校验错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    Empty,
    NotAbsolute,
    Traversal,
    /// Windows：首段必须是单字母盘符（如 `/c/...`）
    InvalidDrive,
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("path is empty"),
            Self::NotAbsolute => f.write_str("path must be absolute (start with '/')"),
            Self::Traversal => f.write_str("path traversal ('..') is not allowed"),
            Self::InvalidDrive => f.write_str("path must start with a drive letter (e.g. '/c/...')"),
        }
    }
}

impl std::error::Error for PathError {}

/// 严格校验内部格式的本地绝对路径并返回规范化字符串。
///
/// - 所有平台接受 `/` 开头的 Unix 风格路径
/// - Windows 上首段必须是单字母盘符（如 `/c`、`/d`），否则返回 `InvalidDrive`
/// - 拒绝 `..` 遍历，静默丢弃 `.` 组件
/// - 不触发文件系统访问，不解析符号链接
/// - 返回值始终使用 `/` 分隔符（内部格式）
pub fn normalize_local_path(input: &str) -> Result<String, PathError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(PathError::Empty);
    }

    if !trimmed.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }

    // 拆出有效组件（过滤空段和 `.`）
    let parts: Vec<&str> = trimmed.split('/').filter(|c| !c.is_empty() && *c != ".").collect();

    if parts.is_empty() {
        // Linux: / is the root directory. Windows: / is a virtual drive list,
        // not a valid path for storage operations — require a drive letter.
        #[cfg(windows)]
        {
            return Err(PathError::InvalidDrive);
        }
        #[cfg(not(windows))]
        {
            return Ok("/".to_string());
        }
    }

    // 拒绝 `..` 遍历
    if parts.contains(&"..") {
        return Err(PathError::Traversal);
    }

    #[cfg(windows)]
    {
        let drive = parts[0];
        if drive.len() != 1 || !drive.as_bytes()[0].is_ascii_alphabetic() {
            return Err(PathError::InvalidDrive);
        }
    }

    // 组装规范化路径（Windows 盘符小写）
    let mut normalized = String::with_capacity(trimmed.len());
    for (i, part) in parts.iter().enumerate() {
        normalized.push('/');
        if cfg!(windows) && i == 0 {
            normalized.push((part.as_bytes()[0] as char).to_ascii_lowercase());
        } else {
            normalized.push_str(part);
        }
    }

    Ok(normalized)
}

/// 判断给定字符串是否表示当前 OS 的"根"。
///
/// - 所有平台：空串 / `/` 视为根
/// - Windows 上额外将 `/c`、`/d` 等盘符根视为 root
#[must_use]
pub fn is_local_root(input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return true;
    }
    #[cfg(windows)]
    {
        let bytes = trimmed.as_bytes();
        bytes.len() == 2 && bytes[0] == b'/' && bytes[1].is_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// 列出本机文件系统的根入口（内部格式）。
///
/// - Linux: `vec!["/"]`
/// - Windows: best-effort 探测 A: ~ Z:，能 stat 成功的入选，返回 `/c`、`/d` 等
#[must_use]
pub fn list_roots() -> Vec<String> {
    #[cfg(windows)]
    {
        let mut out = Vec::new();
        for letter in b'a'..=b'z' {
            let native_root = format!("{}:\\", (letter as char).to_ascii_uppercase());
            if std::fs::metadata(&native_root).is_ok() {
                out.push(format!("/{}", letter as char));
            }
        }
        if out.is_empty() { vec!["/c".to_string()] } else { out }
    }
    #[cfg(not(windows))]
    {
        vec!["/".to_string()]
    }
}

/// 内部格式 → OS 原生路径（文件系统边界）。
///
/// **调用方必须先用 [`normalize_local_path`] 校验**，否则可能产生带 `..` 的原生路径。
///
/// - Linux：恒等函数
/// - Windows：`/c/Users/foo` → `C:\Users\foo`，`/c` → `C:\`，`/` → `""`
#[must_use]
pub fn internal_to_native(internal: &str) -> String {
    // Defense in depth: 调用方必须已通过 normalize_local_path 校验。
    // `/` 在 Windows 上对 normalize 非法（缺盘符），但对 internal_to_native 合法（返回空串）。
    debug_assert!(
        normalize_local_path(internal).is_ok() || internal == "/",
        "internal_to_native called with unvalidated path: {internal}"
    );
    #[cfg(not(windows))]
    {
        internal.to_string()
    }
    #[cfg(windows)]
    {
        if internal == "/" {
            return String::new();
        }
        let Some(rest) = internal.strip_prefix('/') else {
            return internal.to_string();
        };
        // "/c/foo/bar" → rest="c/foo/bar"
        match rest.split_once('/') {
            Some((drive, subpath)) if drive.len() == 1 && drive.as_bytes()[0].is_ascii_alphabetic() => {
                let drive_upper = drive.to_ascii_uppercase();
                if subpath.is_empty() {
                    format!("{drive_upper}:\\")
                } else {
                    format!("{}:\\{}", drive_upper, subpath.replace('/', "\\"))
                }
            }
            _ if rest.len() == 1 && rest.as_bytes()[0].is_ascii_alphabetic() => {
                // "/c" → "C:\"
                format!("{}:\\", rest.to_ascii_uppercase())
            }
            _ => {
                // 非盘符路径，仅翻转分隔符
                internal.replace('/', "\\")
            }
        }
    }
}

/// OS 原生路径 → 内部格式（文件系统边界）。
///
/// - Linux：恒等函数
/// - Windows：`C:\Users\foo` → `/c/Users/foo`，`C:\` → `/c`
#[must_use]
pub fn native_to_internal(native: &str) -> String {
    #[cfg(not(windows))]
    {
        native.to_string()
    }
    #[cfg(windows)]
    {
        let bytes = native.as_bytes();
        if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            let drive = (bytes[0] as char).to_ascii_lowercase();
            let rest = native[2..]
                .trim_start_matches(['\\', '/'])
                .trim_end_matches(['\\', '/']);
            if rest.is_empty() {
                format!("/{drive}")
            } else {
                format!("/{}/{}", drive, rest.replace('\\', "/"))
            }
        } else {
            native.replace('\\', "/")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════
    // normalize_local_path
    // ═══════════════════════════════════════════════════════

    #[test]
    fn rejects_empty() {
        assert_eq!(normalize_local_path(""), Err(PathError::Empty));
        assert_eq!(normalize_local_path("   "), Err(PathError::Empty));
    }

    #[test]
    fn rejects_relative() {
        assert_eq!(normalize_local_path("foo/bar"), Err(PathError::NotAbsolute));
        assert_eq!(normalize_local_path("foo"), Err(PathError::NotAbsolute));
        assert_eq!(normalize_local_path("./foo"), Err(PathError::NotAbsolute));
    }

    // ── traversal ──────────────────────────────────────────

    #[test]
    #[cfg(unix)]
    fn rejects_traversal() {
        assert_eq!(normalize_local_path("/foo/../bar"), Err(PathError::Traversal));
        assert_eq!(normalize_local_path("/.."), Err(PathError::Traversal));
        assert_eq!(normalize_local_path("/../foo"), Err(PathError::Traversal));
        // double slashes + traversal
        assert_eq!(normalize_local_path("//foo//..//bar"), Err(PathError::Traversal));
        // dots + traversal
        assert_eq!(normalize_local_path("/./foo/./../bar"), Err(PathError::Traversal));
    }

    #[test]
    #[cfg(windows)]
    fn windows_rejects_traversal() {
        assert_eq!(normalize_local_path("/c/foo/../bar"), Err(PathError::Traversal));
        assert_eq!(normalize_local_path("/c/.."), Err(PathError::Traversal));
    }

    #[test]
    #[cfg(unix)]
    fn non_traversal_dot_variants() {
        // Three dots is a valid filename (not parent dir)
        assert_eq!(normalize_local_path("/foo/.../bar").unwrap(), "/foo/.../bar");
        // Dot-prefixed filenames are not traversal
        assert_eq!(normalize_local_path("/foo/..hidden").unwrap(), "/foo/..hidden");
        assert_eq!(normalize_local_path("/foo/...hidden").unwrap(), "/foo/...hidden");
    }

    // ── normalization ──────────────────────────────────────

    #[test]
    #[cfg(unix)]
    fn normalization_double_slashes() {
        assert_eq!(normalize_local_path("//foo//bar").unwrap(), "/foo/bar");
        assert_eq!(normalize_local_path("///foo").unwrap(), "/foo");
    }

    #[test]
    #[cfg(unix)]
    fn normalization_dot_components() {
        assert_eq!(normalize_local_path("/foo/./bar").unwrap(), "/foo/bar");
        assert_eq!(normalize_local_path("/foo/././bar").unwrap(), "/foo/bar");
        assert_eq!(normalize_local_path("/./foo/./bar/.").unwrap(), "/foo/bar");
        assert_eq!(normalize_local_path("/.").unwrap(), "/"); // dot-only after filtering → root
    }

    #[test]
    #[cfg(unix)]
    fn normalization_trailing_slash() {
        assert_eq!(normalize_local_path("/foo/bar/").unwrap(), "/foo/bar");
        assert_eq!(normalize_local_path("/foo/").unwrap(), "/foo");
    }

    #[test]
    #[cfg(unix)]
    fn normalization_leading_whitespace() {
        assert_eq!(normalize_local_path("  /foo/bar").unwrap(), "/foo/bar");
        assert_eq!(normalize_local_path("\t/foo/bar").unwrap(), "/foo/bar");
    }

    // ── unix-specific ──────────────────────────────────────

    #[test]
    #[cfg(unix)]
    fn unix_accepts_root() {
        assert_eq!(normalize_local_path("/").unwrap(), "/");
    }

    #[test]
    #[cfg(unix)]
    fn unix_accepts_any_absolute() {
        assert_eq!(normalize_local_path("/home/user/docs").unwrap(), "/home/user/docs");
        // On Linux, /c/foo is a regular path, not a drive
        assert_eq!(normalize_local_path("/c/foo").unwrap(), "/c/foo");
    }

    #[test]
    #[cfg(unix)]
    fn unix_rejects_backslash_paths() {
        // C:\foo doesn't start with / so it's NotAbsolute
        assert_eq!(normalize_local_path("C:\\foo"), Err(PathError::NotAbsolute));
        // UNC-style
        assert_eq!(normalize_local_path("\\\\server\\share"), Err(PathError::NotAbsolute));
    }

    // ── windows-specific ───────────────────────────────────

    #[test]
    #[cfg(windows)]
    fn windows_rejects_bare_slash() {
        assert!(matches!(normalize_local_path("/"), Err(PathError::InvalidDrive)));
        // With dots stripped, still bare slash
        assert!(matches!(normalize_local_path("/./."), Err(PathError::InvalidDrive)));
    }

    #[test]
    #[cfg(windows)]
    fn windows_accepts_drive_prefix() {
        assert_eq!(normalize_local_path("/c/Users").unwrap(), "/c/Users");
        assert_eq!(normalize_local_path("/C/Users").unwrap(), "/c/Users");
        assert_eq!(normalize_local_path("/d").unwrap(), "/d");
        assert_eq!(normalize_local_path("/z/foo/bar").unwrap(), "/z/foo/bar");
        // Trailing slash
        assert_eq!(normalize_local_path("/c/Users/").unwrap(), "/c/Users");
        // Double slashes
        assert_eq!(normalize_local_path("//c//Users//foo").unwrap(), "/c/Users/foo");
    }

    #[test]
    #[cfg(windows)]
    fn windows_rejects_no_drive() {
        assert!(matches!(
            normalize_local_path("/Users/foo"),
            Err(PathError::InvalidDrive)
        ));
    }

    #[test]
    #[cfg(windows)]
    fn windows_rejects_invalid_drive() {
        // Two letters
        assert!(matches!(normalize_local_path("/cd/foo"), Err(PathError::InvalidDrive)));
        // Digit
        assert!(matches!(normalize_local_path("/1/foo"), Err(PathError::InvalidDrive)));
        // Empty drive component
        assert!(matches!(normalize_local_path("//foo"), Err(PathError::InvalidDrive)));
        // Special char
        assert!(matches!(normalize_local_path("/$/foo"), Err(PathError::InvalidDrive)));
    }

    #[test]
    #[cfg(windows)]
    fn windows_rejects_native_style() {
        // Native Windows paths rejected (must use internal format)
        assert!(normalize_local_path("C:\\foo").is_err());
        assert!(normalize_local_path("D:\\Projects").is_err());
    }

    // ═══════════════════════════════════════════════════════
    // is_local_root
    // ═══════════════════════════════════════════════════════

    #[test]
    fn root_check_empty_and_slash() {
        assert!(is_local_root(""));
        assert!(is_local_root("/"));
        assert!(is_local_root("  "));
        assert!(!is_local_root("/foo"));
    }

    #[test]
    #[cfg(windows)]
    fn windows_root_check_drives() {
        assert!(is_local_root("/c"));
        assert!(is_local_root("/d"));
        assert!(is_local_root("/z"));
        assert!(!is_local_root("/c/Users"));
        assert!(!is_local_root("/ab")); // two letters
        assert!(!is_local_root("/1")); // digit
        assert!(!is_local_root("/C")); // uppercase (internal format is always lowercase)
    }

    #[test]
    #[cfg(unix)]
    fn unix_root_check_only_slash() {
        assert!(is_local_root("/"));
        assert!(is_local_root(""));
        assert!(!is_local_root("/c"));
    }

    // ═══════════════════════════════════════════════════════
    // list_roots
    // ═══════════════════════════════════════════════════════

    #[test]
    #[cfg(unix)]
    fn list_roots_unix_is_slash() {
        assert_eq!(list_roots(), vec!["/".to_string()]);
    }

    #[test]
    fn current_info_not_empty() {
        let info = LocalPathInfo::current();
        assert!(!info.roots.is_empty());
    }

    #[test]
    #[cfg(windows)]
    fn windows_roots_start_with_slash() {
        for root in list_roots() {
            assert!(root.starts_with('/'), "root must start with /: {root}");
            assert_eq!(root.len(), 2, "root must be /<letter>: {root}");
            assert!(
                root.as_bytes()[1].is_ascii_lowercase(),
                "drive must be lowercase: {root}"
            );
        }
    }

    // ═══════════════════════════════════════════════════════
    // internal_to_native
    // ═══════════════════════════════════════════════════════

    #[test]
    #[cfg(windows)]
    fn internal_to_native_drives() {
        assert_eq!(internal_to_native("/c"), "C:\\");
        assert_eq!(internal_to_native("/c/"), "C:\\");
        assert_eq!(internal_to_native("/c/Users/foo"), "C:\\Users\\foo");
        assert_eq!(internal_to_native("/d/Projects"), "D:\\Projects");
        assert_eq!(internal_to_native("/"), "");
    }

    #[test]
    #[cfg(windows)]
    fn internal_to_native_deep_path() {
        assert_eq!(
            internal_to_native("/c/Users/william/Documents/project/src"),
            "C:\\Users\\william\\Documents\\project\\src"
        );
    }

    #[test]
    #[cfg(unix)]
    fn internal_to_native_is_identity_on_unix() {
        assert_eq!(internal_to_native("/foo/bar"), "/foo/bar");
        assert_eq!(internal_to_native("/"), "/");
        assert_eq!(internal_to_native("/home/user/docs"), "/home/user/docs");
    }

    // ═══════════════════════════════════════════════════════
    // native_to_internal
    // ═══════════════════════════════════════════════════════

    #[test]
    #[cfg(windows)]
    fn native_to_internal_drives() {
        assert_eq!(native_to_internal("C:\\"), "/c");
        assert_eq!(native_to_internal("C:\\Users\\foo"), "/c/Users/foo");
        assert_eq!(native_to_internal("D:/Projects"), "/d/Projects");
        assert_eq!(native_to_internal("C:"), "/c");
    }

    #[test]
    #[cfg(windows)]
    fn native_to_internal_mixed_separators() {
        assert_eq!(
            native_to_internal("C:\\Users/william\\Documents"),
            "/c/Users/william/Documents"
        );
        assert_eq!(native_to_internal("D:/Projects/test\\src"), "/d/Projects/test/src");
    }

    #[test]
    #[cfg(windows)]
    fn native_to_internal_trailing_slash() {
        assert_eq!(native_to_internal("C:\\Users\\foo\\"), "/c/Users/foo");
        assert_eq!(native_to_internal("D:/"), "/d");
    }

    #[test]
    #[cfg(windows)]
    fn native_to_internal_no_drive_letter() {
        // UNC paths or device paths — just flip separators
        assert_eq!(native_to_internal("\\\\server\\share\\path"), "//server/share/path");
        assert_eq!(native_to_internal("\\\\.\\pipe\\tokimo"), "//./pipe/tokimo");
    }

    #[test]
    #[cfg(unix)]
    fn native_to_internal_is_identity_on_unix() {
        assert_eq!(native_to_internal("/foo/bar"), "/foo/bar");
        assert_eq!(native_to_internal("/"), "/");
        assert_eq!(native_to_internal("/home/user/docs"), "/home/user/docs");
    }

    // ═══════════════════════════════════════════════════════
    // roundtrip: internal → native → internal
    // ═══════════════════════════════════════════════════════

    #[test]
    #[cfg(unix)]
    fn roundtrip() {
        let cases = vec!["/foo/bar", "/foo", "/a/b/c/d/e"];
        for case in cases {
            assert_eq!(
                native_to_internal(&internal_to_native(case)),
                case,
                "roundtrip failed for: {case}"
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn roundtrip_root() {
        assert_eq!(native_to_internal(&internal_to_native("/")), "/");
    }

    #[test]
    #[cfg(windows)]
    fn roundtrip_windows_drives() {
        let cases = vec!["/c", "/c/Users/william", "/d/Projects/foo", "/z/a/b/c"];
        for case in cases {
            assert_eq!(
                native_to_internal(&internal_to_native(case)),
                case,
                "roundtrip failed for: {case}"
            );
        }
    }

    /// 验证 `normalize_local_path` 的输出 → `internal_to_native` 的链路。
    /// 所有通过 normalize 的路径都应能安全转换。
    #[test]
    #[cfg(unix)]
    fn normalize_then_convert_roundtrip() {
        let cases = vec![
            "/foo/bar",
            "/foo",
            "/a/b/c",
            "/foo/./bar",   // dot component
            "/foo/bar/",    // trailing slash
            "//foo//bar",   // double slashes
            "/foo/.../bar", // three dots (valid filename)
        ];
        for case in cases {
            let normalized = normalize_local_path(case).unwrap();
            let native = internal_to_native(&normalized);
            let back = native_to_internal(&native);
            assert_eq!(back, normalized, "chain failed for: {case}");
        }
    }

    #[test]
    #[cfg(windows)]
    fn normalize_then_convert_roundtrip_windows() {
        let cases = vec![
            "/c/Users/foo",
            "/C/Users/foo", // uppercase drive → normalized to lowercase
            "/d/Projects",
            "/c/Users/foo/",    // trailing slash
            "//c//Users//foo",  // double slashes
            "/./c/./Users/./.", // dot components
        ];
        for case in cases {
            let normalized = normalize_local_path(case).unwrap();
            let native = internal_to_native(&normalized);
            let back = native_to_internal(&native);
            assert_eq!(
                back, normalized,
                "chain failed for: {case} → normalized={normalized} → native={native} → back={back}"
            );
        }
    }
}

// ── Sandbox guest path helpers ──────────────────────────
//
// AI sandbox runs Linux semantics (paths like /data/...) but the host server
// may run Windows. `std::path::Path::is_absolute()` is host-OS-specific and
// returns false for POSIX paths on Windows hosts, which would cause absolute
// guest paths to be wrongly concatenated to the workspace prefix.
//
// `is_guest_absolute` is OS-independent and recognizes everything an LLM
// might emit as "absolute":
//   * POSIX:        /home/foo
//   * UNC / root:   \\server\share, \foo
//   * Windows drive: C:\foo, c:/foo, F:/Users
//
// Windows-style paths inside a Linux sandbox don't physically exist; treating
// them as absolute means the sandbox stat will return "not found" — the
// correct behavior (LLM gets accurate feedback rather than a silently
// rebased path).

/// True if `path` looks like an absolute path under any common convention.
pub fn is_guest_absolute(path: &str) -> bool {
    if path.starts_with('/') || path.starts_with('\\') {
        return true;
    }
    let b = path.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Resolve a user-supplied path against a sandbox workspace.
///
/// Absolute paths (any convention) are returned untouched. Relative paths
/// are joined under `workspace_guest` with a single `/` separator.
pub fn resolve_under_guest(workspace_guest: &str, path: &str) -> String {
    if is_guest_absolute(path) {
        return path.to_string();
    }
    let trimmed = workspace_guest.trim_end_matches('/');
    format!("{trimmed}/{path}")
}

#[cfg(test)]
mod sandbox_path_tests {
    use super::*;

    #[test]
    fn posix_absolute() {
        assert!(is_guest_absolute("/home/foo"));
        assert!(is_guest_absolute("/"));
    }

    #[test]
    fn windows_drive_absolute() {
        assert!(is_guest_absolute("C:\\foo"));
        assert!(is_guest_absolute("c:/foo"));
        assert!(is_guest_absolute("F:/Users"));
        assert!(is_guest_absolute("z:"));
    }

    #[test]
    fn unc_and_root_absolute() {
        assert!(is_guest_absolute("\\\\server\\share"));
        assert!(is_guest_absolute("\\foo"));
    }

    #[test]
    fn relative_is_not_absolute() {
        assert!(!is_guest_absolute("src/main.rs"));
        assert!(!is_guest_absolute("foo"));
        assert!(!is_guest_absolute(""));
        assert!(!is_guest_absolute("1:bad"));
        assert!(!is_guest_absolute("ab:bad"));
    }

    #[test]
    fn resolve_keeps_absolute() {
        assert_eq!(resolve_under_guest("/work", "/etc/foo"), "/etc/foo");
        assert_eq!(resolve_under_guest("/work", "C:\\foo"), "C:\\foo");
    }

    #[test]
    fn resolve_joins_relative() {
        assert_eq!(resolve_under_guest("/work", "src/main.rs"), "/work/src/main.rs");
        assert_eq!(resolve_under_guest("/work/", "a.txt"), "/work/a.txt");
    }
}

