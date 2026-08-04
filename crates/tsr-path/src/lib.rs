//! TypeScript's path handling.
//!
//! Ported from `internal/tspath` at the pinned commit. The compiler cannot use
//! the platform's path type for this: TypeScript's paths are **always** forward
//! slashed, are compared under a case-insensitivity rule that comes from the host
//! rather than from the syntax, and describe a virtual file system as often as a
//! real one. `std::path::Path` answers a different question on every one of those
//! points.
//!
//! # The two representations
//!
//! A **file name** is what a user or an import specifier wrote: any slashes, any
//! case, possibly relative. A [`Path`] is what the compiler keys a map by:
//! normalised, absolute, and case-folded if the host is case-insensitive. The
//! conversion is [`to_path`], and it is the only one — everything the compiler
//! stores by file identity stores it under a [`Path`].
//!
//! Keeping them distinct in the type system is not upstream's design; upstream's
//! `Path` is a `string` newtype that nothing forces you to construct correctly.
//! It is the same newtype here, but the compiler will not let a `&str` be used as
//! one.

use std::fmt;

/// The directory separator TypeScript uses, on every platform.
pub const DIRECTORY_SEPARATOR: char = '/';

/// A file's canonical identity: normalised, absolute, and case-folded if the
/// host requires it.
///
/// Upstream's `tspath.Path`. Two file names denote the same file exactly when
/// their `Path`s are equal, which is what makes this the key type for every
/// file-addressed map in the compiler.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Path(String);

impl Path {
    /// The underlying string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A file name's canonical [`Path`] (`tspath.ToPath`).
///
/// `base_path` is the current directory a relative name is resolved against.
#[must_use]
pub fn to_path(file_name: &str, base_path: &str, use_case_sensitive_file_names: bool) -> Path {
    let non_canonical = if is_rooted_disk_path(file_name) {
        normalize_path(file_name)
    } else {
        get_normalized_absolute_path(file_name, base_path)
    };
    Path(get_canonical_file_name(&non_canonical, use_case_sensitive_file_names))
}

/// A file name folded for comparison (`tspath.GetCanonicalFileName`).
///
/// Upstream lowercases only ASCII (`ToFileNameLowerCase` is deliberately not
/// Unicode-aware, because a file system's case folding is not Unicode's), and so
/// does this.
#[must_use]
pub fn get_canonical_file_name(file_name: &str, use_case_sensitive_file_names: bool) -> String {
    if use_case_sensitive_file_names {
        file_name.to_string()
    } else {
        file_name.to_ascii_lowercase()
    }
}

/// Backslashes to forward slashes (`tspath.NormalizeSlashes`).
#[must_use]
pub fn normalize_slashes(path: &str) -> String {
    path.replace('\\', "/")
}

/// How many leading characters of `path` are its root (`tspath.GetRootLength`).
///
/// `/` on POSIX, `c:/` or `c:` on Windows, and the server-and-share prefix of a
/// UNC path. Zero for a relative path.
#[must_use]
pub fn get_root_length(path: &str) -> usize {
    let bytes = path.as_bytes();
    if bytes.first() == Some(&b'/') || bytes.first() == Some(&b'\\') {
        // A UNC path: `//server/share`. Both separators, because this runs
        // before normalisation.
        if bytes.get(1) != Some(&b'/') && bytes.get(1) != Some(&b'\\') {
            return 1;
        }
        let after_separators = &path[2..];
        let Some(server_end) = after_separators.find(['/', '\\']) else {
            return path.len();
        };
        let share = &after_separators[server_end + 1..];
        return match share.find(['/', '\\']) {
            Some(share_end) => 2 + server_end + 1 + share_end + 1,
            None => path.len(),
        };
    }
    // A DOS path: `c:/` — and `c:` alone, which is a *relative* path on that
    // drive, so its root is the two characters and no separator.
    if bytes.get(1) == Some(&b':') && bytes.first().is_some_and(u8::is_ascii_alphabetic) {
        return match bytes.get(2) {
            Some(b'/' | b'\\') => 3,
            _ => 2,
        };
    }
    0
}

/// Whether `path` begins at a root (`tspath.IsRootedDiskPath`).
#[must_use]
pub fn is_rooted_disk_path(path: &str) -> bool {
    get_root_length(path) > 0
}

/// Join path segments, normalising slashes (`tspath.CombinePaths`).
///
/// A rooted segment discards everything before it, which is what makes
/// `combine_paths("/a", "/b")` be `/b`.
#[must_use]
pub fn combine_paths(base: &str, parts: &[&str]) -> String {
    let mut result = normalize_slashes(base);
    for part in parts {
        if part.is_empty() {
            continue;
        }
        let part = normalize_slashes(part);
        if result.is_empty() || is_rooted_disk_path(&part) {
            result = part;
        } else {
            result = format!("{}/{part}", result.trim_end_matches('/'));
        }
    }
    result
}

/// Resolve `.` and `..` against `current_directory`
/// (`tspath.GetNormalizedAbsolutePath`).
///
/// Upstream's implementation is hand-optimised to allocate nothing when the path
/// is already normal, scanning segment by segment and only building a new string
/// once it finds something to change. This is the same function written plainly.
/// The difference is allocation, not result — and it is a deliberate deferral:
/// path normalisation has not appeared in a profile, and the optimised version is
/// 90 lines of index arithmetic that would need its own tests to trust. Revisit
/// if a profile says so.
#[must_use]
pub fn get_normalized_absolute_path(file_name: &str, current_directory: &str) -> String {
    let root_length = get_root_length(file_name);
    let combined = if root_length == 0 && !current_directory.is_empty() {
        combine_paths(current_directory, &[file_name])
    } else {
        normalize_slashes(file_name)
    };

    let root_length = get_root_length(&combined);
    let (root, rest) = combined.split_at(root_length);

    let mut segments: Vec<&str> = Vec::new();
    for segment in rest.split('/') {
        match segment {
            // A `.` or a doubled separator contributes nothing.
            "" | "." => {}
            ".." => {
                // `..` above the root stays: `../a` is a real relative path,
                // while `/../a` is just `/a`.
                if segments.last().is_some_and(|last| *last != "..") {
                    segments.pop();
                } else if root.is_empty() {
                    segments.push("..");
                }
            }
            other => segments.push(other),
        }
    }

    let joined = segments.join("/");
    if root.is_empty() {
        joined
    } else if joined.is_empty() {
        // The root alone keeps its trailing separator: `/` and `c:/`.
        ensure_trailing_directory_separator(root)
    } else {
        format!("{}{joined}", ensure_trailing_directory_separator(root))
    }
}

/// Normalise a path without making it absolute (`tspath.NormalizePath`).
#[must_use]
pub fn normalize_path(path: &str) -> String {
    let slashed = normalize_slashes(path);
    let normalized = get_normalized_absolute_path(&slashed, "");
    if !normalized.is_empty() && has_trailing_directory_separator(&slashed) {
        return ensure_trailing_directory_separator(&normalized);
    }
    normalized
}

/// Whether `path` ends with a separator.
#[must_use]
pub fn has_trailing_directory_separator(path: &str) -> bool {
    path.ends_with('/') || path.ends_with('\\')
}

/// `path` with exactly one trailing separator
/// (`tspath.EnsureTrailingDirectorySeparator`).
#[must_use]
pub fn ensure_trailing_directory_separator(path: &str) -> String {
    if has_trailing_directory_separator(path) { path.to_string() } else { format!("{path}/") }
}

/// `path` with any trailing separator removed, unless it is the root
/// (`tspath.RemoveTrailingDirectorySeparator`).
#[must_use]
pub fn remove_trailing_directory_separator(path: &str) -> String {
    let root_length = get_root_length(path);
    if path.len() > root_length && has_trailing_directory_separator(path) {
        return path[..path.len() - 1].to_string();
    }
    path.to_string()
}

/// The extension of a file name, or `""` (`tspath.GetAnyExtensionFromPath`).
///
/// `.d.ts` counts as one extension, which is the case that stops this being
/// `rsplit_once('.')`.
#[must_use]
pub fn get_any_extension_from_path(path: &str) -> &str {
    let base = get_base_file_name(path);
    for multi in [".d.ts", ".d.mts", ".d.cts"] {
        if base.len() > multi.len() && base.ends_with(multi) {
            return &base[base.len() - multi.len()..];
        }
    }
    match base.rfind('.') {
        // A leading dot is the whole name (`.gitignore`), not an extension.
        Some(dot) if dot > 0 => &base[dot..],
        _ => "",
    }
}

/// The last segment of a path (`tspath.GetBaseFileName`).
#[must_use]
pub fn get_base_file_name(path: &str) -> &str {
    let root_length = get_root_length(path);
    let trimmed = path[root_length..].trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(slash) => &trimmed[slash + 1..],
        None => trimmed,
    }
}

/// A path's directory (`tspath.GetDirectoryPath`).
#[must_use]
pub fn get_directory_path(path: &str) -> &str {
    let root_length = get_root_length(path);
    let trimmed = path.trim_end_matches('/');
    if trimmed.len() <= root_length {
        return &path[..root_length];
    }
    match trimmed.rfind('/') {
        Some(slash) if slash >= root_length => &trimmed[..slash.max(root_length)],
        _ => &path[..root_length],
    }
}

/// `path` with its extension removed (`tspath.RemoveFileExtension`).
#[must_use]
pub fn remove_file_extension(path: &str) -> &str {
    let extension = get_any_extension_from_path(path);
    if extension.is_empty() { path } else { &path[..path.len() - extension.len()] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_root_is_recognised_in_each_of_its_forms() {
        assert_eq!(get_root_length("/"), 1);
        assert_eq!(get_root_length("/a/b"), 1);
        assert_eq!(get_root_length("c:/"), 3);
        assert_eq!(get_root_length("c:\\a"), 3);
        // `c:` without a separator is relative *to that drive*, so the root is
        // the drive letter and nothing more.
        assert_eq!(get_root_length("c:"), 2);
        assert_eq!(get_root_length("//server/share/a"), "//server/share/".len());
        assert_eq!(get_root_length("a/b"), 0);
        assert_eq!(get_root_length(""), 0);
    }

    #[test]
    fn dot_segments_resolve() {
        assert_eq!(get_normalized_absolute_path("/a/./b", ""), "/a/b");
        assert_eq!(get_normalized_absolute_path("/a/b/../c", ""), "/a/c");
        assert_eq!(get_normalized_absolute_path("/a//b", ""), "/a/b");
        // `..` cannot climb above a root.
        assert_eq!(get_normalized_absolute_path("/../a", ""), "/a");
        // But it can in a relative path, where it means something.
        assert_eq!(get_normalized_absolute_path("../a", ""), "../a");
        assert_eq!(get_normalized_absolute_path("../../a", ""), "../../a");
    }

    #[test]
    fn a_relative_name_resolves_against_the_current_directory() {
        assert_eq!(get_normalized_absolute_path("b.ts", "/a"), "/a/b.ts");
        assert_eq!(get_normalized_absolute_path("./b.ts", "/a"), "/a/b.ts");
        assert_eq!(get_normalized_absolute_path("../b.ts", "/a/c"), "/a/b.ts");
    }

    #[test]
    fn a_root_keeps_its_separator_and_nothing_else_does() {
        assert_eq!(get_normalized_absolute_path("/", ""), "/");
        assert_eq!(get_normalized_absolute_path("c:/", ""), "c:/");
        assert_eq!(normalize_path("/a/b/"), "/a/b/");
        assert_eq!(normalize_path("/a/b"), "/a/b");
    }

    #[test]
    fn a_path_is_case_folded_only_when_the_host_is_insensitive() {
        assert_eq!(to_path("/A/B.ts", "", true).as_str(), "/A/B.ts");
        assert_eq!(to_path("/A/B.ts", "", false).as_str(), "/a/b.ts");
        // Two spellings of one file are one `Path`.
        assert_eq!(to_path("./b.ts", "/a", false), to_path("/a/B.TS", "", false));
    }

    #[test]
    fn backslashes_are_normalised_before_anything_else() {
        assert_eq!(to_path("c:\\a\\b.ts", "", true).as_str(), "c:/a/b.ts");
    }

    #[test]
    fn combining_takes_the_last_rooted_part() {
        assert_eq!(combine_paths("/a", &["b"]), "/a/b");
        assert_eq!(combine_paths("/a", &["/b"]), "/b");
        assert_eq!(combine_paths("/a/", &["b"]), "/a/b");
        assert_eq!(combine_paths("", &["b"]), "b");
    }

    #[test]
    fn a_declaration_file_has_one_extension_not_two() {
        assert_eq!(get_any_extension_from_path("a.d.ts"), ".d.ts");
        assert_eq!(get_any_extension_from_path("a.ts"), ".ts");
        assert_eq!(get_any_extension_from_path("a.b.ts"), ".ts");
        assert_eq!(get_any_extension_from_path(".gitignore"), "");
        assert_eq!(remove_file_extension("/a/b.d.ts"), "/a/b");
        assert_eq!(remove_file_extension("/a/b.ts"), "/a/b");
    }

    #[test]
    fn base_names_and_directories() {
        assert_eq!(get_base_file_name("/a/b.ts"), "b.ts");
        assert_eq!(get_base_file_name("/a/b/"), "b");
        assert_eq!(get_base_file_name("/"), "");
        assert_eq!(get_directory_path("/a/b.ts"), "/a");
        assert_eq!(get_directory_path("/a"), "/");
        assert_eq!(get_directory_path("/"), "/");
    }
}
