//! Matching `include`/`exclude` globs against a file system.
//!
//! Ported from `internal/vfs/vfsmatch/vfsmatch.go` at the pinned commit, which
//! implements the algorithm upstream specifies in its `MATCHING_ALGORITHM.md`.
//!
//! # Why this is not a regex, and not a glob crate
//!
//! TypeScript's `include` patterns are not POSIX globs and not `.gitignore`
//! patterns. They carry three rules that no general-purpose matcher has, and all
//! three change which files end up in a program:
//!
//! - **`**` in an *include* refuses to descend into `node_modules`,
//!   `bower_components`, `jspm_packages`, or any dot-directory** — but the same
//!   `**` in an *exclude* descends happily. A pattern's meaning depends on which
//!   list it was written in.
//! - **A leading wildcard does not match a dotfile.** `*.ts` does not match
//!   `.a.ts`, though `.a.ts` is a perfectly good file name.
//! - **`*.js` does not match `foo.min.js`** unless the pattern itself mentions
//!   `.min.`. A minified file is assumed to be output, not input.
//!
//! Upstream used to compile these to regular expressions and stopped; this is
//! the replacement, and it is a direct port rather than an approximation because
//! the rules above are the difference between a program and a slightly different
//! program.
//!
//! # What is a deliberate simplification
//!
//! Upstream's visitor computes a child directory's real path *incrementally*
//! (`CombinePaths(realPath, dir)`) when it knows the child is not a symlink, and
//! only calls `Realpath` otherwise. That needs the file system to report which
//! entries are symlinks, which [`DirectoryEntries`] does not. This calls
//! [`FileSystem::realpath`] for every directory instead: the same answer, one
//! lookup rather than none, on a path that runs once per configured project.

use rustc_hash::FxHashSet;
use tsr_path::{
    ComparePathsOptions, combine_paths, compare_paths, contains_path,
    extension::file_extension_is_one_of, get_canonical_file_name, get_directory_path,
    get_path_components, has_extension, is_rooted_disk_path, normalize_path,
    reduce_path_components, remove_trailing_directory_separator,
};

use crate::{DirectoryEntries, FileSystem};

/// What a compiled pattern is going to be asked about (`vfsmatch.Usage`).
///
/// Not presentation: it selects between three genuinely different matching
/// rules. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Usage {
    /// Matching file names. Excludes `*.min.js` by default.
    Files,
    /// Matching directory names, to decide whether to descend.
    Directories,
    /// An `exclude` pattern, which may descend into package folders.
    Exclude,
}

/// Every file under `path` matching `includes`, minus `excludes`, restricted to
/// `extensions` (`vfsmatch.ReadDirectory`).
///
/// Results are grouped by which include pattern matched and concatenated in that
/// order, because `include` order is observable: it decides which of two files
/// differing only in extension wins.
#[must_use]
pub fn read_directory(
    fs: &dyn FileSystem,
    current_directory: &str,
    path: &str,
    extensions: &[&str],
    excludes: &[String],
    includes: &[String],
) -> Vec<String> {
    let path = normalize_path(path);
    let current_directory = normalize_path(current_directory);
    let absolute_path = combine_paths(&current_directory, &[&path]);
    let case_sensitive = fs.use_case_sensitive_file_names();

    let file_matcher =
        GlobMatcher::new(includes, excludes, &absolute_path, case_sensitive, Usage::Files);
    let directory_matcher =
        GlobMatcher::new(includes, excludes, &absolute_path, case_sensitive, Usage::Directories);

    let mut visitor = Visitor {
        fs,
        results: vec![Vec::new(); file_matcher.includes.len().max(1)],
        file_matcher,
        directory_matcher,
        extensions,
        case_sensitive,
        visited: FxHashSet::default(),
    };

    for base_path in base_paths(&path, includes, case_sensitive) {
        let absolute = combine_paths(&current_directory, &[&base_path]);
        visitor.visit(&base_path, &absolute);
    }
    visitor.results.concat()
}

/// One or more compiled specs, for asking "does this path match any of them"
/// without walking a file system (`vfsmatch.SpecMatcher`).
#[derive(Debug)]
pub struct SpecMatcher {
    patterns: Vec<GlobPattern>,
}

impl SpecMatcher {
    /// Compile `specs`. Returns `None` when nothing compiled to a pattern that
    /// can match — a spec ending in `**` matches no *file*, for instance.
    #[must_use]
    pub fn new(
        specs: &[String],
        base_path: &str,
        usage: Usage,
        case_sensitive: bool,
    ) -> Option<Self> {
        let patterns: Vec<GlobPattern> = specs
            .iter()
            .filter_map(|spec| GlobPattern::compile(spec, base_path, usage, case_sensitive))
            .collect();
        (!patterns.is_empty()).then_some(Self { patterns })
    }

    /// The index of the first pattern matching `path` (`SpecMatcher.MatchIndex`).
    #[must_use]
    pub fn match_index(&self, path: &str) -> Option<usize> {
        self.patterns.iter().position(|pattern| pattern.matches(path, ""))
    }

    /// Whether any pattern matches `path` (`SpecMatcher.MatchString`).
    #[must_use]
    pub fn matches(&self, path: &str) -> bool {
        self.match_index(path).is_some()
    }
}

/// Whether a path component is implicitly a directory glob
/// (`vfsmatch.IsImplicitGlob`).
///
/// An `include` of `foo` means `foo/**/*` when `foo` has no extension and no
/// wildcard of its own.
#[must_use]
pub fn is_implicit_glob(last_path_component: &str) -> bool {
    !last_path_component.contains(['.', '*', '?'])
}

// ---- Compiled patterns -----------------------------------------------------

/// A compiled glob (`vfsmatch.globPattern`).
#[derive(Debug)]
struct GlobPattern {
    components: Vec<Component>,
    is_exclude: bool,
    case_sensitive: bool,
    /// `*.js` should not match `foo.min.js`. Only when matching files.
    exclude_min_js: bool,
}

/// One path segment of a pattern (`vfsmatch.component`).
#[derive(Debug)]
enum Component {
    /// An exact name, e.g. `src`.
    Literal(String),
    /// Contains `*` or `?`, e.g. `*.ts`.
    Wildcard {
        segments: Vec<Segment>,
        /// Include patterns refuse to match `node_modules` and friends.
        skip_package_folders: bool,
    },
    /// `**`: zero or more directories.
    DoubleAsterisk,
}

/// A piece of a wildcard component (`vfsmatch.segment`). `*.ts` is
/// `[Star, Literal(".ts")]`.
#[derive(Debug)]
enum Segment {
    Literal(String),
    Star,
    Question,
}

impl GlobPattern {
    /// `vfsmatch.compileGlobPattern`. `None` when the pattern can match nothing.
    fn compile(spec: &str, base_path: &str, usage: Usage, case_sensitive: bool) -> Option<Self> {
        let mut parts = reduce_path_components(&get_path_components(spec, base_path));

        // `src/**` names no file, so as an include it matches nothing. As an
        // exclude it means the directory and everything under it.
        if usage != Usage::Exclude && parts.last().is_some_and(|last| last == "**") {
            return None;
        }
        // The root component is stored without its separator, because that is
        // what the empty leading component of a rooted path compares equal to.
        if let Some(first) = parts.first_mut() {
            *first = remove_trailing_directory_separator(first);
        }
        // A bare directory name means everything under it.
        if parts.last().is_some_and(|last| is_implicit_glob(last)) {
            parts.push("**".to_string());
            parts.push("*".to_string());
        }

        let is_include = usage != Usage::Exclude;
        Some(Self {
            components: parts.iter().map(|part| Component::parse(part, is_include)).collect(),
            is_exclude: usage == Usage::Exclude,
            case_sensitive,
            exclude_min_js: usage == Usage::Files,
        })
    }

    /// Whether `prefix + suffix` matches (`globPattern.matchesParts`).
    fn matches(&self, prefix: &str, suffix: &str) -> bool {
        self.match_from(prefix, suffix, 0, 0, false)
    }

    /// Whether anything *under* `prefix + suffix` could match
    /// (`globPattern.matchesPrefixParts`), which is what decides whether to
    /// descend into a directory.
    fn matches_prefix(&self, prefix: &str, suffix: &str) -> bool {
        self.match_from(prefix, suffix, 0, 0, true)
    }

    /// `globPattern.matchPathParts`.
    ///
    /// `prefix`/`suffix` are matched as though concatenated. The split exists
    /// because the caller has a directory path and an entry name and would
    /// otherwise allocate their concatenation once per entry.
    fn match_from(
        &self,
        prefix: &str,
        suffix: &str,
        mut offset: usize,
        mut component_index: usize,
        prefix_only: bool,
    ) -> bool {
        loop {
            let Some((part, next_offset)) = next_path_part(prefix, suffix, offset) else {
                return prefix_only || self.is_satisfied(component_index);
            };
            let Some(component) = self.components.get(component_index) else {
                // The path is longer than the pattern. An exclude of `foo`
                // excludes everything under it; an include does not.
                return self.is_exclude && !prefix_only;
            };

            match component {
                Component::DoubleAsterisk => {
                    if self.match_from(prefix, suffix, offset, component_index + 1, prefix_only) {
                        return true;
                    }
                    // `**` in an include never descends into a package folder or
                    // a dot-directory. This is the rule that keeps a default
                    // `include` out of `node_modules`.
                    if !self.is_exclude && (is_hidden(part) || is_package_folder(part)) {
                        return false;
                    }
                    offset = next_offset;
                    continue;
                }
                Component::Literal(literal) => {
                    if !self.eq(literal, part) {
                        return false;
                    }
                }
                Component::Wildcard { segments, skip_package_folders } => {
                    if *skip_package_folders && is_package_folder(part) {
                        return false;
                    }
                    if !self.match_wildcard(segments, part) {
                        return false;
                    }
                }
            }

            offset = next_offset;
            component_index += 1;
        }
    }

    /// Whether the components from `index` on can match nothing at all
    /// (`globPattern.patternSatisfied`). Only a trailing run of `**` can.
    fn is_satisfied(&self, index: usize) -> bool {
        self.components[index..].iter().all(|c| matches!(c, Component::DoubleAsterisk))
    }

    /// `globPattern.matchWildcard`.
    fn match_wildcard(&self, segments: &[Segment], candidate: &str) -> bool {
        // A leading wildcard in an include does not match a dotfile.
        if !self.is_exclude
            && is_hidden(candidate)
            && matches!(segments.first(), Some(Segment::Star | Segment::Question))
        {
            return false;
        }
        // `*.ts`, by far the common shape, needs no backtracking.
        if let [Segment::Star, Segment::Literal(suffix)] = segments {
            return candidate.len() >= suffix.len()
                && self.eq(suffix, &candidate[candidate.len() - suffix.len()..])
                && self.should_include_min_js(candidate, segments);
        }
        self.match_segments(segments, candidate) && self.should_include_min_js(candidate, segments)
    }

    /// `globPattern.matchSegments`.
    ///
    /// Iterative with a single backtrack point, which is what makes it O(n·m)
    /// rather than exponential: a pattern like `*a*a*a*b` against `aaaa…` would
    /// otherwise be a denial of service on any repository containing it.
    fn match_segments(&self, segments: &[Segment], candidate: &str) -> bool {
        let (mut segment_index, mut index) = (0_usize, 0_usize);
        let mut star: Option<(usize, usize)> = None;

        while index < candidate.len() {
            if let Some(segment) = segments.get(segment_index) {
                match segment {
                    Segment::Literal(literal) => {
                        let end = index + literal.len();
                        if candidate.is_char_boundary(end.min(candidate.len()))
                            && end <= candidate.len()
                            && self.eq(literal, &candidate[index..end])
                        {
                            index = end;
                            segment_index += 1;
                            continue;
                        }
                    }
                    Segment::Question => {
                        if !candidate[index..].starts_with('/') {
                            index += next_char_len(candidate, index);
                            segment_index += 1;
                            continue;
                        }
                    }
                    Segment::Star => {
                        star = Some((segment_index, index));
                        segment_index += 1;
                        continue;
                    }
                }
            }

            // No match here: let the most recent star eat one more character.
            match star {
                Some((star_segment, star_index))
                    if star_index < candidate.len()
                        && !candidate[star_index..].starts_with('/') =>
                {
                    let advanced = star_index + next_char_len(candidate, star_index);
                    star = Some((star_segment, advanced));
                    index = advanced;
                    segment_index = star_segment + 1;
                }
                _ => return false,
            }
        }

        while matches!(segments.get(segment_index), Some(Segment::Star)) {
            segment_index += 1;
        }
        segment_index >= segments.len()
    }

    /// `globPattern.shouldIncludeMinJs`: a minified file is output, unless the
    /// pattern says otherwise.
    fn should_include_min_js(&self, file_name: &str, segments: &[Segment]) -> bool {
        if !self.exclude_min_js || !self.has_min_js_suffix(file_name) {
            return true;
        }
        segments.iter().any(|segment| match segment {
            Segment::Literal(literal) if self.case_sensitive => {
                literal.contains(".min.js") || literal.contains(".min.")
            }
            Segment::Literal(literal) => {
                let lowered = literal.to_lowercase();
                lowered.contains(".min.js") || lowered.contains(".min.")
            }
            _ => false,
        })
    }

    fn has_min_js_suffix(&self, file_name: &str) -> bool {
        const MIN_JS: &str = ".min.js";
        if self.case_sensitive {
            return file_name.ends_with(MIN_JS);
        }
        file_name.len() >= MIN_JS.len()
            && file_name[file_name.len() - MIN_JS.len()..].eq_ignore_ascii_case(MIN_JS)
    }

    fn eq(&self, a: &str, b: &str) -> bool {
        if self.case_sensitive { a == b } else { a.eq_ignore_ascii_case(b) }
    }
}

impl Component {
    /// `vfsmatch.parseComponent`.
    fn parse(part: &str, is_include: bool) -> Self {
        if part == "**" {
            return Self::DoubleAsterisk;
        }
        if !part.contains(['*', '?']) {
            return Self::Literal(part.to_string());
        }
        Self::Wildcard { segments: parse_segments(part), skip_package_folders: is_include }
    }
}

/// `vfsmatch.parseSegments`.
fn parse_segments(part: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut start = 0;
    for (index, byte) in part.bytes().enumerate() {
        if byte != b'*' && byte != b'?' {
            continue;
        }
        if index > start {
            segments.push(Segment::Literal(part[start..index].to_string()));
        }
        segments.push(if byte == b'*' { Segment::Star } else { Segment::Question });
        start = index + 1;
    }
    if start < part.len() {
        segments.push(Segment::Literal(part[start..].to_string()));
    }
    segments
}

/// The next path component of `prefix + suffix` from `offset`, and where the one
/// after it starts (`vfsmatch.nextPathPartParts`).
///
/// A rooted path yields an empty first component, which is what the pattern's
/// separator-stripped root component compares equal to.
fn next_path_part<'a>(prefix: &'a str, suffix: &'a str, offset: usize) -> Option<(&'a str, usize)> {
    let total = prefix.len() + suffix.len();
    if offset >= total {
        return None;
    }
    if offset == 0 && prefix.as_bytes().first().or(suffix.as_bytes().first()) == Some(&b'/') {
        return Some(("", 1));
    }

    let mut offset = offset;
    if offset < prefix.len() {
        while offset < prefix.len() && prefix.as_bytes()[offset] == b'/' {
            offset += 1;
        }
        if offset < prefix.len() {
            let rest = &prefix[offset..];
            return Some(match rest.find('/') {
                Some(slash) => (&rest[..slash], offset + slash),
                // Only reachable when there is no suffix; with one, the caller's
                // prefix is a directory path ending in `/`.
                None => (rest, prefix.len()),
            });
        }
    }

    let in_suffix = offset - prefix.len();
    if in_suffix >= suffix.len() {
        return None;
    }
    let rest = &suffix[in_suffix..];
    Some(match rest.find('/') {
        Some(slash) => (&rest[..slash], offset + slash),
        None => (rest, total),
    })
}

/// The byte length of the character at `index`, so a star consumes one *char*
/// rather than one byte.
fn next_char_len(s: &str, index: usize) -> usize {
    s[index..].chars().next().map_or(1, char::len_utf8)
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// `vfsmatch.isPackageFolder`.
fn is_package_folder(name: &str) -> bool {
    ["node_modules", "jspm_packages", "bower_components"]
        .iter()
        .any(|folder| name.eq_ignore_ascii_case(folder))
}

fn ensure_trailing_slash(path: &str) -> String {
    if path.ends_with('/') { path.to_string() } else { format!("{path}/") }
}

// ---- Matching a whole spec list --------------------------------------------

/// Include and exclude patterns together (`vfsmatch.globMatcher`).
#[derive(Debug)]
struct GlobMatcher {
    includes: Vec<GlobPattern>,
    excludes: Vec<GlobPattern>,
    /// Whether include specs were *given*, which is not the same as whether any
    /// compiled: `include: ["src/**"]` gives one spec and no file pattern, and
    /// must match nothing rather than everything.
    had_includes: bool,
}

impl GlobMatcher {
    fn new(
        include_specs: &[String],
        exclude_specs: &[String],
        base_path: &str,
        case_sensitive: bool,
        usage: Usage,
    ) -> Self {
        Self {
            had_includes: !include_specs.is_empty(),
            includes: include_specs
                .iter()
                .filter_map(|spec| GlobPattern::compile(spec, base_path, usage, case_sensitive))
                .collect(),
            excludes: exclude_specs
                .iter()
                .filter_map(|spec| {
                    GlobPattern::compile(spec, base_path, Usage::Exclude, case_sensitive)
                })
                .collect(),
        }
    }

    /// Which include bucket `prefix + suffix` lands in, if any
    /// (`globMatcher.matchesFileParts`).
    fn match_file(&self, prefix: &str, suffix: &str) -> Option<usize> {
        if self.excludes.iter().any(|pattern| pattern.matches(prefix, suffix)) {
            return None;
        }
        if self.includes.is_empty() {
            return (!self.had_includes).then_some(0);
        }
        self.includes.iter().position(|pattern| pattern.matches(prefix, suffix))
    }

    /// Whether to descend into a directory (`globMatcher.matchesDirectoryParts`).
    fn match_directory(&self, prefix: &str, suffix: &str) -> bool {
        if self.excludes.iter().any(|pattern| pattern.matches(prefix, suffix)) {
            return false;
        }
        if self.includes.is_empty() {
            return !self.had_includes;
        }
        self.includes.iter().any(|pattern| pattern.matches_prefix(prefix, suffix))
    }
}

/// The directory walk (`vfsmatch.globVisitor`).
struct Visitor<'a> {
    fs: &'a dyn FileSystem,
    file_matcher: GlobMatcher,
    directory_matcher: GlobMatcher,
    extensions: &'a [&'a str],
    case_sensitive: bool,
    /// Real paths already walked, so a symlink cycle terminates.
    visited: FxHashSet<String>,
    /// One bucket per include pattern, concatenated at the end.
    results: Vec<Vec<String>>,
}

impl Visitor<'_> {
    fn visit(&mut self, path: &str, absolute_path: &str) {
        let real_path = self.fs.realpath(absolute_path);
        if !self.visited.insert(get_canonical_file_name(&real_path, self.case_sensitive)) {
            return;
        }

        let DirectoryEntries { files, directories } = self.fs.get_accessible_entries(absolute_path);
        let path_prefix = ensure_trailing_slash(path);
        let absolute_prefix = ensure_trailing_slash(absolute_path);

        for file in &files {
            if !self.extensions.is_empty() && !file_extension_is_one_of(file, self.extensions) {
                continue;
            }
            if let Some(bucket) = self.file_matcher.match_file(&absolute_prefix, file) {
                self.results[bucket].push(format!("{path_prefix}{file}"));
            }
        }

        for directory in &directories {
            if !self.directory_matcher.match_directory(&absolute_prefix, directory) {
                continue;
            }
            self.visit(
                &format!("{path_prefix}{directory}"),
                &format!("{absolute_prefix}{directory}"),
            );
        }
    }
}

/// The distinct non-wildcard roots the include patterns reach into
/// (`vfsmatch.getBasePaths`).
///
/// Without this an `include` of `../shared/**/*` would never be found, because
/// the walk starts at the config's directory.
fn base_paths(path: &str, includes: &[String], case_sensitive: bool) -> Vec<String> {
    let mut base_paths = vec![path.to_string()];
    if includes.is_empty() {
        return base_paths;
    }

    let options = ComparePathsOptions {
        current_directory: path.to_string(),
        use_case_sensitive_file_names: case_sensitive,
    };
    let mut include_base_paths: Vec<String> = includes
        .iter()
        .map(|include| {
            let absolute = if is_rooted_disk_path(include) {
                include.clone()
            } else {
                normalize_path(&combine_paths(path, &[include]))
            };
            include_base_path(&absolute)
        })
        .collect();
    include_base_paths.sort_by(|a, b| compare_paths(a, b, &options));

    for candidate in include_base_paths {
        if base_paths.iter().all(|base| !contains_path(base, &candidate, &options)) {
            base_paths.push(candidate);
        }
    }
    base_paths
}

/// The part of an include pattern before its first wildcard
/// (`vfsmatch.getIncludeBasePath`).
fn include_base_path(absolute: &str) -> String {
    match absolute.find(['*', '?']) {
        None if !has_extension(absolute) => absolute.to_string(),
        None => remove_trailing_directory_separator(get_directory_path(absolute)),
        Some(wildcard) => absolute[..absolute[..wildcard].rfind('/').unwrap_or(0)].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryFileSystem;

    fn fs(paths: &[&str]) -> InMemoryFileSystem {
        InMemoryFileSystem::new(
            paths.iter().map(|path| ((*path).to_string(), String::new())),
            [],
            true,
        )
    }

    fn read(fs: &InMemoryFileSystem, includes: &[&str], excludes: &[&str]) -> Vec<String> {
        read_directory(
            fs,
            "/",
            "/",
            &[".ts", ".tsx", ".d.ts"],
            &excludes.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(),
            &includes.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(),
        )
    }

    #[test]
    fn the_default_include_takes_every_supported_file_recursively() {
        let fs = fs(&["/a.ts", "/src/b.ts", "/src/deep/c.tsx", "/src/d.js", "/README.md"]);
        assert_eq!(read(&fs, &["**/*"], &[]), ["/a.ts", "/src/b.ts", "/src/deep/c.tsx"]);
    }

    #[test]
    fn an_include_wildcard_refuses_to_enter_package_folders_and_an_exclude_does_not() {
        // The single most consequential rule here: it is why a default config
        // does not compile every dependency it has.
        let fs = fs(&["/a.ts", "/node_modules/pkg/index.d.ts", "/bower_components/b.ts"]);
        assert_eq!(read(&fs, &["**/*"], &[]), ["/a.ts"]);
        // Named explicitly, a package folder is reachable — the refusal is
        // about the wildcard, not about the directory.
        assert_eq!(read(&fs, &["node_modules/**/*"], &[]), ["/node_modules/pkg/index.d.ts"]);
    }

    #[test]
    fn a_leading_wildcard_does_not_match_a_dotfile_or_a_dot_directory() {
        let fs = fs(&["/a.ts", "/.hidden.ts", "/.git/x.ts"]);
        assert_eq!(read(&fs, &["**/*"], &[]), ["/a.ts"]);
    }

    #[test]
    fn excludes_win_over_includes() {
        let fs = fs(&["/src/a.ts", "/bin/a.ts"]);
        assert_eq!(read(&fs, &["**/*"], &["bin"]), ["/src/a.ts"]);
    }

    #[test]
    fn results_are_grouped_by_include_pattern_in_order() {
        // `include` order is observable: it decides which of two files that
        // differ only in extension the config-file parser keeps.
        let fs = fs(&["/a.ts", "/b.tsx"]);
        assert_eq!(read(&fs, &["**/*.tsx", "**/*.ts"], &[]), ["/b.tsx", "/a.ts"]);
    }

    #[test]
    fn an_include_ending_in_a_double_asterisk_matches_no_file() {
        let fs = fs(&["/src/a.ts"]);
        assert!(read(&fs, &["src/**"], &[]).is_empty());
    }

    #[test]
    fn a_bare_directory_name_means_everything_under_it() {
        let fs = fs(&["/src/a.ts", "/src/deep/b.ts", "/other/c.ts"]);
        assert_eq!(read(&fs, &["src"], &[]), ["/src/a.ts", "/src/deep/b.ts"]);
    }

    #[test]
    fn a_minified_file_is_output_unless_the_pattern_says_otherwise() {
        let fs = InMemoryFileSystem::new(
            [("/a.min.js".to_string(), String::new()), ("/b.js".to_string(), String::new())],
            [],
            true,
        );
        let read = |includes: &[&str]| {
            read_directory(
                &fs,
                "/",
                "/",
                &[".js"],
                &[],
                &includes.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(),
            )
        };
        assert_eq!(read(&["**/*"]), ["/b.js"]);
        assert_eq!(read(&["**/*.min.js"]), ["/a.min.js"]);
    }

    #[test]
    fn question_marks_match_exactly_one_character() {
        let fs = fs(&["/ab.ts", "/abc.ts"]);
        assert_eq!(read(&fs, &["??.ts"], &[]), ["/ab.ts"]);
    }

    #[test]
    fn an_include_outside_the_base_path_is_still_reached() {
        let fs = fs(&["/root/a.ts", "/shared/b.ts"]);
        let found = read_directory(
            &fs,
            "/",
            "/root",
            &[".ts"],
            &[],
            &["**/*".to_string(), "../shared/**/*".to_string()],
        );
        assert_eq!(found, ["/root/a.ts", "/shared/b.ts"]);
    }

    #[test]
    fn a_symlink_cycle_terminates() {
        let fs = InMemoryFileSystem::new(
            [("/a/x.ts".to_string(), String::new())],
            [("/a/self".to_string(), "/a".to_string())],
            true,
        );
        assert_eq!(read(&fs, &["**/*"], &[]), ["/a/x.ts"]);
    }
}
