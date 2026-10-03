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

pub mod extension;

pub use extension::{
    change_extension, extension_is_one_of, extension_is_ts, file_extension_is,
    file_extension_is_one_of, get_declaration_file_extension,
    get_possible_original_input_extension_for_extension, has_implementation_ts_file_extension,
    is_declaration_file_name, remove_extension, remove_file_extension, try_extract_ts_extension,
    try_get_extension_from_path,
};

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

/// How many leading bytes of `path` are its root (`tspath.GetRootLength`).
///
/// Includes POSIX, DOS, UNC server, URL and untitled roots. Zero for a relative
/// path, including a drive-relative name such as `c:d`.
#[must_use]
pub fn get_root_length(path: &str) -> usize {
    get_root_length_and_is_url(path).0
}

/// The two components of native `GetEncodedRootLength`: root length and whether
/// it is a URL. Keep URL roots distinct from absolute disk paths.
fn get_root_length_and_is_url(path: &str) -> (usize, bool) {
    let bytes = path.as_bytes();
    let Some(&first) = bytes.first() else {
        return (0, false);
    };
    if matches!(first, b'/' | b'\\') {
        if bytes.get(1) != Some(&first) {
            return (1, false);
        }
        let length =
            bytes[2..].iter().position(|&byte| byte == first).map_or(path.len(), |index| index + 3);
        return (length, false);
    }
    if first.is_ascii_alphabetic() && bytes.get(1) == Some(&b':') {
        if path.len() == 2 {
            return (2, false);
        }
        if matches!(bytes.get(2), Some(b'/' | b'\\')) {
            return (3, false);
        }
    }
    if first == b'^' && bytes.get(1) == Some(&b'/') {
        return (2, false);
    }
    if let Some(scheme_end) = path.find("://") {
        let authority_start = scheme_end + 3;
        let Some(authority_length) = path[authority_start..].find('/') else {
            return (path.len(), true);
        };
        let authority_end = authority_start + authority_length;
        let authority = &path[authority_start..authority_end];
        if &path[..scheme_end] == "file"
            && matches!(authority, "" | "localhost")
            && bytes.get(authority_end + 1).is_some_and(u8::is_ascii_alphabetic)
        {
            let start = authority_end + 2;
            let volume_end = if bytes.get(start) == Some(&b':') {
                Some(start + 1)
            } else if bytes.get(start) == Some(&b'%')
                && bytes.get(start + 1) == Some(&b'3')
                && matches!(bytes.get(start + 2), Some(b'a' | b'A'))
            {
                Some(start + 3)
            } else {
                None
            };
            if let Some(end) = volume_end {
                if end == path.len() {
                    return (end, true);
                }
                if bytes.get(end) == Some(&b'/') {
                    return (end + 1, true);
                }
            }
        }
        return (authority_end + 1, true);
    }
    (0, false)
}

/// Whether `path` begins at a root (`tspath.IsRootedDiskPath`).
#[must_use]
pub fn is_rooted_disk_path(path: &str) -> bool {
    let (length, is_url) = get_root_length_and_is_url(path);
    length > 0 && !is_url
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
        if result.is_empty() || get_root_length(&part) != 0 {
            result = part;
        } else {
            if !has_trailing_directory_separator(&result) {
                result.push('/');
            }
            result.push_str(&part);
        }
    }
    result
}

/// Resolve `.` and `..` against `current_directory`
/// (`tspath.GetNormalizedAbsolutePath`).
///
/// Use the native `simpleNormalizePath` fast path before collecting segments.
/// An already-normal path only needs the owned result required by this API.
#[must_use]
pub fn get_normalized_absolute_path(file_name: &str, current_directory: &str) -> String {
    let root_length = get_root_length(file_name);
    let mut combined = if root_length == 0 && !current_directory.is_empty() {
        combine_paths(current_directory, &[file_name])
    } else {
        normalize_slashes(file_name)
    };

    let root_length = get_root_length(&combined);
    if simple_normalize_path(&mut combined) {
        if combined.len() > root_length && combined.ends_with('/') {
            combined.pop();
        } else if combined.len() == root_length && root_length != 0 && !combined.ends_with('/') {
            combined.push('/');
        }
        return combined;
    }
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

/// Native `simpleNormalizePath`: preserve the path when no work is needed,
/// otherwise try `/./` and leading `./` cleanup before the general fallback.
fn simple_normalize_path(path: &mut String) -> bool {
    if !has_relative_path_segment(path) {
        return true;
    }
    let simplified = path.replace("/./", "/");
    let trimmed = simplified.strip_prefix("./").unwrap_or(&simplified);
    if trimmed != path
        && !has_relative_path_segment(trimmed)
        && !(trimmed != simplified && trimmed.starts_with('/'))
    {
        *path = trimmed.to_string();
        return true;
    }
    false
}

/// Native `hasRelativePathSegment`. Byte scanning is safe for UTF-8: only ASCII
/// separators and whole dot segments affect normalization.
fn has_relative_path_segment(path: &str) -> bool {
    let mut previous_slash = false;
    let mut segment_length = 0;
    let mut only_dots = true;
    for byte in path.bytes() {
        if byte == b'/' {
            if previous_slash || (only_dots && matches!(segment_length, 1 | 2)) {
                return true;
            }
            previous_slash = true;
            segment_length = 0;
            only_dots = true;
        } else {
            only_dots &= byte == b'.';
            segment_length += 1;
            previous_slash = false;
        }
    }
    only_dots && matches!(segment_length, 1 | 2)
}

/// Normalise a path without making it absolute (`tspath.NormalizePath`).
#[must_use]
pub fn normalize_path(path: &str) -> String {
    let mut slashed = normalize_slashes(path);
    if simple_normalize_path(&mut slashed) {
        return slashed;
    }
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

/// `path` with one trailing separator removed
/// (`tspath.RemoveTrailingDirectorySeparator`).
///
/// **The root is not special-cased**: `"/"` becomes `""`, exactly as upstream's
/// does. An earlier version here kept the root, which reads as the safer
/// choice and is not: glob compilation strips the separator from a pattern's
/// first component precisely so it compares equal to the *empty* leading
/// component a rooted path yields, and `"/" != ""` made every rooted include
/// pattern match nothing.
#[must_use]
pub fn remove_trailing_directory_separator(path: &str) -> String {
    if has_trailing_directory_separator(path) {
        return path[..path.len() - 1].to_string();
    }
    path.to_string()
}

/// The extension of a file name, or `""` (`tspath.GetAnyExtensionFromPath`).
///
/// **Lexical**: everything from the last dot in the base name, so `a.d.ts` yields
/// `.ts` and not `.d.ts`. That is upstream's behaviour and it is deliberate — this
/// is the answer to "what did the user write", while
/// [`extension::try_get_extension_from_path`] answers "what does TypeScript
/// recognise". The resolver needs both and uses them in different places.
#[must_use]
pub fn get_any_extension_from_path(path: &str) -> &str {
    let base = get_base_file_name(path);
    match base.rfind('.') {
        // A leading dot is the whole name (`.gitignore`), not an extension.
        Some(dot) if dot > 0 => &base[dot..],
        _ => "",
    }
}

/// The first of `extensions` that `path` ends with, or `""`
/// (`tspath.GetAnyExtensionFromPath` with an explicit extension list).
///
/// Case-sensitive, which is the only form the resolver asks for.
#[must_use]
pub fn get_any_extension_from_path_with<'a>(path: &'a str, extensions: &[&str]) -> &'a str {
    let root_length = get_root_length(path);
    let trimmed = if path.len() > root_length && has_trailing_directory_separator(path) {
        &path[..path.len() - 1]
    } else {
        path
    };
    for extension in extensions {
        // Every caller passes dot-prefixed extensions; upstream tolerates both.
        debug_assert!(extension.starts_with('.'));
        // `>=` rather than `>`: upstream returns `.ts` for the path `.ts` itself.
        if trimmed.len() >= extension.len() && trimmed.ends_with(extension) {
            return &trimmed[trimmed.len() - extension.len()..];
        }
    }
    ""
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

/// Whether the file name has an extension at all (`tspath.HasExtension`).
///
/// Purely lexical, and deliberately so: `a.b/c` has none and `.gitignore` has
/// one. Module resolution, the file loader, and `include` glob compilation all
/// branch on it before deciding whether a name needs an extension appended.
#[must_use]
pub fn has_extension(file_name: &str) -> bool {
    get_base_file_name(file_name).contains('.')
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

/// A path split into its root and segments (`tspath.GetPathComponents`).
///
/// The first element is the root — `""` for a relative path — and the rest are
/// the segments, with a trailing empty segment dropped. That first-element
/// convention is upstream's and every consumer below relies on it.
#[must_use]
pub fn get_path_components(path: &str, current_directory: &str) -> Vec<String> {
    let path = combine_paths(current_directory, &[path]);
    let root_length = get_root_length(&path);
    let (root, rest) = path.split_at(root_length);
    let mut components = vec![root.to_string()];
    let mut segments: Vec<&str> = rest.split('/').collect();
    if segments.last().is_some_and(|last| last.is_empty()) {
        segments.pop();
    }
    components.extend(segments.iter().map(|s| (*s).to_string()));
    components
}

/// Resolve `.` and `..` within already-split components
/// (`tspath.reducePathComponents`).
#[must_use]
pub fn reduce_path_components(components: &[String]) -> Vec<String> {
    if components.is_empty() {
        return Vec::new();
    }
    let mut reduced = vec![components[0].clone()];
    for component in &components[1..] {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            if reduced.len() > 1 {
                if reduced[reduced.len() - 1] != ".." {
                    reduced.pop();
                    continue;
                }
            } else if !reduced[0].is_empty() {
                // `..` cannot climb above a root, but *can* above nothing.
                continue;
            }
        }
        reduced.push(component.clone());
    }
    reduced
}

/// Rejoin components produced by [`get_path_components`]
/// (`tspath.GetPathFromPathComponents`).
#[must_use]
pub fn get_path_from_path_components(components: &[String]) -> String {
    if components.is_empty() {
        return String::new();
    }
    let root = ensure_trailing_directory_separator_if_nonempty(&components[0]);
    format!("{root}{}", components[1..].join("/"))
}

fn ensure_trailing_directory_separator_if_nonempty(path: &str) -> String {
    if path.is_empty() { String::new() } else { ensure_trailing_directory_separator(path) }
}

/// Whether `path` starts with `.` or `..` as a segment (`tspath.PathIsRelative`).
#[must_use]
pub fn path_is_relative(path: &str) -> bool {
    if path == "." || path == ".." {
        return true;
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'.' && matches!(bytes[1], b'/' | b'\\') {
        return true;
    }
    bytes.len() >= 3 && bytes[0] == b'.' && bytes[1] == b'.' && matches!(bytes[2], b'/' | b'\\')
}

/// Whether a module specifier names a file rather than a package
/// (`tspath.IsExternalModuleNameRelative`).
///
/// A rooted path counts as "relative" here, which reads oddly and is upstream's:
/// what the flag really means is "do not look this up in `node_modules`".
#[must_use]
pub fn is_external_module_name_relative(module_name: &str) -> bool {
    path_is_relative(module_name) || is_rooted_disk_path(module_name)
}

/// How paths are compared: by the host's case rule, against a base directory.
///
/// Upstream's `tspath.ComparePathsOptions`.
#[derive(Debug, Clone, Default)]
pub struct ComparePathsOptions {
    /// Whether the host distinguishes `A.ts` from `a.ts`.
    pub use_case_sensitive_file_names: bool,
    /// What a relative path is resolved against. May be empty.
    pub current_directory: String,
}

impl ComparePathsOptions {
    fn equate(&self, a: &str, b: &str) -> bool {
        if self.use_case_sensitive_file_names { a == b } else { a.eq_ignore_ascii_case(b) }
    }

    fn compare(&self, a: &str, b: &str) -> std::cmp::Ordering {
        if self.use_case_sensitive_file_names {
            a.cmp(b)
        } else {
            // Case-insensitive first, then case-sensitive as the tiebreak, so the
            // ordering stays total. Upstream's `CompareStringsCaseInsensitive`
            // lowercases both sides and compares.
            a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()).then_with(|| a.cmp(b))
        }
    }
}

/// Order two paths (`tspath.ComparePaths`).
///
/// Roots are always compared case-insensitively — a drive letter's case is never
/// meaningful — and the rest by the host's rule.
#[must_use]
pub fn compare_paths(a: &str, b: &str, options: &ComparePathsOptions) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let a = combine_paths(&options.current_directory, &[a]);
    let b = combine_paths(&options.current_directory, &[b]);
    if a == b {
        return Ordering::Equal;
    }
    if a.is_empty() {
        return Ordering::Less;
    }
    if b.is_empty() {
        return Ordering::Greater;
    }

    let a_root = &a[..get_root_length(&a)];
    let b_root = &b[..get_root_length(&b)];
    let root_order = a_root.to_ascii_lowercase().cmp(&b_root.to_ascii_lowercase());
    if root_order != Ordering::Equal {
        return root_order;
    }

    let a_components = reduce_path_components(&get_path_components(&a, ""));
    let b_components = reduce_path_components(&get_path_components(&b, ""));
    let shared = a_components.len().min(b_components.len());
    for i in 1..shared {
        let order = options.compare(&a_components[i], &b_components[i]);
        if order != Ordering::Equal {
            return order;
        }
    }
    a_components.len().cmp(&b_components.len())
}

/// Whether `child` is `parent` or lies beneath it (`tspath.ContainsPath`).
#[must_use]
pub fn contains_path(parent: &str, child: &str, options: &ComparePathsOptions) -> bool {
    let parent = combine_paths(&options.current_directory, &[parent]);
    let child = combine_paths(&options.current_directory, &[child]);
    if parent.is_empty() || child.is_empty() {
        return false;
    }
    if parent == child {
        return true;
    }
    let parent_components = reduce_path_components(&get_path_components(&parent, ""));
    let child_components = reduce_path_components(&get_path_components(&child, ""));
    if child_components.len() < parent_components.len() {
        return false;
    }
    for (i, parent_component) in parent_components.iter().enumerate() {
        // The root, again, is always case-insensitive.
        let equal = if i == 0 {
            parent_component.eq_ignore_ascii_case(&child_components[i])
        } else {
            options.equate(parent_component, &child_components[i])
        };
        if !equal {
            return false;
        }
    }
    true
}

/// The components of `to` relative to `from`
/// (`tspath.GetPathComponentsRelativeTo`).
#[must_use]
pub fn get_path_components_relative_to(
    from: &str,
    to: &str,
    options: &ComparePathsOptions,
) -> Vec<String> {
    let from_components =
        reduce_path_components(&get_path_components(from, &options.current_directory));
    let to_components =
        reduce_path_components(&get_path_components(to, &options.current_directory));

    let max_common = from_components.len().min(to_components.len());
    let mut start = 0;
    while start < max_common {
        let equal = if start == 0 {
            from_components[start].eq_ignore_ascii_case(&to_components[start])
        } else {
            options.equate(&from_components[start], &to_components[start])
        };
        if !equal {
            break;
        }
        start += 1;
    }

    if start == 0 {
        return to_components;
    }

    let mut result = vec![String::new()];
    result.extend(std::iter::repeat_n("..".to_string(), from_components.len() - start));
    result.extend_from_slice(&to_components[start..]);
    result
}

/// `to` written relative to the directory `from_directory`
/// (`tspath.GetRelativePathFromDirectory`).
///
/// Both must be absolute or both relative; a mismatch is a caller bug upstream
/// panics on, and returning the absolute `to` instead would silently produce a
/// wrong module specifier.
///
/// # Panics
///
/// If one path is rooted and the other is not.
#[must_use]
pub fn get_relative_path_from_directory(
    from_directory: &str,
    to: &str,
    options: &ComparePathsOptions,
) -> String {
    assert_eq!(
        get_root_length(from_directory) > 0,
        get_root_length(to) > 0,
        "paths must either both be absolute or both be relative"
    );
    get_path_from_path_components(&get_path_components_relative_to(from_directory, to, options))
}

/// A path as a diagnostic should print it (`tspath.ConvertToRelativePath`).
///
/// `internal/tspath/path.go:785`. An absolute path becomes relative to
/// `options.current_directory`; anything already relative is returned untouched.
/// This is why `tsc` prints `src/a.ts(1,1)` rather than the absolute path it
/// resolved internally, and why the same compilation run from two directories
/// produces two different-looking error logs.
///
/// Deliberately **not** routed through [`get_relative_path_from_directory`],
/// whose assertion that both paths are rooted-or-neither would fire here:
/// upstream reaches [`get_path_components_relative_to`] directly
/// (`GetRelativePathToDirectoryOrUrl`, `:793`), and an empty current directory
/// with an absolute file is a legitimate configuration.
#[must_use]
pub fn convert_to_relative_path(path: &str, options: &ComparePathsOptions) -> String {
    if !is_rooted_disk_path(path) {
        return path.to_string();
    }
    get_path_from_path_components(&get_path_components_relative_to(
        &options.current_directory,
        path,
        options,
    ))
}

/// Call `f` on `directory` and each ancestor until it returns a value
/// (`tspath.ForEachAncestorDirectory`).
///
/// Stops at the root, which is the directory whose parent is itself.
pub fn for_each_ancestor_directory<T>(
    directory: &str,
    mut f: impl FnMut(&str) -> Option<T>,
) -> Option<T> {
    let mut directory = directory.to_string();
    loop {
        if let Some(result) = f(&directory) {
            return Some(result);
        }
        let parent = get_directory_path(&directory).to_string();
        if parent == directory {
            return None;
        }
        directory = parent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected results from the pinned tspath path_test.go, including root
    // forms whose normalization differs from a platform filesystem path.
    #[test]
    fn native_root_forms() {
        let cases = [
            ("a", 0),
            ("/", 1),
            ("/path", 1),
            ("c:", 2),
            ("c:d", 0),
            ("c:/", 3),
            ("c:\\", 3),
            ("//server", 8),
            ("//server/share", 9),
            ("\\\\server", 8),
            ("\\\\server\\share", 9),
            ("file:///", 8),
            ("file:///path", 8),
            ("file:///c:", 10),
            ("file:///c:d", 8),
            ("file:///c:/path", 11),
            ("file:///c%3a", 12),
            ("file:///c%3ad", 8),
            ("file:///c%3a/path", 13),
            ("file:///c%3A", 12),
            ("file:///c%3Ad", 8),
            ("file:///c%3A/path", 13),
            ("file://localhost", 16),
            ("file://localhost/", 17),
            ("file://localhost/path", 17),
            ("file://localhost/c:", 19),
            ("file://localhost/c:d", 17),
            ("file://localhost/c:/path", 20),
            ("file://localhost/c%3a", 21),
            ("file://localhost/c%3ad", 17),
            ("file://localhost/c%3a/path", 22),
            ("file://localhost/c%3A", 21),
            ("file://localhost/c%3Ad", 17),
            ("file://localhost/c%3A/path", 22),
            ("file://server", 13),
            ("file://server/", 14),
            ("file://server/path", 14),
            ("file://server/c:", 14),
            ("file://server/c:d", 14),
            ("file://server/c:/d", 14),
            ("file://server/c%3a", 14),
            ("file://server/c%3ad", 14),
            ("file://server/c%3a/d", 14),
            ("file://server/c%3A", 14),
            ("file://server/c%3Ad", 14),
            ("file://server/c%3A/d", 14),
            ("http://server", 13),
            ("http://server/path", 14),
            ("^/untitled/a", 2),
            ("//日本/share/a", 9),
            ("/\\a", 1),
            ("\\/a", 1),
        ];
        for (path, expected) in cases {
            assert_eq!(get_root_length(path), expected, "{path:?}");
        }
        assert!(is_rooted_disk_path("c:"));
        assert!(!is_rooted_disk_path("c:d"));
        assert!(!is_rooted_disk_path("file:///c:/a"));
        assert!(!is_rooted_disk_path("http://server/a"));
        assert!(is_rooted_disk_path("^/untitled/a"));
    }

    #[test]
    fn native_absolute_normalization_controls() {
        let cases = [
            ("/", "", "/"),
            ("/.", "", "/"),
            ("/./", "", "/"),
            ("/../", "", "/"),
            ("/a", "", "/a"),
            ("/a/", "", "/a"),
            ("/a/.", "", "/a"),
            ("/a/foo.", "", "/a/foo."),
            ("/a/./", "", "/a"),
            ("/a/./b", "", "/a/b"),
            ("/a/./b/", "", "/a/b"),
            ("/a/..", "", "/"),
            ("/a/../", "", "/"),
            ("/a/../", "", "/"),
            ("/a/../b", "", "/b"),
            ("/a/../b/", "", "/b"),
            ("/a/..", "", "/"),
            ("/a/..", "/", "/"),
            ("/a/..", "b/", "/"),
            ("/a/..", "/b", "/"),
            ("/a/.", "b", "/a"),
            ("/a/.", ".", "/a"),
            ("\\", "", "/"),
            ("\\.", "", "/"),
            ("\\.\\", "", "/"),
            ("\\..\\", "", "/"),
            ("\\a\\.\\", "", "/a"),
            ("\\a\\.\\b", "", "/a/b"),
            ("\\a\\.\\b\\", "", "/a/b"),
            ("\\a\\..", "", "/"),
            ("\\a\\..\\", "", "/"),
            ("\\a\\..\\", "", "/"),
            ("\\a\\..\\b", "", "/b"),
            ("\\a\\..\\b\\", "", "/b"),
            ("\\a\\..", "", "/"),
            ("\\a\\..", "\\", "/"),
            ("\\a\\..", "b\\", "/"),
            ("\\a\\..", "\\b", "/"),
            ("\\a\\.", "b", "/a"),
            ("\\a\\.", ".", "/a"),
            ("", "", ""),
            (".", "", ""),
            ("./", "", ""),
            ("..", "", ".."),
            ("../", "", ".."),
            ("", "/home", "/home"),
            (".", "/home", "/home"),
            ("./", "/home", "/home"),
            ("..", "/home", "/"),
            ("../", "/home", "/"),
            ("a", "b", "b/a"),
            ("a", "b/c", "b/c/a"),
            (".a", "", ".a"),
            ("..a", "", "..a"),
            ("a.", "", "a."),
            ("a..", "", "a.."),
            ("/base/./.a", "", "/base/.a"),
            ("/base/../.a", "", "/.a"),
            ("/base/./..a", "", "/base/..a"),
            ("/base/../..a", "", "/..a"),
            ("/base/./..a/b", "", "/base/..a/b"),
            ("/base/../..a/b", "", "/..a/b"),
            ("/base/./a.", "", "/base/a."),
            ("/base/../a.", "", "/a."),
            ("/base/./a..", "", "/base/a.."),
            ("/base/../a..", "", "/a.."),
            ("/base/./a../b", "", "/base/a../b"),
            ("/base/../a../b", "", "/a../b"),
            ("a/..", "", ""),
            ("/a//", "", "/a"),
            ("//a", "a", "//a/"),
            ("/\\", "", "//"),
            ("a///", "a", "a/a"),
            ("/.//", "", "/"),
            ("//\\\\", "", "///"),
            (".//a", ".", "a"),
            ("a/../..", "", ".."),
            ("../..", "\\a", "/"),
            ("a:", "b", "a:/"),
            ("a/../..", "..", "../.."),
            ("a/../..", "b", ""),
            ("a//../..", "..", "../.."),
            ("a//b", "", "a/b"),
            ("a///b", "", "a/b"),
            ("a/b//c", "", "a/b/c"),
            ("/a/b//c", "", "/a/b/c"),
            ("//a/b//c", "", "//a/b/c"),
            ("a\\\\b", "", "a/b"),
            ("a\\\\\\b", "", "a/b"),
            ("a\\b\\\\c", "", "a/b/c"),
            ("\\a\\b\\\\c", "", "/a/b/c"),
            ("\\\\a\\b\\\\c", "", "//a/b/c"),
            ("a/\\b", "", "a/b"),
            ("a\\/b", "", "a/b"),
            ("a\\/\\b", "", "a/b"),
            ("a\\b//c", "", "a/b/c"),
            ("\\a\\b\\\\c", "", "/a/b/c"),
            ("\\\\a\\b\\\\c", "", "//a/b/c"),
            ("c:x", "/base", "/base/c:x"),
            ("//server/share/../a", "", "//server/a"),
            ("file:///c:/../a", "", "file:///c:/a"),
            ("http://server/a/../../b", "", "http://server/b"),
            ("日本/../é", "/base", "/base/é"),
        ];
        for (path, directory, expected) in cases {
            assert_eq!(
                get_normalized_absolute_path(path, directory),
                expected,
                "path={path:?}, directory={directory:?}"
            );
        }
    }

    #[test]
    fn native_normalize_path_and_combine_preserve_root_and_separator_spelling() {
        assert_eq!(normalize_path("c:"), "c:");
        assert_eq!(normalize_path("//server"), "//server/");
        assert_eq!(normalize_path("/a/./b/"), "/a/b/");
        assert_eq!(normalize_path(".//a"), "a");
        assert_eq!(normalize_path("file:///c:/a/../"), "file:///c:/");
        assert_eq!(combine_paths("/a//", &["b"]), "/a//b");
        assert_eq!(combine_paths("/base", &["c:x"]), "/base/c:x");
        assert_eq!(combine_paths("/base", &["http://server/a"]), "http://server/a");
    }

    #[test]
    fn a_root_is_recognised_in_each_of_its_forms() {
        assert_eq!(get_root_length("/"), 1);
        assert_eq!(get_root_length("/a/b"), 1);
        assert_eq!(get_root_length("c:/"), 3);
        assert_eq!(get_root_length("c:\\a"), 3);
        // A bare drive is a root; a drive-relative name is not.
        assert_eq!(get_root_length("c:"), 2);
        assert_eq!(get_root_length("c:d"), 0);
        assert_eq!(get_root_length("//server/share/a"), "//server/".len());
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
    fn the_lexical_and_known_extension_answers_differ_on_declaration_files() {
        // `GetAnyExtensionFromPath` is purely lexical — last dot in the base name
        // — so a declaration file's extension is `.ts`. An earlier version of this
        // function special-cased `.d.ts` and was wrong about upstream: the
        // composite answer comes from the known-extension list instead, and the
        // resolver depends on having *both*. See `extension.rs`.
        assert_eq!(get_any_extension_from_path("a.d.ts"), ".ts");
        assert_eq!(try_get_extension_from_path("a.d.ts"), ".d.ts");

        assert_eq!(get_any_extension_from_path("a.ts"), ".ts");
        assert_eq!(get_any_extension_from_path("a.b.ts"), ".ts");
        assert_eq!(get_any_extension_from_path(".gitignore"), "");
        assert_eq!(remove_file_extension("/a/b.d.ts"), "/a/b");
        assert_eq!(remove_file_extension("/a/b.ts"), "/a/b");
    }

    #[test]
    fn relative_paths_and_module_specifiers() {
        assert!(path_is_relative("./a"));
        assert!(path_is_relative(".."));
        assert!(!path_is_relative("a/b"));
        assert!(!path_is_relative(".hidden"));
        // A rooted path counts as "relative" for module resolution: it means
        // "do not search node_modules".
        assert!(is_external_module_name_relative("/a/b"));
        assert!(is_external_module_name_relative("./a"));
        assert!(!is_external_module_name_relative("react"));
    }

    #[test]
    fn ancestor_directories_stop_at_the_root() {
        let mut seen = Vec::new();
        let result: Option<()> = for_each_ancestor_directory("/a/b/c", |dir| {
            seen.push(dir.to_string());
            None
        });
        assert!(result.is_none());
        assert_eq!(seen, ["/a/b/c", "/a/b", "/a", "/"]);
    }

    #[test]
    fn containment_compares_component_by_component() {
        let options = ComparePathsOptions {
            use_case_sensitive_file_names: true,
            current_directory: String::new(),
        };
        assert!(contains_path("/a/b", "/a/b/c", &options));
        assert!(contains_path("/a/b", "/a/b", &options));
        // A prefix of a *name* is not a prefix of a *path*.
        assert!(!contains_path("/a/b", "/a/bc", &options));
        assert!(!contains_path("/a/b/c", "/a/b", &options));
    }

    #[test]
    fn a_relative_path_between_directories_climbs_and_descends() {
        let options = ComparePathsOptions {
            use_case_sensitive_file_names: true,
            current_directory: String::new(),
        };
        assert_eq!(get_relative_path_from_directory("/a/b", "/a/b/c/d.ts", &options), "c/d.ts");
        assert_eq!(get_relative_path_from_directory("/a/b/c", "/a/d.ts", &options), "../../d.ts");
        assert_eq!(get_relative_path_from_directory("/a/b", "/a/b", &options), "");
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
