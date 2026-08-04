//! File extensions, as TypeScript understands them.
//!
//! Ported from `internal/tspath/extension.go` at the pinned commit.
//!
//! # Why an extension is not "the text after the last dot"
//!
//! TypeScript has *composite* extensions — `.d.ts`, `.d.mts`, `.d.cts`, and the
//! open-ended `.d.<anything>.ts` family — and module resolution's central trick is
//! substituting one extension for another (`./foo.js` finds `foo.ts`). Both need a
//! notion of extension that a `rsplit_once('.')` does not give.
//!
//! Upstream keeps *two* answers and uses them in different places, which is worth
//! stating because getting them the wrong way round is silent:
//!
//! - [`try_get_extension_from_path`] and [`remove_file_extension`] match against a
//!   **fixed list** of known extensions, longest-composite first, so `a.d.ts` has
//!   the extension `.d.ts`.
//! - [`get_any_extension_from_path`] is purely lexical — everything from the last
//!   dot in the base name — so the same `a.d.ts` has the extension `.ts`.
//!
//! The resolver uses the first when it wants "strip what TypeScript would emit"
//! and the second when it wants "whatever the user actually wrote".

/// `.ts`.
pub const EXTENSION_TS: &str = ".ts";
/// `.tsx`.
pub const EXTENSION_TSX: &str = ".tsx";
/// `.d.ts`.
pub const EXTENSION_DTS: &str = ".d.ts";
/// `.js`.
pub const EXTENSION_JS: &str = ".js";
/// `.jsx`.
pub const EXTENSION_JSX: &str = ".jsx";
/// `.json`.
pub const EXTENSION_JSON: &str = ".json";
/// `.mjs`.
pub const EXTENSION_MJS: &str = ".mjs";
/// `.mts`.
pub const EXTENSION_MTS: &str = ".mts";
/// `.d.mts`.
pub const EXTENSION_DMTS: &str = ".d.mts";
/// `.cjs`.
pub const EXTENSION_CJS: &str = ".cjs";
/// `.cts`.
pub const EXTENSION_CTS: &str = ".cts";
/// `.d.cts`.
pub const EXTENSION_DCTS: &str = ".d.cts";

/// Declaration extensions, in upstream's order
/// (`tspath.SupportedDeclarationExtensions`).
pub const SUPPORTED_DECLARATION_EXTENSIONS: &[&str] =
    &[EXTENSION_DTS, EXTENSION_DCTS, EXTENSION_DMTS];

/// TypeScript implementation extensions
/// (`tspath.SupportedTSImplementationExtensions`).
pub const SUPPORTED_TS_IMPLEMENTATION_EXTENSIONS: &[&str] =
    &[EXTENSION_TS, EXTENSION_TSX, EXTENSION_MTS, EXTENSION_CTS];

/// JavaScript extensions (`tspath.SupportedJSExtensionsFlat`).
pub const SUPPORTED_JS_EXTENSIONS_FLAT: &[&str] =
    &[EXTENSION_JS, EXTENSION_JSX, EXTENSION_MJS, EXTENSION_CJS];

/// Every TypeScript extension (`tspath.SupportedTSExtensionsFlat`).
pub const SUPPORTED_TS_EXTENSIONS_FLAT: &[&str] = &[
    EXTENSION_TS,
    EXTENSION_TSX,
    EXTENSION_DTS,
    EXTENSION_CTS,
    EXTENSION_DCTS,
    EXTENSION_MTS,
    EXTENSION_DMTS,
];

/// Every TypeScript extension plus `.json`
/// (`tspath.SupportedTSExtensionsWithJsonFlat`).
pub const SUPPORTED_TS_EXTENSIONS_WITH_JSON_FLAT: &[&str] = &[
    EXTENSION_TS,
    EXTENSION_TSX,
    EXTENSION_DTS,
    EXTENSION_CTS,
    EXTENSION_DCTS,
    EXTENSION_MTS,
    EXTENSION_DMTS,
    EXTENSION_JSON,
];

/// The extensions a known-extension lookup strips, longest-composite first
/// (`tspath.extensionsToRemove`).
///
/// The order is load-bearing: `.d.ts` must be tried before `.ts`, or every
/// declaration file would be stripped to `a.d`.
const EXTENSIONS_TO_REMOVE: &[&str] = &[
    EXTENSION_DTS,
    EXTENSION_DMTS,
    EXTENSION_DCTS,
    EXTENSION_MJS,
    EXTENSION_MTS,
    EXTENSION_CJS,
    EXTENSION_CTS,
    EXTENSION_TS,
    EXTENSION_JS,
    EXTENSION_TSX,
    EXTENSION_JSX,
    EXTENSION_JSON,
];

/// The extensions [`try_extract_ts_extension`] recognises
/// (`tspath.supportedTSExtensionsForExtractExtension`).
const SUPPORTED_TS_EXTENSIONS_FOR_EXTRACT_EXTENSION: &[&str] = &[
    EXTENSION_DTS,
    EXTENSION_DCTS,
    EXTENSION_DMTS,
    EXTENSION_TS,
    EXTENSION_TSX,
    EXTENSION_MTS,
    EXTENSION_CTS,
];

/// Whether `path` ends with `extension` (`tspath.FileExtensionIs`).
///
/// Strictly longer, so a file *named* `.ts` is not a `.ts` file.
#[must_use]
pub fn file_extension_is(path: &str, extension: &str) -> bool {
    path.len() > extension.len() && path.ends_with(extension)
}

/// Whether `path` ends with any of `extensions` (`tspath.FileExtensionIsOneOf`).
#[must_use]
pub fn file_extension_is_one_of(path: &str, extensions: &[&str]) -> bool {
    extensions.iter().any(|extension| file_extension_is(path, extension))
}

/// Whether `extension` is one of `extensions` (`tspath.ExtensionIsOneOf`).
#[must_use]
pub fn extension_is_one_of(extension: &str, extensions: &[&str]) -> bool {
    extensions.contains(&extension)
}

/// The known extension `path` ends with, or `""`
/// (`tspath.TryGetExtensionFromPath`).
#[must_use]
pub fn try_get_extension_from_path(path: &str) -> &'static str {
    EXTENSIONS_TO_REMOVE
        .iter()
        .copied()
        .find(|extension| file_extension_is(path, extension))
        .unwrap_or("")
}

/// `path` with its known extension removed (`tspath.RemoveFileExtension`).
///
/// Unlike the lexical [`get_any_extension_from_path`], this strips `.d.ts` whole.
/// A path with no *known* extension is returned unchanged — which is how the
/// resolver detects "this specifier ends in something we do not recognise".
#[must_use]
pub fn remove_file_extension(path: &str) -> &str {
    for extension in EXTENSIONS_TO_REMOVE {
        if let Some(stem) = path.strip_suffix(extension) {
            return stem;
        }
    }
    path
}

/// `path` with `extension` chopped off its end (`tspath.RemoveExtension`).
///
/// The caller is asserting that `path` ends with `extension`; if it does not, the
/// path is returned unchanged rather than panicking, because every caller here
/// obtained `extension` from `path` in the first place.
#[must_use]
pub fn remove_extension<'a>(path: &'a str, extension: &str) -> &'a str {
    path.strip_suffix(extension).unwrap_or(path)
}

/// The TypeScript extension `file_name` ends with, or `""`
/// (`tspath.TryExtractTSExtension`).
#[must_use]
pub fn try_extract_ts_extension(file_name: &str) -> &'static str {
    SUPPORTED_TS_EXTENSIONS_FOR_EXTRACT_EXTENSION
        .iter()
        .copied()
        .find(|extension| file_extension_is(file_name, extension))
        .unwrap_or("")
}

/// Whether `ext` is a TypeScript extension (`tspath.ExtensionIsTs`).
///
/// The trailing clause is the open-ended declaration family: `.d.json.ts`,
/// `.d.css.ts`, and anything else shaped `.d.*.ts`.
// The argument is an *extension*, not a file name, so clippy's
// case-insensitivity advice for `ends_with(".ts")` does not apply: upstream
// compares these case-sensitively and so must we.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
#[must_use]
pub fn extension_is_ts(ext: &str) -> bool {
    matches!(
        ext,
        EXTENSION_TS
            | EXTENSION_TSX
            | EXTENSION_DTS
            | EXTENSION_MTS
            | EXTENSION_DMTS
            | EXTENSION_CTS
            | EXTENSION_DCTS
    ) || (ext.len() >= 7 && ext.starts_with(".d.") && ext.ends_with(".ts"))
}

/// The declaration extension of `file_name`, or `""`
/// (`tspath.GetDeclarationFileExtension`).
///
/// Matched against the *base* name, and the `.d.*.ts` case is found by looking for
/// `.d.` anywhere — so `a.d.json.ts` yields `.d.json.ts`.
#[must_use]
pub fn get_declaration_file_extension(file_name: &str) -> &str {
    let base = crate::get_base_file_name(file_name);
    for extension in SUPPORTED_DECLARATION_EXTENSIONS {
        if base.ends_with(extension) {
            return &base[base.len() - extension.len()..];
        }
    }
    if base.ends_with(EXTENSION_TS)
        && let Some(index) = base.find(".d.")
    {
        return &base[index..];
    }
    ""
}

/// Whether `file_name` is a declaration file (`tspath.IsDeclarationFileName`).
#[must_use]
pub fn is_declaration_file_name(file_name: &str) -> bool {
    !get_declaration_file_extension(file_name).is_empty()
}

/// Whether `path` is a TypeScript *implementation* file
/// (`tspath.HasImplementationTSFileExtension`).
#[must_use]
pub fn has_implementation_ts_file_extension(path: &str) -> bool {
    file_extension_is_one_of(path, SUPPORTED_TS_IMPLEMENTATION_EXTENSIONS)
        && !is_declaration_file_name(path)
}

/// The source extensions an output with this extension could have come from
/// (`tspath.GetPossibleOriginalInputExtensionForExtension`).
#[must_use]
pub fn get_possible_original_input_extension_for_extension(path: &str) -> Vec<String> {
    if file_extension_is_one_of(path, &[EXTENSION_DMTS, EXTENSION_MJS, EXTENSION_MTS]) {
        return vec![EXTENSION_MTS.to_string(), EXTENSION_MJS.to_string()];
    }
    if file_extension_is_one_of(path, &[EXTENSION_DCTS, EXTENSION_CJS, EXTENSION_CTS]) {
        return vec![EXTENSION_CTS.to_string(), EXTENSION_CJS.to_string()];
    }
    // A custom `.d.<inner>.ts` came from `.<inner>`.
    let declaration = get_declaration_file_extension(path);
    if !declaration.is_empty() && declaration != EXTENSION_DTS {
        let inner = &declaration[".d.".len()..declaration.len() - ".ts".len()];
        return vec![format!(".{inner}")];
    }
    vec![
        EXTENSION_TSX.to_string(),
        EXTENSION_TS.to_string(),
        EXTENSION_JSX.to_string(),
        EXTENSION_JS.to_string(),
    ]
}

/// `path` with its known extension replaced (`tspath.ChangeExtension`).
#[must_use]
pub fn change_extension(path: &str, new_extension: &str) -> String {
    let extension = crate::get_any_extension_from_path_with(path, EXTENSIONS_TO_REMOVE);
    if extension.is_empty() {
        return path.to_string();
    }
    let stem = &path[..path.len() - extension.len()];
    if new_extension.is_empty() {
        stem.to_string()
    } else if new_extension.starts_with('.') {
        format!("{stem}{new_extension}")
    } else {
        format!("{stem}.{new_extension}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_known_extension_list_prefers_the_composite_form() {
        // The order in `EXTENSIONS_TO_REMOVE` is the whole point: `.d.ts` before
        // `.ts`, or `a.d.ts` would strip to `a.d`.
        assert_eq!(try_get_extension_from_path("a.d.ts"), ".d.ts");
        assert_eq!(remove_file_extension("a.d.ts"), "a");
        assert_eq!(try_get_extension_from_path("a.d.mts"), ".d.mts");
        assert_eq!(try_get_extension_from_path("a.ts"), ".ts");
    }

    #[test]
    fn an_unrecognised_extension_is_not_stripped() {
        // This is how the resolver tells `./foo.css` apart from `./foo.js`.
        assert_eq!(try_get_extension_from_path("a.css"), "");
        assert_eq!(remove_file_extension("a.css"), "a.css");
    }

    #[test]
    fn a_file_named_exactly_like_an_extension_does_not_have_it() {
        // `FileExtensionIs` is strictly-longer, so `.ts` is a name, not a suffix.
        assert!(!file_extension_is(".ts", ".ts"));
        assert!(file_extension_is("a.ts", ".ts"));
    }

    #[test]
    fn the_open_ended_declaration_family_is_recognised() {
        assert_eq!(get_declaration_file_extension("a.d.json.ts"), ".d.json.ts");
        assert!(extension_is_ts(".d.json.ts"));
        assert!(is_declaration_file_name("a.d.cts"));
        assert!(!is_declaration_file_name("a.cts"));
        assert_eq!(get_possible_original_input_extension_for_extension("a.d.json.ts"), [".json"]);
    }

    #[test]
    fn an_implementation_file_is_not_a_declaration_file() {
        assert!(has_implementation_ts_file_extension("a.ts"));
        assert!(!has_implementation_ts_file_extension("a.d.ts"));
        assert!(!has_implementation_ts_file_extension("a.js"));
    }

    #[test]
    fn changing_an_extension_replaces_the_composite_whole() {
        assert_eq!(change_extension("/a/b.d.ts", ".js"), "/a/b.js");
        assert_eq!(change_extension("/a/b.ts", ".js"), "/a/b.js");
        // Nothing known to strip means nothing changes.
        assert_eq!(change_extension("/a/b.css", ".js"), "/a/b.css");
    }
}
