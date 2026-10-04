//! Patterns and package-name arithmetic.
//!
//! Ported from `internal/module/util.go` and `internal/core/pattern.go` at the
//! pinned commit.

use std::cmp::Ordering;

use tsr_core::OrderedMap;

use crate::{
    package_json::TYPESCRIPT_VERSION,
    semver::{Version, VersionRange},
};

/// The synthetic containing file automatic type directives are resolved from
/// (`module.InferredTypesContainingFile`).
pub const INFERRED_TYPES_CONTAINING_FILE: &str = "__inferred type names__.ts";

/// A `paths`/`typesVersions` key: literal text with at most one `*`
/// (`core.Pattern`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    /// The key as written.
    pub text: String,
    /// Where the `*` is, or `None` for an exact match.
    pub star_index: Option<usize>,
}

impl Default for Pattern {
    /// The "no pattern" value, which is upstream's Go zero value.
    ///
    /// Not derived, and the difference is load-bearing. Upstream's `StarIndex`
    /// is an `int` where `-1` means "exact match", so its *zero value* is
    /// `{Text: "", StarIndex: 0}` — which `IsValid` rejects, because `0 < 0` is
    /// false. Mapping `-1` onto `Option::None` makes the derived default
    /// `{text: "", star_index: None}`, which reads as a valid *exact* pattern
    /// matching the empty string.
    ///
    /// That is how `MatchPatternOrExact` returning "nothing matched" became a
    /// valid match here, and `MatchedText` then asserted on a candidate that
    /// does not match. It went unnoticed until `tsr-tsoptions` made `paths`
    /// reachable, because no directive-configured case could set one.
    fn default() -> Self {
        Self { text: String::new(), star_index: Some(0) }
    }
}

impl Pattern {
    /// Parse a key (`core.TryParsePattern`).
    ///
    /// A key with *two* stars is not a pattern at all — it is discarded, not
    /// treated as literal — which [`Pattern::is_valid`] reports.
    #[must_use]
    pub fn parse(pattern: &str) -> Self {
        match pattern.find('*') {
            Some(star) if !pattern[star + 1..].contains('*') => {
                Self { text: pattern.to_string(), star_index: Some(star) }
            }
            None => Self { text: pattern.to_string(), star_index: None },
            // Two stars: not a pattern. Upstream returns the zero value, which
            // `is_valid` rejects.
            Some(_) => Self::default(),
        }
    }

    /// Whether this parsed to something usable (`Pattern.IsValid`).
    #[must_use]
    pub fn is_valid(&self) -> bool {
        match self.star_index {
            None => true,
            Some(star) => star < self.text.len(),
        }
    }

    /// Whether `candidate` matches (`Pattern.Matches`).
    #[must_use]
    pub fn matches(&self, candidate: &str) -> bool {
        match self.star_index {
            None => self.text == candidate,
            Some(star) => {
                candidate.len() + 1 >= self.text.len()
                    && candidate.starts_with(&self.text[..star])
                    && candidate.ends_with(&self.text[star + 1..])
            }
        }
    }

    /// What the `*` matched (`Pattern.MatchedText`).
    ///
    /// # Panics
    ///
    /// If `candidate` does not match — upstream panics here too, because a
    /// caller that has not checked has a logic error rather than a bad input.
    #[must_use]
    pub fn matched_text<'a>(&self, candidate: &'a str) -> &'a str {
        assert!(self.matches(candidate), "candidate does not match pattern");
        match self.star_index {
            None => "",
            Some(star) => &candidate[star..candidate.len() - (self.text.len() - star - 1)],
        }
    }
}

/// A `paths` table split into exact keys and star patterns
/// (`module.ParsedPatterns`).
#[derive(Debug, Clone, Default)]
pub struct ParsedPatterns {
    matchable_strings: Vec<String>,
    patterns: Vec<Pattern>,
}

/// Split a mapping's keys (`module.TryParsePatterns`).
#[must_use]
pub fn try_parse_patterns(path_mappings: &OrderedMap<Vec<String>>) -> ParsedPatterns {
    let mut matchable_strings = Vec::new();
    let mut patterns = Vec::new();
    for key in path_mappings.keys() {
        let pattern = Pattern::parse(key);
        if !pattern.is_valid() {
            continue;
        }
        if pattern.star_index.is_none() {
            matchable_strings.push(key.to_string());
        } else {
            patterns.push(pattern);
        }
    }
    ParsedPatterns { matchable_strings, patterns }
}

/// The best key for `candidate`, exact before pattern
/// (`module.MatchPatternOrExact`).
///
/// Returns an invalid pattern when nothing matches.
#[must_use]
pub fn match_pattern_or_exact(patterns: &ParsedPatterns, candidate: &str) -> Pattern {
    if patterns.matchable_strings.iter().any(|text| text == candidate) {
        return Pattern { text: candidate.to_string(), star_index: None };
    }
    if patterns.patterns.is_empty() {
        return Pattern::default();
    }
    find_best_pattern_match(&patterns.patterns, candidate)
}

/// The pattern with the longest literal prefix that matches
/// (`core.FindBestPatternMatch`).
#[must_use]
pub fn find_best_pattern_match(patterns: &[Pattern], candidate: &str) -> Pattern {
    let mut best = Pattern::default();
    // `None` stands for upstream's `-1`: no prefix seen yet. An *exact* pattern
    // also resets it to `None`, which is upstream's behaviour and not an
    // oversight — an exact match cannot be beaten, so the bar goes back down.
    let mut longest_prefix: Option<usize> = None;
    for pattern in patterns {
        let beats_current = match (pattern.star_index, longest_prefix) {
            (None, _) | (Some(_), None) => true,
            (Some(star), Some(longest)) => star > longest,
        };
        if beats_current && pattern.matches(candidate) {
            best = pattern.clone();
            longest_prefix = pattern.star_index;
        }
    }
    best
}

/// Order `exports` keys: longest literal prefix first
/// (`module.ComparePatternKeys`).
///
/// Note the reversal — this returns `Less` for the *longer* prefix, so a sort
/// puts the most specific key first. That is what makes `./sub/*` win over `./*`.
#[must_use]
pub fn compare_pattern_keys(a: &str, b: &str) -> Ordering {
    let a_star = a.find('*');
    let b_star = b.find('*');
    let base_a = a_star.map_or(a.len(), |index| index + 1);
    let base_b = b_star.map_or(b.len(), |index| index + 1);

    if base_a > base_b {
        return Ordering::Less;
    }
    if base_b > base_a {
        return Ordering::Greater;
    }
    // A key with no star loses to one with a star of the same base length,
    // which reads backwards and is upstream's.
    match (a_star, b_star) {
        (None, _) => Ordering::Greater,
        (_, None) => Ordering::Less,
        (Some(_), Some(_)) => b.len().cmp(&a.len()),
    }
}

/// Whether an `exports` condition spelled `types@<range>` applies
/// (`module.IsApplicableVersionedTypesKey`).
#[must_use]
pub fn is_applicable_versioned_types_key(key: &str) -> bool {
    let Some(range) = key.strip_prefix("types@") else { return false };
    VersionRange::parse(range)
        .is_some_and(|range| range.test(&Version::must_parse(TYPESCRIPT_VERSION)))
}

/// The package directory a resolved path lives in
/// (`module.ParseNodeModuleFromPath`).
///
/// Empty when the path is not under `node_modules`. `is_folder` decides what
/// happens when the package name runs to the end of the string: a folder keeps
/// it, a file does not.
#[must_use]
pub fn parse_node_module_from_path(resolved: &str, is_folder: bool) -> String {
    let path = tsr_path::normalize_path(resolved);
    let Some(index) = path.rfind("/node_modules/") else { return String::new() };

    let after_node_modules = index + "/node_modules/".len();
    let mut end = move_to_next_directory_separator(&path, after_node_modules, is_folder);
    if path.as_bytes().get(after_node_modules) == Some(&b'@') {
        end = move_to_next_directory_separator(&path, end, is_folder);
    }
    path[..end].to_string()
}

/// `module.moveToNextDirectorySeparatorIfAvailable`.
fn move_to_next_directory_separator(path: &str, previous: usize, is_folder: bool) -> usize {
    let offset = previous + 1;
    if offset > path.len() {
        return if is_folder { path.len() } else { previous };
    }
    match path[offset..].find('/') {
        Some(index) => index + offset,
        None if is_folder => path.len(),
        None => previous,
    }
}

/// Split a specifier into its package name and the rest
/// (`module.ParsePackageName`).
///
/// `@scope/name/sub` splits after the *second* segment, which is the whole
/// reason this is not `split_once('/')`.
#[must_use]
pub fn parse_package_name(module_name: &str) -> (&str, &str) {
    let mut index = module_name.find('/');
    if module_name.starts_with('@') {
        let offset = index.map_or(0, |i| i + 1);
        index = module_name[offset..].find('/').map(|i| i + offset);
    }
    match index {
        None => (module_name, ""),
        Some(index) => (&module_name[..index], &module_name[index + 1..]),
    }
}

/// `@scope/name` to `scope__name` (`module.MangleScopedPackageName`).
///
/// `@types` packages on npm are published under the mangled name because npm
/// scopes cannot nest.
#[must_use]
pub fn mangle_scoped_package_name(package_name: &str) -> String {
    if let Some(rest) = package_name.strip_prefix('@')
        && let Some(slash) = rest.find('/')
    {
        return format!("{}__{}", &rest[..slash], &rest[slash + 1..]);
    }
    package_name.to_string()
}

/// `scope__name` back to `@scope/name` (`module.UnmangleScopedPackageName`).
#[must_use]
pub fn unmangle_scoped_package_name(package_name: &str) -> String {
    match package_name.split_once("__") {
        Some((before, after)) => format!("@{before}/{after}"),
        None => package_name.to_string(),
    }
}

/// The `@types` package for a package (`module.GetTypesPackageName`).
#[must_use]
pub fn get_types_package_name(package_name: &str) -> String {
    format!("@types/{}", mangle_scoped_package_name(package_name))
}

/// If `./foo` names a directory, keep the trailing slash
/// (`module.normalizePathForCJSResolution`).
///
/// Normalising `import "."` inside `/foo` would give `/foo` and lose the fact
/// that we meant to look *inside* it.
#[must_use]
pub fn normalize_path_for_cjs_resolution(containing_directory: &str, module_name: &str) -> String {
    let combined = tsr_path::combine_paths(containing_directory, &[module_name]);
    // Native pathComponents keeps the root separate and removes one trailing
    // empty segment. Borrow its final body segment; URI/UNC roots are not dots.
    let body = &combined[tsr_path::get_root_length(&combined)..];
    let body = body.strip_suffix('/').unwrap_or(body);
    if matches!(body.rsplit('/').next(), Some("." | "..")) {
        return tsr_path::ensure_trailing_directory_separator(&tsr_path::normalize_path(&combined));
    }
    tsr_path::normalize_path(&combined)
}

/// Whether an `exports` key with text after its `*` matches
/// (`module.matchesPatternWithTrailer`).
#[must_use]
pub fn matches_pattern_with_trailer(target: &str, name: &str) -> bool {
    if target.ends_with('*') {
        return false;
    }
    let Some((before, after)) = target.split_once('*') else { return false };
    name.starts_with(before) && name.ends_with(after)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjs_last_component_preserves_native_root_and_separator_boundaries() {
        // Pinned module.normalizePathForCJSResolution at5b1047d1; root and
        // trailing-empty segment behavior comes from tspath.pathComponents.
        let cases = [
            ("/a/b", ".", "/a/b/"),
            ("/a/b", "..", "/a/"),
            ("/a/b", "a/.//", "/a/b/a/"),
            ("/a/b", "a/..///", "/a/b/"),
            ("", ".", "/"),
            ("", "..", "../"),
            ("", "", ""),
            ("", "c:", "c:"),
            ("", "c:x", "c:x"),
            ("", "//.", "//./"),
            ("", "http://.", "http://./"),
            ("", "http://../", "http://../"),
            ("", "http://./.", "http://./"),
            ("", "http://../..", "http://../"),
            ("", "file:///c:/..", "file:///c:/"),
            ("^/α", "..", "^/"),
            ("/α/文件", "🙂/..", "/α/文件/"),
            ("c:/a", "a\\..\\", "c:/a/"),
            ("//server/share", "..", "//server/"),
        ];
        for (directory, name, expected) in cases {
            assert_eq!(
                normalize_path_for_cjs_resolution(directory, name),
                expected,
                "directory={directory:?}, name={name:?}"
            );
        }
    }

    #[test]
    fn a_scoped_package_splits_after_its_second_segment() {
        assert_eq!(parse_package_name("foo"), ("foo", ""));
        assert_eq!(parse_package_name("foo/bar"), ("foo", "bar"));
        assert_eq!(parse_package_name("@scope/name"), ("@scope/name", ""));
        assert_eq!(parse_package_name("@scope/name/sub"), ("@scope/name", "sub"));
    }

    #[test]
    fn scoped_names_round_trip_through_the_mangling_npm_forces() {
        assert_eq!(mangle_scoped_package_name("@scope/name"), "scope__name");
        assert_eq!(unmangle_scoped_package_name("scope__name"), "@scope/name");
        assert_eq!(mangle_scoped_package_name("plain"), "plain");
        assert_eq!(get_types_package_name("@scope/name"), "@types/scope__name");
    }

    #[test]
    fn the_package_directory_is_found_by_walking_past_node_modules() {
        assert_eq!(
            parse_node_module_from_path("/a/node_modules/foo/index.d.ts", false),
            "/a/node_modules/foo"
        );
        assert_eq!(
            parse_node_module_from_path("/a/node_modules/@scope/foo/index.d.ts", false),
            "/a/node_modules/@scope/foo"
        );
        // The innermost `node_modules` wins.
        assert_eq!(
            parse_node_module_from_path("/a/node_modules/foo/node_modules/bar/i.d.ts", false),
            "/a/node_modules/foo/node_modules/bar"
        );
        assert_eq!(parse_node_module_from_path("/a/b.ts", false), "");
    }

    #[test]
    fn a_two_star_pattern_is_discarded_rather_than_taken_literally() {
        assert!(!Pattern::parse("a*b*c").is_valid());
        assert!(Pattern::parse("a*c").is_valid());
        assert!(Pattern::parse("exact").is_valid());
    }

    #[test]
    fn a_star_may_match_nothing() {
        let pattern = Pattern::parse("a*c");
        assert!(pattern.matches("ac"));
        assert_eq!(pattern.matched_text("ac"), "");
        assert!(pattern.matches("abc"));
        assert_eq!(pattern.matched_text("abc"), "b");
        assert!(!pattern.matches("ab"));
    }

    #[test]
    fn the_longest_literal_prefix_wins() {
        let patterns = [Pattern::parse("*"), Pattern::parse("a/*"), Pattern::parse("a/b/*")];
        assert_eq!(find_best_pattern_match(&patterns, "a/b/c").text, "a/b/*");
        assert_eq!(find_best_pattern_match(&patterns, "a/c").text, "a/*");
        assert_eq!(find_best_pattern_match(&patterns, "z").text, "*");
    }

    #[test]
    fn exports_keys_sort_most_specific_first() {
        let mut keys = vec!["./*", "./sub/*", "./sub/deep/*"];
        keys.sort_by(|a, b| compare_pattern_keys(a, b));
        assert_eq!(keys, ["./sub/deep/*", "./sub/*", "./*"]);
    }

    #[test]
    fn a_versioned_types_condition_is_tested_against_the_compiler_version() {
        assert!(is_applicable_versioned_types_key("types@>=4"));
        assert!(!is_applicable_versioned_types_key("types@<4"));
        assert!(!is_applicable_versioned_types_key("types@>=10000"));
        // Not a versioned key at all.
        assert!(!is_applicable_versioned_types_key("import"));
        assert!(!is_applicable_versioned_types_key("types@not-a-range"));
    }

    #[test]
    fn importing_a_directory_by_dot_keeps_its_trailing_slash() {
        // Without this, `import "."` inside `/foo` resolves as if it named the
        // file `/foo` rather than the directory.
        assert_eq!(normalize_path_for_cjs_resolution("/foo", "."), "/foo/");
        assert_eq!(normalize_path_for_cjs_resolution("/foo/bar", ".."), "/foo/");
        assert_eq!(normalize_path_for_cjs_resolution("/foo", "./a"), "/foo/a");
    }

    #[test]
    fn a_pattern_with_a_trailer_matches_on_both_ends() {
        assert!(matches_pattern_with_trailer("./*.js", "./a.js"));
        assert!(!matches_pattern_with_trailer("./*", "./a.js"));
        assert!(!matches_pattern_with_trailer("./a", "./a"));
    }
}
