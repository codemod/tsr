//! Turning `files`/`include`/`exclude` into the program's root file list.
//!
//! Ported from `internal/tsoptions/tsconfigparsing.go`
//! (`getFileNamesFromConfigSpecs`, `hasFileWithHigherPriorityExtension`,
//! `removeWildcardFilesWithLowerPriorityExtension`, `GetSupportedExtensions`)
//! at the pinned commit.
//!
//! # The rule that is not obvious
//!
//! A wildcard must not pull in a file's own compiler *output*. Given `a.ts` and
//! `a.js` side by side, `include: ["**/*"]` with `allowJs` takes only `a.ts` —
//! the `.js` is assumed to be what `tsc` last emitted. That is what the two
//! priority functions here do, and it works in both directions because the walk
//! finds files in directory order rather than in extension order:
//!
//! - **Higher priority already present** — skip this file.
//! - **Lower priority already present** — remove it.
//!
//! Files named explicitly in `files` are exempt: a literal entry is never
//! removed and never suppressed.

use tsr_core::{CompilerOptions, OrderedMap};
use tsr_path::{
    extension::{
        ALL_SUPPORTED_EXTENSIONS, ALL_SUPPORTED_EXTENSIONS_WITH_JSON, EXTENSION_DTS, EXTENSION_JS,
        EXTENSION_JSON, EXTENSION_JSX, EXTENSION_TS, SUPPORTED_TS_EXTENSIONS,
        SUPPORTED_TS_EXTENSIONS_WITH_JSON, change_extension, file_extension_is,
        file_extension_is_one_of,
    },
    get_canonical_file_name, get_normalized_absolute_path, normalize_path,
};
use tsr_vfs::{
    FileSystem,
    glob::{SpecMatcher, Usage, read_directory},
};

/// Which extensions a program will load, grouped by format family
/// (`tsoptions.GetSupportedExtensions`).
#[must_use]
pub fn supported_extensions(options: &CompilerOptions) -> &'static [&'static [&'static str]] {
    if options.get_allow_js() { ALL_SUPPORTED_EXTENSIONS } else { SUPPORTED_TS_EXTENSIONS }
}

/// [`supported_extensions`] plus `.json` when `resolveJsonModule` is on
/// (`tsoptions.GetSupportedExtensionsWithJsonIfResolveJsonModule`).
#[must_use]
pub fn supported_extensions_with_json(
    options: &CompilerOptions,
) -> &'static [&'static [&'static str]] {
    if !options.get_resolve_json_module() {
        supported_extensions(options)
    } else if options.get_allow_js() {
        ALL_SUPPORTED_EXTENSIONS_WITH_JSON
    } else {
        SUPPORTED_TS_EXTENSIONS_WITH_JSON
    }
}

/// The `files`, `include` and `exclude` a config resolved to
/// (`tsoptions.configFileSpecs`, reduced to the validated lists).
#[derive(Debug, Default, Clone)]
pub struct ConfigFileSpecs {
    /// Files named one by one. Always included, whatever `exclude` says.
    pub files: Vec<String>,
    /// Wildcards to include.
    pub include: Vec<String>,
    /// Wildcards to exclude.
    pub exclude: Vec<String>,
}

/// Expand the specs into a root file list (`getFileNamesFromConfigSpecs`).
///
/// Returns the file names and how many of them came from `files`, which is the
/// prefix a caller may not reorder.
#[must_use]
pub fn expand(
    specs: &ConfigFileSpecs,
    base_path: &str,
    options: &CompilerOptions,
    fs: &dyn FileSystem,
) -> (Vec<String>, usize) {
    let base_path = normalize_path(base_path);
    let case_sensitive = fs.use_case_sensitive_file_names();
    let key = |value: &str| get_canonical_file_name(value, case_sensitive);

    // Three maps rather than one, because they have different rules: literal
    // files are immune to everything, wildcard files displace each other by
    // extension priority, and `.json` files only enter through an include
    // pattern that names `.json` explicitly.
    let mut literal: OrderedMap<String> = OrderedMap::new();
    let mut wildcard: OrderedMap<String> = OrderedMap::new();
    let mut wildcard_json: OrderedMap<String> = OrderedMap::new();

    for file_name in &specs.files {
        literal.set(key(file_name), get_normalized_absolute_path(file_name, &base_path));
    }

    if !specs.include.is_empty() {
        let extensions = supported_extensions(options);
        let extensions_with_json: Vec<&str> = supported_extensions_with_json(options).concat();
        let mut json_matcher: Option<Option<SpecMatcher>> = None;

        for file in read_directory(
            fs,
            &base_path,
            &base_path,
            &extensions_with_json,
            &specs.exclude,
            &specs.include,
        ) {
            if file_extension_is(&file, EXTENSION_JSON) {
                // A `.json` file is only included by a pattern that asks for
                // one: `**/*` does not sweep up every `package.json` in sight.
                let matcher = json_matcher.get_or_insert_with(|| {
                    let json_includes: Vec<String> = specs
                        .include
                        .iter()
                        .filter(|include| include.ends_with(EXTENSION_JSON))
                        .cloned()
                        .collect();
                    SpecMatcher::new(&json_includes, &base_path, Usage::Files, case_sensitive)
                });
                if matcher.as_ref().is_some_and(|matcher| matcher.matches(&file)) {
                    let file_key = key(&file);
                    if !literal.contains_key(&file_key) && !wildcard_json.contains_key(&file_key) {
                        wildcard_json.set(file_key, file);
                    }
                }
                continue;
            }

            if has_file_with_higher_priority_extension(&file, extensions, |candidate| {
                let candidate = key(candidate);
                literal.contains_key(&candidate) || wildcard.contains_key(&candidate)
            }) {
                continue;
            }
            remove_wildcard_files_with_lower_priority_extension(
                &file,
                &mut wildcard,
                extensions,
                key,
            );

            let file_key = key(&file);
            if !literal.contains_key(&file_key) && !wildcard.contains_key(&file_key) {
                wildcard.set(file_key, file);
            }
        }
    }

    let literal_count = literal.len();
    let files = literal
        .into_values()
        .chain(wildcard.into_values())
        .chain(wildcard_json.into_values())
        .collect();
    (files, literal_count)
}

/// Whether a file that would take precedence over `file` is already included
/// (`hasFileWithHigherPriorityExtension`).
fn has_file_with_higher_priority_extension(
    file: &str,
    extensions: &[&[&str]],
    has_file: impl Fn(&str) -> bool,
) -> bool {
    let group = extension_group(file, extensions);
    if group.is_empty() {
        return false;
    }
    for extension in group {
        // Within a group the order *is* the priority, so reaching this file's
        // own extension means nothing higher exists. The `.d.ts` guard is
        // upstream's: `a.d.ts` ends with `.ts`, and treating it as a `.ts` here
        // would make a declaration file suppress its own implementation.
        if file_extension_is(file, extension)
            && (extension != EXTENSION_TS || !file_extension_is(file, EXTENSION_DTS))
        {
            return false;
        }
        if has_file(&change_extension(file, extension)) {
            // LEGACY BEHAVIOUR, upstream's word: an off-by-one in the original
            // extension-priority table let a declaration file be loaded
            // alongside its `.js(x)` counterpart. Upstream keeps it to avoid
            // breaking projects that depend on it, and so does this — dropping
            // it would silently change which files a real project compiles.
            if extension == EXTENSION_DTS
                && (file_extension_is(file, EXTENSION_JS) || file_extension_is(file, EXTENSION_JSX))
            {
                continue;
            }
            return true;
        }
    }
    false
}

/// Drop any already-included file that `file` takes precedence over
/// (`removeWildcardFilesWithLowerPriorityExtension`).
fn remove_wildcard_files_with_lower_priority_extension(
    file: &str,
    wildcard_files: &mut OrderedMap<String>,
    extensions: &[&[&str]],
    key: impl Fn(&str) -> String,
) {
    let group = extension_group(file, extensions);
    // Walking backwards: everything after this file's own extension in the
    // group is lower priority, and nothing before it is.
    for extension in group.iter().rev().copied() {
        if file_extension_is(file, extension) {
            return;
        }
        wildcard_files.remove(&key(&change_extension(file, extension)));
    }
}

/// The extension groups `file` belongs to, flattened.
///
/// A file can only be in one group — the groups partition by format family —
/// but upstream concatenates rather than stopping, and the concatenation is
/// what the priority order is defined over.
fn extension_group<'a>(file: &str, extensions: &'a [&'a [&'a str]]) -> Vec<&'a str> {
    extensions
        .iter()
        .filter(|group| file_extension_is_one_of(file, group))
        .flat_map(|group| group.iter().copied())
        .collect()
}

#[cfg(test)]
mod tests {
    use tsr_core::Tristate;
    use tsr_vfs::InMemoryFileSystem;

    use super::*;

    fn fs(paths: &[&str]) -> InMemoryFileSystem {
        InMemoryFileSystem::new(
            paths.iter().map(|path| ((*path).to_string(), String::new())),
            [],
            true,
        )
    }

    fn specs(files: &[&str], include: &[&str], exclude: &[&str]) -> ConfigFileSpecs {
        let owned = |values: &[&str]| values.iter().map(|v| (*v).to_string()).collect();
        ConfigFileSpecs { files: owned(files), include: owned(include), exclude: owned(exclude) }
    }

    #[test]
    fn a_wildcard_does_not_pull_in_a_files_own_output() {
        // Both directions of the priority rule, in one file system: `a.js`
        // comes before `a.ts` in directory order and must lose anyway.
        let fs = fs(&["/a.js", "/a.ts", "/b.ts", "/b.js"]);
        let options = CompilerOptions { allow_js: Tristate::True, ..CompilerOptions::default() };
        let (files, literal) = expand(&specs(&[], &["**/*"], &[]), "/", &options, &fs);
        assert_eq!(files, ["/a.ts", "/b.ts"]);
        assert_eq!(literal, 0);
    }

    #[test]
    fn a_declaration_file_still_accompanies_its_javascript() {
        // Upstream's named legacy behaviour, kept deliberately.
        let fs = fs(&["/a.js", "/a.d.ts"]);
        let options = CompilerOptions { allow_js: Tristate::True, ..CompilerOptions::default() };
        let (files, _) = expand(&specs(&[], &["**/*"], &[]), "/", &options, &fs);
        // Both survive, in directory order — the point is that neither
        // suppresses the other, which is what the exemption buys.
        assert_eq!(files, ["/a.d.ts", "/a.js"]);
    }

    #[test]
    fn a_literal_file_survives_an_exclude_and_leads_the_list() {
        let fs = fs(&["/excluded/a.ts", "/b.ts"]);
        let (files, literal) = expand(
            &specs(&["excluded/a.ts"], &["**/*"], &["excluded"]),
            "/",
            &CompilerOptions::default(),
            &fs,
        );
        assert_eq!(files, ["/excluded/a.ts", "/b.ts"]);
        assert_eq!(literal, 1);
    }

    #[test]
    fn json_enters_only_through_a_pattern_that_names_it() {
        let fs = fs(&["/a.ts", "/data.json"]);
        let options =
            CompilerOptions { resolve_json_module: Tristate::True, ..CompilerOptions::default() };
        assert_eq!(expand(&specs(&[], &["**/*"], &[]), "/", &options, &fs).0, ["/a.ts"]);
        assert_eq!(
            expand(&specs(&[], &["**/*", "**/*.json"], &[]), "/", &options, &fs).0,
            ["/a.ts", "/data.json"]
        );
    }

    #[test]
    fn without_allow_js_a_javascript_file_is_not_a_root() {
        let fs = fs(&["/a.ts", "/b.js"]);
        assert_eq!(
            expand(&specs(&[], &["**/*"], &[]), "/", &CompilerOptions::default(), &fs).0,
            ["/a.ts"]
        );
    }
}
