//! The trace messages module resolution emits.
//!
//! Ported from the subset of `internal/diagnostics/diagnostics_generated.go` that
//! `internal/module` writes, at the pinned commit.
//!
//! # These strings are behaviour, not presentation
//!
//! `--traceResolution` output is baselined: 146 committed `.trace.json` files
//! record the exact text, in order, of every step. A wording change is a
//! conformance failure, so each message carries its upstream diagnostic **code**
//! as the thing to check drift against — codes are stable across localisation
//! and rewording in a way the English text is not.
//!
//! # Why not the whole diagnostics table
//!
//! `tsr-diagnostics` exists and will eventually carry all ~2,000 messages with
//! their categories and 13 locales (bd tsr-5e7.6). These 69 are needed *now*, are
//! all `Message` category, and are only ever emitted in the invariant locale —
//! upstream's baseline tracer calls `msg.Localize(locale.Default, ...)`. Pulling
//! the generated table forward for them would couple this crate to a codegen step
//! that has not been designed yet.

use std::fmt;

/// A trace message template and its upstream diagnostic code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Message {
    /// The upstream diagnostic code, e.g. 6096. Stable across rewording.
    pub code: u32,
    /// The English template, with `{0}`-style placeholders.
    pub text: &'static str,
}

impl Message {
    /// Substitute `args` for `{0}`, `{1}`, … in the template.
    ///
    /// A placeholder with no corresponding argument is left as written, which
    /// makes a miscount visible in the baseline diff rather than silently
    /// producing a plausible line.
    #[must_use]
    pub fn format(&self, args: &[&str]) -> String {
        let mut result = String::with_capacity(self.text.len());
        let mut rest = self.text;
        while let Some(open) = rest.find('{') {
            let Some(close) = rest[open..].find('}').map(|offset| open + offset) else {
                break;
            };
            let Ok(index) = rest[open + 1..close].parse::<usize>() else {
                result.push_str(&rest[..=open]);
                rest = &rest[open + 1..];
                continue;
            };
            result.push_str(&rest[..open]);
            match args.get(index) {
                Some(arg) => result.push_str(arg),
                None => result.push_str(&rest[open..=close]),
            }
            rest = &rest[close + 1..];
        }
        result.push_str(rest);
        result
    }
}

/// One emitted trace line: the message it came from and its rendered text.
///
/// Both are kept because the conformance harness's sanitiser rewrites lines by
/// *text* (mirroring upstream's `TracerForBaselining`), while drift-checking
/// wants the code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trace {
    /// The upstream diagnostic code.
    pub code: u32,
    /// The rendered line.
    pub text: String,
}

impl fmt::Display for Trace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

macro_rules! messages {
    ($($(#[$doc:meta])* $name:ident = $code:literal, $text:literal;)*) => {
        $(
            $(#[$doc])*
            pub const $name: Message = Message { code: $code, text: $text };
        )*

        /// Every message in this module, for the drift check.
        #[cfg(test)]
        const ALL: &[Message] = &[$($name),*];
    };
}

messages! {
    /// `======== Resolving module '{0}' from '{1}'. ========`
    RESOLVING_MODULE_0_FROM_1 = 6086, "======== Resolving module '{0}' from '{1}'. ========";
    /// The resolution kind was derived from `module`.
    MODULE_RESOLUTION_KIND_IS_NOT_SPECIFIED_USING_0 = 6088, "Module resolution kind is not specified, using '{0}'.";
    /// The resolution kind was set explicitly.
    EXPLICITLY_SPECIFIED_MODULE_RESOLUTION_KIND_COLON_0 = 6087, "Explicitly specified module resolution kind: '{0}'.";
    /// Success.
    MODULE_NAME_0_WAS_SUCCESSFULLY_RESOLVED_TO_1 = 6089, "======== Module name '{0}' was successfully resolved to '{1}'. ========";
    /// Success, from inside a package.
    MODULE_NAME_0_WAS_SUCCESSFULLY_RESOLVED_TO_1_WITH_PACKAGE_ID_2 = 6218, "======== Module name '{0}' was successfully resolved to '{1}' with Package ID '{2}'. ========";
    /// Failure.
    MODULE_NAME_0_WAS_NOT_RESOLVED = 6090, "======== Module name '{0}' was not resolved. ========";
    /// A `paths` lookup is starting.
    X_PATHS_OPTION_IS_SPECIFIED_LOOKING_FOR_A_PATTERN_TO_MATCH_MODULE_NAME_0 = 6091, "'paths' option is specified, looking for a pattern to match module name '{0}'.";
    /// A `paths` or `typesVersions` pattern matched.
    MODULE_NAME_0_MATCHED_PATTERN_1 = 6092, "Module name '{0}', matched pattern '{1}'.";
    /// One substitution from a matched pattern.
    TRYING_SUBSTITUTION_0_CANDIDATE_MODULE_LOCATION_COLON_1 = 6093, "Trying substitution '{0}', candidate module location: '{1}'.";
    /// A file-or-directory probe.
    LOADING_MODULE_AS_FILE_SLASH_FOLDER_CANDIDATE_MODULE_LOCATION_0_TARGET_FILE_TYPES_COLON_1 = 6095, "Loading module as file / folder, candidate module location '{0}', target file types: {1}.";
    /// A file probe that missed.
    FILE_0_DOES_NOT_EXIST = 6096, "File '{0}' does not exist.";
    /// A file probe that hit.
    FILE_0_EXISTS_USE_IT_AS_A_NAME_RESOLUTION_RESULT = 6097, "File '{0}' exists - use it as a name resolution result.";
    /// A bare specifier is going to `node_modules`.
    LOADING_MODULE_0_FROM_NODE_MODULES_FOLDER_TARGET_FILE_TYPES_COLON_1 = 6098, "Loading module '{0}' from 'node_modules' folder, target file types: {1}.";
    /// A `package.json` was read.
    FOUND_PACKAGE_JSON_AT_0 = 6099, "Found 'package.json' at '{0}'.";
    /// A `package.json` field is absent.
    X_PACKAGE_JSON_DOES_NOT_HAVE_A_0_FIELD = 6100, "'package.json' does not have a '{0}' field.";
    /// A `package.json` path field points somewhere.
    X_PACKAGE_JSON_HAS_0_FIELD_1_THAT_REFERENCES_2 = 6101, "'package.json' has '{0}' field '{1}' that references '{2}'.";
    /// A `rootDirs` prefix test.
    CHECKING_IF_0_IS_THE_LONGEST_MATCHING_PREFIX_FOR_1_2 = 6104, "Checking if '{0}' is the longest matching prefix for '{1}' - '{2}'.";
    /// A `package.json` field had the wrong JSON type.
    EXPECTED_TYPE_OF_0_FIELD_IN_PACKAGE_JSON_TO_BE_1_GOT_2 = 6105, "Expected type of '{0}' field in 'package.json' to be '{1}', got '{2}'.";
    /// A `rootDirs` lookup is starting.
    X_ROOTDIRS_OPTION_IS_SET_USING_IT_TO_RESOLVE_RELATIVE_MODULE_NAME_0 = 6107, "'rootDirs' option is set, using it to resolve relative module name '{0}'.";
    /// The winning `rootDirs` prefix.
    LONGEST_MATCHING_PREFIX_FOR_0_IS_1 = 6108, "Longest matching prefix for '{0}' is '{1}'.";
    /// One `rootDirs` candidate.
    LOADING_0_FROM_THE_ROOT_DIR_1_CANDIDATE_LOCATION_2 = 6109, "Loading '{0}' from the root dir '{1}', candidate location '{2}'.";
    /// Falling through to the other `rootDirs` entries.
    TRYING_OTHER_ENTRIES_IN_ROOTDIRS = 6110, "Trying other entries in 'rootDirs'.";
    /// Every `rootDirs` entry missed.
    MODULE_RESOLUTION_USING_ROOTDIRS_HAS_FAILED = 6111, "Module resolution using 'rootDirs' has failed.";
    /// A type reference directive is being resolved.
    RESOLVING_TYPE_REFERENCE_DIRECTIVE_0_CONTAINING_FILE_1_ROOT_DIRECTORY_2 = 6116, "======== Resolving type reference directive '{0}', containing file '{1}', root directory '{2}'. ========";
    /// Success.
    TYPE_REFERENCE_DIRECTIVE_0_WAS_SUCCESSFULLY_RESOLVED_TO_1_PRIMARY_COLON_2 = 6119, "======== Type reference directive '{0}' was successfully resolved to '{1}', primary: {2}. ========";
    /// Failure.
    TYPE_REFERENCE_DIRECTIVE_0_WAS_NOT_RESOLVED = 6120, "======== Type reference directive '{0}' was not resolved. ========";
    /// The `typeRoots` pass is starting.
    RESOLVING_WITH_PRIMARY_SEARCH_PATH_0 = 6121, "Resolving with primary search path '{0}'.";
    /// There are no `typeRoots` to search.
    ROOT_DIRECTORY_CANNOT_BE_DETERMINED_SKIPPING_PRIMARY_SEARCH_PATHS = 6122, "Root directory cannot be determined, skipping primary search paths.";
    /// The `node_modules` pass is starting.
    LOOKING_UP_IN_NODE_MODULES_FOLDER_INITIAL_LOCATION_0 = 6125, "Looking up in 'node_modules' folder, initial location '{0}'.";
    /// A symlink was followed.
    RESOLVING_REAL_PATH_FOR_0_RESULT_1 = 6130, "Resolving real path for '{0}', result '{1}'.";
    /// The specifier's extension is being substituted.
    FILE_NAME_0_HAS_A_1_EXTENSION_STRIPPING_IT = 6132, "File name '{0}' has a '{1}' extension - stripping it.";
    /// An extra pass through a typings cache.
    AUTO_DISCOVERY_FOR_TYPINGS_IS_ENABLED_IN_PROJECT_0_RUNNING_EXTRA_RESOLUTION_PASS_FOR_MODULE_1_USING_CACHE_LOCATION_2 = 6140, "Auto discovery for typings is enabled in project '{0}'. Running extra resolution pass for module '{1}' using cache location '{2}'.";
    /// A directory probe that missed.
    DIRECTORY_0_DOES_NOT_EXIST_SKIPPING_ALL_LOOKUPS_IN_IT = 6148, "Directory '{0}' does not exist, skipping all lookups in it.";
    /// The specifier looks like a URL.
    SKIPPING_MODULE_0_THAT_LOOKS_LIKE_AN_ABSOLUTE_URI_TARGET_FILE_TYPES_COLON_1 = 6164, "Skipping module '{0}' that looks like an absolute URI, target file types: {1}.";
    /// An `@scope/name` package was mangled to `scope__name`.
    SCOPED_PACKAGE_DETECTED_LOOKING_IN_0 = 6182, "Scoped package detected, looking in '{0}'";
    /// `typesVersions` is present.
    X_PACKAGE_JSON_HAS_A_TYPESVERSIONS_FIELD_WITH_VERSION_SPECIFIC_PATH_MAPPINGS = 6206, "'package.json' has a 'typesVersions' field with version-specific path mappings.";
    /// No `typesVersions` entry matched.
    X_PACKAGE_JSON_DOES_NOT_HAVE_A_TYPESVERSIONS_ENTRY_THAT_MATCHES_VERSION_0 = 6207, "'package.json' does not have a 'typesVersions' entry that matches version '{0}'.";
    /// A `typesVersions` entry matched.
    X_PACKAGE_JSON_HAS_A_TYPESVERSIONS_ENTRY_0_THAT_MATCHES_COMPILER_VERSION_1_LOOKING_FOR_A_PATTERN_TO_MATCH_MODULE_NAME_2 = 6208, "'package.json' has a 'typesVersions' entry '{0}' that matches compiler version '{1}', looking for a pattern to match module name '{2}'.";
    /// A `typesVersions` key was not a semver range.
    X_PACKAGE_JSON_HAS_A_TYPESVERSIONS_ENTRY_0_THAT_IS_NOT_A_VALID_SEMVER_RANGE = 6209, "'package.json' has a 'typesVersions' entry '{0}' that is not a valid semver range.";
    /// Options came from a project reference.
    USING_COMPILER_OPTIONS_OF_PROJECT_REFERENCE_REDIRECT_0 = 6215, "Using compiler options of project reference redirect '{0}'.";
    /// Success, from inside a package.
    TYPE_REFERENCE_DIRECTIVE_0_WAS_SUCCESSFULLY_RESOLVED_TO_1_WITH_PACKAGE_ID_2_PRIMARY_COLON_3 = 6219, "======== Type reference directive '{0}' was successfully resolved to '{1}' with Package ID '{2}', primary: {3}. ========";
    /// A `package.json` path field was present but empty.
    X_PACKAGE_JSON_HAD_A_FALSY_0_FIELD = 6220, "'package.json' had a falsy '{0}' field.";
    /// The `package.json` existence cache answered.
    FILE_0_EXISTS_ACCORDING_TO_EARLIER_CACHED_LOOKUPS = 6239, "File '{0}' exists according to earlier cached lookups.";
    /// The `package.json` existence cache answered.
    FILE_0_DOES_NOT_EXIST_ACCORDING_TO_EARLIER_CACHED_LOOKUPS = 6240, "File '{0}' does not exist according to earlier cached lookups.";
    /// `typeRoots` was configured, so `node_modules` is skipped.
    RESOLVING_TYPE_REFERENCE_DIRECTIVE_FOR_PROGRAM_THAT_SPECIFIES_CUSTOM_TYPEROOTS_SKIPPING_LOOKUP_IN_NODE_MODULES_FOLDER = 6265, "Resolving type reference directive for program that specifies custom typeRoots, skipping lookup in 'node_modules' folder.";
    /// A `#`-specifier has no package scope.
    DIRECTORY_0_HAS_NO_CONTAINING_PACKAGE_JSON_SCOPE_IMPORTS_WILL_NOT_RESOLVE = 6270, "Directory '{0}' has no containing package.json scope. Imports will not resolve.";
    /// A `#`-specifier was not in `imports`.
    IMPORT_SPECIFIER_0_DOES_NOT_EXIST_IN_PACKAGE_JSON_SCOPE_AT_PATH_1 = 6271, "Import specifier '{0}' does not exist in package.json scope at path '{1}'.";
    /// `#` or `#/` alone.
    INVALID_IMPORT_SPECIFIER_0_HAS_NO_POSSIBLE_RESOLUTIONS = 6272, "Invalid import specifier '{0}' has no possible resolutions.";
    /// The scope has no `imports`.
    X_PACKAGE_JSON_SCOPE_0_HAS_NO_IMPORTS_DEFINED = 6273, "package.json scope '{0}' has no imports defined.";
    /// An `exports`/`imports` target is `null`.
    X_PACKAGE_JSON_SCOPE_0_EXPLICITLY_MAPS_SPECIFIER_1_TO_NULL = 6274, "package.json scope '{0}' explicitly maps specifier '{1}' to null.";
    /// An `exports`/`imports` target had an unusable shape. Note the missing
    /// full stop: it is upstream's, and the baselines record it.
    X_PACKAGE_JSON_SCOPE_0_HAS_INVALID_TYPE_FOR_TARGET_OF_SPECIFIER_1 = 6275, "package.json scope '{0}' has invalid type for target of specifier '{1}'";
    /// The subpath was not in `exports`.
    EXPORT_SPECIFIER_0_DOES_NOT_EXIST_IN_PACKAGE_JSON_SCOPE_AT_PATH_1 = 6276, "Export specifier '{0}' does not exist in package.json scope at path '{1}'.";
    /// The `node10Result` fallback pass.
    RESOLUTION_OF_NON_RELATIVE_NAME_FAILED_TRYING_WITH_MODERN_NODE_RESOLUTION_FEATURES_DISABLED_TO_SEE_IF_NPM_LIBRARY_NEEDS_CONFIGURATION_UPDATE = 6277, "Resolution of non-relative name failed; trying with modern Node resolution features disabled to see if npm library needs configuration update.";
    /// `peerDependencies` is present, so the package id gains a suffix.
    X_PACKAGE_JSON_HAS_A_PEERDEPENDENCIES_FIELD = 6281, "'package.json' has a 'peerDependencies' field.";
    /// A peer dependency was found.
    FOUND_PEERDEPENDENCY_0_WITH_1_VERSION = 6282, "Found peerDependency '{0}' with '{1}' version.";
    /// A peer dependency was not found.
    FAILED_TO_FIND_PEERDEPENDENCY_0 = 6283, "Failed to find peerDependency '{0}'.";
    /// The resolution mode and its conditions.
    RESOLVING_IN_0_MODE_WITH_CONDITIONS_1 = 6402, "Resolving in {0} mode with conditions {1}.";
    /// A condition matched.
    MATCHED_0_CONDITION_1 = 6403, "Matched '{0}' condition '{1}'.";
    /// An `exports`/`imports` subpath was selected.
    USING_0_SUBPATH_1_WITH_TARGET_2 = 6404, "Using '{0}' subpath '{1}' with target '{2}'.";
    /// A condition did not match.
    SAW_NON_MATCHING_CONDITION_0 = 6405, "Saw non-matching condition '{0}'.";
    /// Entering a conditional `exports` object.
    ENTERING_CONDITIONAL_EXPORTS = 6413, "Entering conditional exports.";
    /// A matched condition produced a resolution.
    RESOLVED_UNDER_CONDITION_0 = 6414, "Resolved under condition '{0}'.";
    /// A matched condition produced nothing.
    FAILED_TO_RESOLVE_UNDER_CONDITION_0 = 6415, "Failed to resolve under condition '{0}'.";
    /// Leaving a conditional `exports` object.
    EXITING_CONDITIONAL_EXPORTS = 6416, "Exiting conditional exports.";
    /// The first `node_modules` pass, for TypeScript and declarations.
    SEARCHING_ALL_ANCESTOR_NODE_MODULES_DIRECTORIES_FOR_PREFERRED_EXTENSIONS_COLON_0 = 6417, "Searching all ancestor node_modules directories for preferred extensions: {0}.";
    /// The second `node_modules` pass, for JavaScript and JSON.
    SEARCHING_ALL_ANCESTOR_NODE_MODULES_DIRECTORIES_FOR_FALLBACK_EXTENSIONS_COLON_0 = 6418, "Searching all ancestor node_modules directories for fallback extensions: {0}.";
    /// An `exports` entry needs a project root that was not supplied.
    THE_PROJECT_ROOT_IS_AMBIGUOUS_BUT_IS_REQUIRED_TO_RESOLVE_EXPORT_MAP_ENTRY_0_IN_FILE_1 = 2209, "The project root is ambiguous, but is required to resolve export map entry '{0}' in file '{1}'. Supply the `rootDir` compiler option to disambiguate.";
    /// An `imports` entry needs a project root that was not supplied.
    THE_PROJECT_ROOT_IS_AMBIGUOUS_BUT_IS_REQUIRED_TO_RESOLVE_IMPORT_MAP_ENTRY_0_IN_FILE_1 = 2210, "The project root is ambiguous, but is required to resolve import map entry '{0}' in file '{1}'. Supply the `rootDir` compiler option to disambiguate.";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_substituted_in_order() {
        assert_eq!(FILE_0_DOES_NOT_EXIST.format(&["/a.ts"]), "File '/a.ts' does not exist.");
        assert_eq!(
            RESOLVING_MODULE_0_FROM_1.format(&["./a", "/b.ts"]),
            "======== Resolving module './a' from '/b.ts'. ========"
        );
    }

    #[test]
    fn a_missing_argument_leaves_the_placeholder_visible() {
        // Deliberate: a silently-dropped argument would produce a line that looks
        // right and diffs wrong, with nothing pointing at the cause.
        assert_eq!(
            RESOLVING_MODULE_0_FROM_1.format(&["./a"]),
            "======== Resolving module './a' from '{1}'. ========"
        );
    }

    #[test]
    fn every_message_has_a_distinct_upstream_code() {
        // The codes are what a drift check compares against upstream; a
        // copy-paste that duplicated one would make that check useless.
        let mut codes: Vec<u32> = ALL.iter().map(|message| message.code).collect();
        codes.sort_unstable();
        let count = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), count, "duplicate diagnostic code among trace messages");
    }
}
