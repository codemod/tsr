//! Reading a `tsconfig.json` into compiler options and a root file list.
//!
//! Ported from `internal/tsoptions` at the pinned commit — the part of it that
//! answers the two questions a `Program` asks: *what options*, and *which
//! files*. Upstream's package is 8,706 lines; most of that is the command-line
//! parser, `tsc --init`, `--showConfig`, `--build` mode, watch options, and the
//! declarations for the ~130 options this port does not yet have fields for.
//!
//! # Where this sits
//!
//! ```text
//! tsconfig.json ──▶ tsr-parser (ScriptKind::Json) ──▶ AST
//!                                                      │
//!                       tsr-tsoptions ◀────────────────┘
//!                            │
//!         CompilerOptions ───┴─── root file names ──▶ tsr-compiler's FileLoader
//! ```
//!
//! [ADR-0017](../../../docs/adr/0017-program-before-tsconfig.md) built the
//! `Program` before this deliberately, so `CompilerOptions` could be a plain
//! struct that a config parser *produces* rather than something the parser
//! defines. That ordering is why this crate is additive: nothing below it
//! changed to accommodate it.
//!
//! # What is here and what is not
//!
//! Here: the config file's own options, `files`/`include`/`exclude` expansion
//! against the file system, and the extension-priority rules that keep a
//! project from compiling its own output.
//!
//! Not here, and each is a named gap rather than an oversight:
//!
//! - **`extends`.** 6 of the corpus's 130 `tsconfig.json` units use it and none
//!   of the 20 that the resolution baselines judge. It needs a resolution stack,
//!   circularity detection, and — in its package form — a module resolution of
//!   its own, which would make config parsing depend on `tsr-module`. bd
//!   tsr-9or.6.
//! - **Project references.** They belong with the rest of the project-reference
//!   machinery the file loader also does not have.
//! - **The command line.** `tsc` does not exist yet (Phase 6).
//! - **Config diagnostics.** Errors are collected and returned, but no suite
//!   judges them, and the spans are the value's rather than upstream's exact
//!   `CreateDiagnosticForNodeInSourceFile` range.

pub mod declarations;
pub mod file_names;
pub mod value;

use tsr_core::{CompilerOptions, OrderedMap};
use tsr_diagnostics::{Diagnostic, messages};
use tsr_parser::{ParseOptions, ScriptKind, parse_with_options};
use tsr_path::{
    get_directory_path, get_normalized_absolute_path, normalize_path, normalize_slashes,
};
use tsr_vfs::FileSystem;

use crate::{
    declarations::OptionDeclaration,
    file_names::ConfigFileSpecs,
    value::{ConfigProperty, ConfigValue},
};

/// The default `include` when a config gives neither `files` nor `include`
/// (`tsoptions.defaultIncludeSpec`).
const DEFAULT_INCLUDE_SPEC: &str = "**/*";

/// A parsed config file (`tsoptions.ParsedCommandLine`).
#[derive(Debug, Default)]
pub struct ParsedCommandLine {
    /// The options the config declares.
    pub compiler_options: CompilerOptions,
    /// The program's root files, absolute, `files` entries first.
    pub file_names: Vec<String>,
    /// How many of [`Self::file_names`] came from `files` rather than a
    /// wildcard.
    pub literal_file_count: usize,
    /// The config as written, for the keys that are not compiler options.
    pub raw: OrderedMap<ConfigValue>,
    /// What was wrong with it.
    pub errors: Vec<Diagnostic>,
}

/// Parse `text` as the config file at `config_file_name`
/// (`ParseJsonSourceFileConfigFileContent`).
///
/// `base_path` is what a relative path in the config resolves against, which is
/// the config's own directory.
#[must_use]
pub fn parse_config_file(
    config_file_name: &str,
    text: &str,
    base_path: &str,
    fs: &dyn FileSystem,
) -> ParsedCommandLine {
    let arena = tsr_core::Arena::new();
    // The *parser* reads it, not a JSON library: a `tsconfig.json` may carry
    // comments and trailing commas, and every option error points at a span.
    // See `tsr_parser::json`.
    let parsed = parse_with_options(
        &arena,
        text,
        ParseOptions { script_kind: ScriptKind::Json, ..ParseOptions::default() },
    );

    let base_path_for_file_names = if config_file_name.is_empty() {
        normalize_path(base_path)
    } else {
        normalize_path(&directory_of_combined_path(config_file_name, base_path))
    };

    let mut config = Config::default();
    if let Some(properties) = value::root_properties(parsed.source_file, &parsed.nodes) {
        config.read(&properties, &base_path_for_file_names, &parsed.nodes);
    }
    let Config { mut compiler_options, raw, mut errors } = config;
    if !config_file_name.is_empty() {
        compiler_options.config_file_path = normalize_slashes(config_file_name);
    }
    // `paths` substitutions resolve against the config's directory, and nothing
    // else knows where that was.
    if !compiler_options.paths.is_empty() {
        compiler_options.paths_base_path.clone_from(&base_path_for_file_names);
    }

    let specs = file_specs(&raw, &compiler_options, config_file_name, &mut errors);
    let (file_names, literal_file_count) =
        file_names::expand(&specs, &base_path_for_file_names, &compiler_options, fs);

    if file_names.is_empty() && can_report_no_input_files(&raw) {
        errors.push(Diagnostic::with_args(
            &messages::NO_INPUTS_WERE_FOUND_IN_CONFIG_FILE_0_SPECIFIED_INCLUDE_PATHS_WERE_1_AND_EXCLUDE_PATHS_WERE_2,
            tsr_core::Span::default(),
            [config_file_name.to_string(), quoted_list(&specs.include), quoted_list(&specs.exclude)],
        ));
    }

    ParsedCommandLine { compiler_options, file_names, literal_file_count, raw, errors }
}

/// What reading the config's own properties produced
/// (`tsoptions.parsedTsconfig`).
#[derive(Default)]
struct Config {
    compiler_options: CompilerOptions,
    raw: OrderedMap<ConfigValue>,
    errors: Vec<Diagnostic>,
}

impl Config {
    /// `parseOwnConfigOfJsonSourceFile`, for the root object's properties.
    fn read(
        &mut self,
        properties: &[ConfigProperty<'_>],
        base_path: &str,
        nodes: &tsr_ast::NodeTable,
    ) {
        for property in properties {
            // Every root key is kept raw — `files`, `include`, `exclude` and
            // `references` are not compiler options and are read from here.
            self.raw.set(property.name.clone(), property.value.clone());

            if property.name != "compilerOptions" {
                continue;
            }
            let Some(tsr_ast::Expression::ObjectLiteralExpression(object)) =
                property.value_expression
            else {
                // `"compilerOptions": 1`. Upstream reports the type error and
                // carries on with no options, which is what an empty read does.
                continue;
            };
            for option in value::properties_of(object, nodes) {
                self.set_compiler_option(&option, base_path);
            }
        }
    }

    /// One `compilerOptions` entry (`onPropertySet` → `ParseCompilerOptions`).
    fn set_compiler_option(&mut self, property: &ConfigProperty<'_>, base_path: &str) {
        let Some(declaration) = declarations::find(&property.name) else {
            self.errors.push(Diagnostic::with_args(
                &messages::UNKNOWN_COMPILER_OPTION_0,
                property.span,
                [property.name.clone()],
            ));
            return;
        };
        // A path option is made absolute against the config's directory
        // *before* it is stored, so nothing downstream needs to know where the
        // config was (`normalizeNonListOptionValue`).
        let value = normalize_option_value(declaration, &property.value, base_path);
        if !(declaration.apply)(&mut self.compiler_options, &value) {
            self.errors.push(Diagnostic::with_args(
                &messages::COMPILER_OPTION_0_REQUIRES_A_VALUE_OF_TYPE_1,
                property.span,
                [property.name.clone(), option_type_name(declaration)],
            ));
        }
    }
}

/// Make a file-path option absolute (`normalizeNonListOptionValue`).
fn normalize_option_value(
    declaration: &OptionDeclaration,
    value: &ConfigValue,
    base_path: &str,
) -> ConfigValue {
    if !declaration.is_file_path {
        return value.clone();
    }
    match value {
        ConfigValue::String(path) => ConfigValue::String(absolute(path, base_path)),
        ConfigValue::List(entries) => ConfigValue::List(
            entries
                .iter()
                .map(|entry| match entry {
                    ConfigValue::String(path) => ConfigValue::String(absolute(path, base_path)),
                    other => other.clone(),
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// An empty path means the config's own directory, which upstream spells `"."`
/// before making it absolute.
fn absolute(path: &str, base_path: &str) -> String {
    let normalized = normalize_slashes(path);
    let normalized = if normalized.is_empty() { "." } else { &normalized };
    get_normalized_absolute_path(normalized, base_path)
}

/// The `files`/`include`/`exclude` a config resolved to, with upstream's
/// defaults applied (the middle of `parseJsonConfigFileContentWorker`).
fn file_specs(
    raw: &OrderedMap<ConfigValue>,
    options: &CompilerOptions,
    config_file_name: &str,
    errors: &mut Vec<Diagnostic>,
) -> ConfigFileSpecs {
    let files = string_list_property(raw, "files");
    let include = string_list_property(raw, "include");
    let mut exclude = string_list_property(raw, "exclude");

    if files.as_ref().is_some_and(Vec::is_empty)
        && raw.get("references").and_then(ConfigValue::as_list).is_none_or(<[_]>::is_empty)
        && raw.get("extends").is_none()
    {
        errors.push(Diagnostic::with_args(
            &messages::THE_FILES_LIST_IN_CONFIG_FILE_0_IS_EMPTY,
            tsr_core::Span::default(),
            [if config_file_name.is_empty() { "tsconfig.json" } else { config_file_name }
                .to_string()],
        ));
    }

    // With no `exclude`, the output directories exclude themselves — otherwise
    // a second build would compile what the first one emitted.
    if exclude.is_none() {
        let outputs: Vec<String> = [&options.out_dir, &options.declaration_dir]
            .into_iter()
            .filter(|directory| !directory.is_empty())
            .cloned()
            .collect();
        if !outputs.is_empty() {
            exclude = Some(outputs);
        }
    }

    // Neither `files` nor `include` means "everything under here".
    let include = match (&files, include) {
        (None, None) => vec![DEFAULT_INCLUDE_SPEC.to_string()],
        (_, include) => include.unwrap_or_default(),
    };

    ConfigFileSpecs {
        files: files.unwrap_or_default(),
        include,
        exclude: exclude.unwrap_or_default(),
    }
}

/// A root property read as a list of strings, or `None` if absent.
///
/// A non-list value is a type error upstream reports; here it reads as absent,
/// which is the same behaviour it produces — `"files": "a.ts"` appears in the
/// corpus and upstream falls back to the default include for it.
fn string_list_property(raw: &OrderedMap<ConfigValue>, name: &str) -> Option<Vec<String>> {
    Some(
        raw.get(name)?
            .as_list()?
            .iter()
            .filter_map(|entry| entry.as_str().map(str::to_string))
            .collect(),
    )
}

/// Whether "no inputs were found" is worth reporting
/// (`canJsonReportNoInputFiles`).
///
/// A config that names neither `files` nor `include` is relying on the default,
/// and an empty directory is not its fault.
fn can_report_no_input_files(raw: &OrderedMap<ConfigValue>) -> bool {
    raw.contains_key("files") || raw.contains_key("include")
}

/// The directory a config's relative paths resolve against
/// (`tsoptions.directoryOfCombinedPath`).
fn directory_of_combined_path(file_name: &str, base_path: &str) -> String {
    get_directory_path(&get_normalized_absolute_path(file_name, base_path)).to_string()
}

/// `["a","b"]`, the way upstream's `StringifyJson` renders a spec list into the
/// "no inputs were found" message.
fn quoted_list(values: &[String]) -> String {
    let inner: Vec<String> = values.iter().map(|value| format!("\"{value}\"")).collect();
    format!("[{}]", inner.join(","))
}

/// The type name an option's error message names
/// (`getCompilerOptionValueTypeString`).
fn option_type_name(declaration: &OptionDeclaration) -> String {
    use declarations::OptionKind;
    match declaration.kind {
        OptionKind::Boolean => "boolean",
        OptionKind::String | OptionKind::Enum => "string",
        OptionKind::Number => "number",
        OptionKind::List(_) => "Array",
        OptionKind::PathMap => "object",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use tsr_core::{ModuleKind, ModuleResolutionKind, ScriptTarget, Tristate};
    use tsr_vfs::InMemoryFileSystem;

    use super::*;

    fn fs(paths: &[&str]) -> InMemoryFileSystem {
        InMemoryFileSystem::new(
            paths.iter().map(|path| ((*path).to_string(), String::new())),
            [],
            true,
        )
    }

    fn parse(config: &str, paths: &[&str]) -> ParsedCommandLine {
        parse_config_file("/tsconfig.json", config, "/", &fs(paths))
    }

    #[test]
    fn an_empty_config_takes_every_file_under_its_directory() {
        let parsed = parse("{}", &["/a.ts", "/src/b.ts", "/node_modules/pkg/c.d.ts"]);
        assert_eq!(parsed.file_names, ["/a.ts", "/src/b.ts"]);
        assert_eq!(parsed.literal_file_count, 0);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    }

    #[test]
    fn compiler_options_are_read_and_typed() {
        let parsed = parse(
            r#"{ "compilerOptions": {
                "module": "commonjs", "moduleResolution": "node16",
                "target": "es2020", "strict": true, "types": ["node"]
            } }"#,
            &["/a.ts"],
        );
        let options = &parsed.compiler_options;
        assert_eq!(options.module, ModuleKind::CommonJS);
        assert_eq!(options.module_resolution, ModuleResolutionKind::Node16);
        assert_eq!(options.target, ScriptTarget::ES2020);
        assert_eq!(options.strict, Tristate::True);
        assert_eq!(options.types, Some(vec!["node".to_string()]));
        assert_eq!(options.config_file_path, "/tsconfig.json");
    }

    #[test]
    fn a_path_option_is_made_absolute_against_the_configs_directory() {
        let parsed = parse_config_file(
            "/project/tsconfig.json",
            r#"{ "compilerOptions": { "outDir": "bin", "typeRoots": ["./types"] } }"#,
            "/",
            &fs(&["/project/a.ts"]),
        );
        assert_eq!(parsed.compiler_options.out_dir, "/project/bin");
        assert_eq!(parsed.compiler_options.type_roots, Some(vec!["/project/types".to_string()]));
    }

    #[test]
    fn an_out_dir_excludes_itself_when_exclude_is_absent() {
        // Otherwise the second build compiles what the first one emitted.
        let parsed = parse(
            r#"{ "compilerOptions": { "outDir": "bin" } }"#,
            &["/a.ts", "/bin/a.js", "/bin/a.d.ts"],
        );
        assert_eq!(parsed.file_names, ["/a.ts"]);
    }

    #[test]
    fn files_are_taken_verbatim_and_lead_the_list() {
        let parsed = parse(r#"{ "files": ["b.ts"] }"#, &["/a.ts", "/b.ts"]);
        assert_eq!(parsed.file_names, ["/b.ts"]);
        assert_eq!(parsed.literal_file_count, 1);
    }

    #[test]
    fn a_files_entry_that_is_not_a_list_falls_back_to_the_default_include() {
        // `"files": "a.ts"` is in the corpus. Upstream reports a type error and
        // behaves as though `files` were absent, so `include` defaults in.
        let parsed = parse(r#"{ "files": "a.ts" }"#, &["/a.ts", "/b.ts"]);
        assert_eq!(parsed.file_names, ["/a.ts", "/b.ts"]);
    }

    #[test]
    fn an_unknown_option_is_reported_and_does_not_stop_the_rest() {
        let parsed =
            parse(r#"{ "compilerOptions": { "nonsense": 1, "strict": true } }"#, &["/a.ts"]);
        assert_eq!(parsed.compiler_options.strict, Tristate::True);
        assert_eq!(parsed.errors.len(), 1);
        assert!(parsed.errors[0].text().contains("nonsense"), "{}", parsed.errors[0].text());
    }

    #[test]
    fn an_option_of_the_wrong_type_is_reported_and_left_unset() {
        let parsed = parse(r#"{ "compilerOptions": { "strict": "yes" } }"#, &["/a.ts"]);
        assert_eq!(parsed.compiler_options.strict, Tristate::Unknown);
        assert_eq!(parsed.errors.len(), 1);
        assert!(parsed.errors[0].text().contains("boolean"), "{}", parsed.errors[0].text());
    }

    #[test]
    fn no_inputs_is_reported_only_when_the_config_asked_for_some() {
        // An empty directory with a default include is not the config's fault.
        assert!(parse("{}", &[]).errors.is_empty());
        let asked = parse(r#"{ "include": ["src/**/*"] }"#, &[]);
        assert_eq!(asked.errors.len(), 1);
        assert!(asked.errors[0].text().contains("No inputs"), "{}", asked.errors[0].text());
    }

    #[test]
    fn comments_and_trailing_commas_survive() {
        // The reason the parser reads this file rather than a JSON library.
        let parsed = parse(
            "{\n  // typeRoots defaults to node_modules/@types\n  \"compilerOptions\": { \"typeRoots\": [], },\n}",
            &["/a.ts"],
        );
        assert_eq!(parsed.compiler_options.type_roots, Some(Vec::new()));
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    }

    #[test]
    fn paths_keep_their_declaration_order_and_learn_their_base() {
        let parsed = parse_config_file(
            "/project/tsconfig.json",
            r#"{ "compilerOptions": { "paths": { "z/*": ["./z/*"], "a/*": ["./a/*"] } } }"#,
            "/",
            &fs(&["/project/a.ts"]),
        );
        assert_eq!(parsed.compiler_options.paths.keys().collect::<Vec<_>>(), ["z/*", "a/*"]);
        assert_eq!(parsed.compiler_options.paths_base_path, "/project");
    }
}
