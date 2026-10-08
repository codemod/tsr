//! `Program.verifyCompilerOptions` (5b1047d `internal/compiler/program.go:751`):
//! the option diagnostics a program reports through `GetProgramDiagnostics`.
//!
//! A free function over the finished program, as upstream's runs once after
//! the files are loaded. It owns no state: no cache, side table or traversal
//! outlives the call. The output-path helpers below are the parts of
//! `internal/outputpaths` (`GetOutputPathsFor`, `GetCommonSourceDirectory`,
//! `GetBuildInfoFileName`) the emit-path check reads, computed per call.
//!
//! Diagnostics are positioned the way upstream positions them: on the
//! `compilerOptions` entry of the program's `tsconfig.json` when one is given
//! (`createDiagnosticForOption` and its fallbacks), otherwise without a file
//! (`ast.NewCompilerDiagnostic`). A file-less diagnostic is returned with an
//! empty file name.
//!
//! Arms with no producer here, each because the program has nothing for it to
//! read:
//!
//! - `verifyProjectReferences`: this program loads no project references.
//! - The `composite` root-file check and `checkSourceFilesBelongToPath`
//!   (`CommonSourceDirectory`'s callback): both add include-processor
//!   explaining diagnostics (TS6307, TS6059), whose file-include reason chains
//!   the loader does not record.
//! - `NoEmitForJsFiles` (an internal option no config or directive sets) and
//!   project-reference source redirects in `sourceFileMayBeEmitted`; see
//!   [`Program::source_file_may_be_emitted`].
//! - `lib`, `customConditions` and `paths` substitutions are stored as plain
//!   vectors, so a written empty `lib`/`customConditions` reads as unset and a
//!   `paths` value of the wrong type never reaches the options (upstream's own
//!   comment at `program.go:1002` says the same of its parsed form).

use std::{collections::HashSet, fmt::Write as _};

use tsr_core::{
    CompilerOptions, JsxEmit, ModuleKind, ModuleResolutionKind, ScriptTarget, Span, Tristate,
};
use tsr_diagnostics::{Diagnostic, Message, messages};
use tsr_path::ComparePathsOptions;
use tsr_tsoptions::syntax::{
    ConfigSyntax, InitializerKind, PropertySyntax, for_each_property_assignment,
};

use crate::Program;

/// What [`verify_compiler_options`] reads beyond the program.
#[derive(Debug, Clone, Copy, Default)]
pub struct OptionsVerification<'c> {
    /// The program's config file (`p.opts.Config.ConfigFile`): its name and
    /// property syntax.
    pub config_file: Option<(&'c str, &'c ConfigSyntax)>,
    /// `CompilerOptions.SuppressOutputPathCheck`, which `tsr_core`'s options do
    /// not carry; the caller reads it where it was set.
    pub suppress_output_path_check: Tristate,
}

/// `Program.verifyCompilerOptions` (`program.go:751`) over `program`, in
/// upstream's order. Each entry is the diagnostic's file name (empty for a
/// compiler diagnostic) and the diagnostic.
#[must_use]
pub fn verify_compiler_options(
    program: &Program<'_>,
    input: OptionsVerification<'_>,
) -> Vec<(String, Diagnostic)> {
    let mut v = Verifier { program, options: program.compiler_options(), input, out: Vec::new() };
    v.verify();
    v.out
}

struct Verifier<'p, 'a, 'c> {
    program: &'p Program<'a>,
    options: &'p CompilerOptions,
    input: OptionsVerification<'c>,
    out: Vec<(String, Diagnostic)>,
}

impl Verifier<'_, '_, '_> {
    fn config_file_path(&self) -> &str {
        self.input.config_file.map_or("", |(name, _)| name)
    }

    /// `getCompilerOptionsPropertySyntax`.
    fn compiler_options_property(&self) -> Option<&PropertySyntax> {
        self.input.config_file.and_then(|(_, syntax)| syntax.root_property("compilerOptions"))
    }

    /// `getCompilerOptionsObjectLiteralSyntax`.
    fn compiler_options_object(&self) -> Option<&[PropertySyntax]> {
        self.compiler_options_property()?.initializer.as_ref()?.as_object()
    }

    fn push_in_config(&mut self, span: Span, message: &'static Message, args: &[String]) -> usize {
        let file = self.config_file_path().to_string();
        self.out.push((file, Diagnostic::with_args(message, span, args.iter().cloned())));
        self.out.len() - 1
    }

    /// `createOptionDiagnosticInObjectLiteralSyntax`.
    fn option_in_object(
        &mut self,
        object: Option<&[PropertySyntax]>,
        on_key: bool,
        key1: &str,
        key2: Option<&str>,
        message: &'static Message,
        args: &[String],
    ) -> Option<usize> {
        let property = for_each_property_assignment(object, key1, key2)?;
        let span = if on_key {
            property.name_span
        } else {
            // `property.Initializer`: a property assignment always has one in
            // upstream's tree; a missing one is the parser's recovery node.
            property.initializer.as_ref().map_or(property.name_span, |i| i.span)
        };
        Some(self.push_in_config(span, message, args))
    }

    /// `createCompilerOptionsDiagnostic`.
    fn compiler_options_diagnostic(&mut self, message: &'static Message, args: &[String]) -> usize {
        if let Some(span) = self.compiler_options_property().map(|p| p.name_span) {
            return self.push_in_config(span, message, args);
        }
        self.out.push((String::new(), compiler_diagnostic(message, args)));
        self.out.len() - 1
    }

    /// `createDiagnosticForOption`.
    fn for_option(
        &mut self,
        on_key: bool,
        option1: &str,
        option2: Option<&str>,
        message: &'static Message,
        args: &[String],
    ) -> usize {
        let object = self.compiler_options_object().map(<[PropertySyntax]>::to_vec);
        self.option_in_object(object.as_deref(), on_key, option1, option2, message, args)
            .unwrap_or_else(|| self.compiler_options_diagnostic(message, args))
    }

    /// `createDiagnosticForOptionName`.
    fn for_option_name(
        &mut self,
        message: &'static Message,
        option1: &str,
        option2: &str,
        extra: &[&str],
    ) {
        let mut args = vec![option1.to_string(), option2.to_string()];
        args.extend(extra.iter().map(|s| (*s).to_string()));
        let option2 = (!option2.is_empty()).then_some(option2);
        self.for_option(true, option1, option2, message, &args);
    }

    /// `createOptionValueDiagnostic`.
    fn option_value(&mut self, option1: &str, message: &'static Message, args: &[String]) {
        self.for_option(false, option1, None, message, args);
    }

    /// `createRemovedOptionDiagnostic`.
    fn removed_option(&mut self, name: &str, value: &str, use_instead: &str) {
        let (message, args) = if value.is_empty() {
            (
                &messages::OPTION_0_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION,
                vec![name.to_string()],
            )
        } else {
            (
                &messages::OPTION_0_1_HAS_BEEN_REMOVED_PLEASE_REMOVE_IT_FROM_YOUR_CONFIGURATION,
                vec![name.to_string(), value.to_string()],
            )
        };
        let index = self.for_option(value.is_empty(), name, None, message, &args);
        if !use_instead.is_empty() {
            self.out[index].1.add_message_chain(Some(compiler_diagnostic(
                &messages::USE_0_INSTEAD,
                &[use_instead.to_string()],
            )));
        }
    }

    fn compare_options(&self) -> ComparePathsOptions {
        ComparePathsOptions {
            use_case_sensitive_file_names: self.program.use_case_sensitive_file_names,
            current_directory: self.program.current_directory.clone(),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn verify(&mut self) {
        let o = self.options;

        // Removed in TS7.
        if !o.base_url.is_empty() {
            let mut use_instead = String::new();
            if !self.config_file_path().is_empty() {
                let mut relative = get_relative_path_from_file(
                    self.config_file_path(),
                    &o.base_url,
                    &self.compare_options(),
                );
                if !(relative.starts_with("./") || relative.starts_with("../")) {
                    relative = format!("./{relative}");
                }
                let suggestion = tsr_path::combine_paths(&relative, &["*"]);
                use_instead = format!("\"paths\": {{\"*\": [{}]}}", json_string(&suggestion));
            }
            self.removed_option("baseUrl", "", &use_instead);
        }
        if !o.out_file.is_empty() {
            self.removed_option("outFile", "", "");
        }
        if o.target == ScriptTarget::ES5 {
            self.removed_option("target", "ES5", "");
        }
        if o.module == ModuleKind::AMD {
            self.removed_option("module", "AMD", "");
        }
        if o.module == ModuleKind::System {
            self.removed_option("module", "System", "");
        }
        if o.module == ModuleKind::UMD {
            self.removed_option("module", "UMD", "");
        }
        if o.module_resolution == ModuleResolutionKind::Classic {
            self.removed_option("moduleResolution", "Classic", "");
        }
        if o.always_strict.is_false() {
            self.removed_option("alwaysStrict", "false", "");
        }
        if o.es_module_interop.is_false() {
            self.removed_option("esModuleInterop", "false", "");
        }
        if o.allow_synthetic_default_imports.is_false() {
            self.removed_option("allowSyntheticDefaultImports", "false", "");
        }
        if o.module_resolution == ModuleResolutionKind::Node10 {
            self.removed_option("moduleResolution", "node10", "");
        }
        if !o.downlevel_iteration.is_unknown() {
            self.removed_option("downlevelIteration", "", "");
        }

        let without_1 = &messages::OPTION_0_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_1;
        let without_1_or_2 =
            &messages::OPTION_0_CANNOT_BE_SPECIFIED_WITHOUT_SPECIFYING_OPTION_1_OR_OPTION_2;
        let with_1 = &messages::OPTION_0_CANNOT_BE_SPECIFIED_WITH_OPTION_1;
        let emit_declarations = o.declaration.is_true() || o.composite.is_true();

        if o.strict_property_initialization.is_true()
            && !o.strict_option_value(o.strict_null_checks)
        {
            self.for_option_name(
                without_1,
                "strictPropertyInitialization",
                "strictNullChecks",
                &[],
            );
        }
        if o.exact_optional_property_types.is_true() && !o.strict_option_value(o.strict_null_checks)
        {
            self.for_option_name(without_1, "exactOptionalPropertyTypes", "strictNullChecks", &[]);
        }
        if o.isolated_declarations.is_true() {
            if o.get_allow_js() {
                self.for_option_name(with_1, "allowJs", "isolatedDeclarations", &[]);
            }
            if !emit_declarations {
                self.for_option_name(
                    without_1_or_2,
                    "isolatedDeclarations",
                    "declaration",
                    &["composite"],
                );
            }
        }
        if o.inline_source_map.is_true() {
            if o.source_map.is_true() {
                self.for_option_name(with_1, "sourceMap", "inlineSourceMap", &[]);
            }
            if !o.map_root.is_empty() {
                self.for_option_name(with_1, "mapRoot", "inlineSourceMap", &[]);
            }
        }
        if o.composite.is_true() {
            if o.declaration.is_false() {
                self.for_option_name(
                    &messages::COMPOSITE_PROJECTS_MAY_NOT_DISABLE_DECLARATION_EMIT,
                    "declaration",
                    "",
                    &[],
                );
            }
            if o.incremental.is_false() {
                self.for_option_name(
                    &messages::COMPOSITE_PROJECTS_MAY_NOT_DISABLE_INCREMENTAL_COMPILATION,
                    "declaration",
                    "",
                    &[],
                );
            }
        }
        if o.ts_build_info_file.is_empty()
            && o.incremental.is_true()
            && o.config_file_path.is_empty()
        {
            self.compiler_options_diagnostic(
                &messages::OPTION_INCREMENTAL_IS_ONLY_VALID_WITH_A_KNOWN_CONFIGURATION_FILE_LIKE_TSCONFIG_JSON_OR_WHEN_TSBUILDINFOFILE_IS_EXPLICITLY_PROVIDED,
                &[],
            );
        }

        self.verify_paths();

        if !o.source_map.is_true() && !o.inline_source_map.is_true() {
            let message = &messages::OPTION_0_CAN_ONLY_BE_USED_WHEN_EITHER_OPTION_INLINESOURCEMAP_OR_OPTION_SOURCEMAP_IS_PROVIDED;
            if o.inline_sources.is_true() {
                self.for_option_name(message, "inlineSources", "", &[]);
            }
            if !o.source_root.is_empty() {
                self.for_option_name(message, "sourceRoot", "", &[]);
            }
        }
        if !(o.map_root.is_empty() || o.source_map.is_true() || o.declaration_map.is_true()) {
            self.for_option_name(without_1_or_2, "mapRoot", "sourceMap", &["declarationMap"]);
        }
        if !o.declaration_dir.is_empty() && !emit_declarations {
            self.for_option_name(without_1_or_2, "declarationDir", "declaration", &["composite"]);
        }
        if o.declaration_map.is_true() && !emit_declarations {
            self.for_option_name(without_1_or_2, "declarationMap", "declaration", &["composite"]);
        }
        if !o.lib.is_empty() && o.no_lib.is_true() {
            self.for_option_name(with_1, "lib", "noLib", &[]);
        }
        if (o.isolated_modules.is_true() || o.verbatim_module_syntax.is_true())
            && o.preserve_const_enums.is_false()
        {
            let enabled = if o.verbatim_module_syntax.is_true() {
                "verbatimModuleSyntax"
            } else {
                "isolatedModules"
            };
            self.for_option_name(
                &messages::OPTION_PRESERVECONSTENUMS_CANNOT_BE_DISABLED_WHEN_0_IS_ENABLED,
                enabled,
                "preserveConstEnums",
                &[],
            );
        }

        let needs_common_dir = !o.out_dir.is_empty()
            || !o.root_dir.is_empty()
            || !o.source_root.is_empty()
            || !o.map_root.is_empty()
            || (emit_declarations && !o.declaration_dir.is_empty());
        let common_source_directory = needs_common_dir.then(|| self.common_source_directory());
        if let Some(dir) = &common_source_directory
            && !o.out_dir.is_empty()
            && dir.is_empty()
            && self.program.files.iter().any(|f| tsr_path::get_root_length(f.file_name()) > 1)
        {
            self.for_option_name(
                &messages::CANNOT_FIND_THE_COMMON_SUBDIRECTORY_PATH_FOR_THE_INPUT_FILES,
                "outDir",
                "",
                &[],
            );
        }

        if !o.no_emit.is_true()
            && !o.composite.is_true()
            && o.root_dir.is_empty()
            && !o.config_file_path.is_empty()
            && (!o.out_dir.is_empty()
                || (emit_declarations && !o.declaration_dir.is_empty())
                || !o.out_file.is_empty())
        {
            self.verify_inferred_root_dir(common_source_directory);
        }

        if o.check_js.is_true() && !o.get_allow_js() {
            self.for_option_name(without_1, "checkJs", "allowJs", &[]);
        }
        if o.emit_declaration_only.is_true() && !emit_declarations {
            self.for_option_name(
                without_1_or_2,
                "emitDeclarationOnly",
                "declaration",
                &["composite"],
            );
        }
        if o.emit_decorator_metadata.is_true() && !o.experimental_decorators.is_true() {
            self.for_option_name(without_1, "emitDecoratorMetadata", "experimentalDecorators", &[]);
        }

        let when_jsx = &messages::OPTION_0_CANNOT_BE_SPECIFIED_WHEN_OPTION_JSX_IS_1;
        let react_jsx = matches!(o.jsx, JsxEmit::ReactJsx | JsxEmit::ReactJsxDev);
        if !o.jsx_factory.is_empty() {
            if !o.react_namespace.is_empty() {
                self.for_option_name(with_1, "reactNamespace", "jsxFactory", &[]);
            }
            if react_jsx {
                self.for_option_name(when_jsx, "jsxFactory", jsx_name(o.jsx), &[]);
            }
            if !is_isolated_entity_name(&o.jsx_factory) {
                self.option_value(
                    "jsxFactory",
                    &messages::INVALID_VALUE_FOR_JSXFACTORY_0_IS_NOT_A_VALID_IDENTIFIER_OR_QUALIFIED_NAME,
                    std::slice::from_ref(&o.jsx_factory),
                );
            }
        } else if !o.react_namespace.is_empty() && !is_identifier_text(&o.react_namespace) {
            self.option_value(
                "reactNamespace",
                &messages::INVALID_VALUE_FOR_REACTNAMESPACE_0_IS_NOT_A_VALID_IDENTIFIER,
                std::slice::from_ref(&o.react_namespace),
            );
        }
        if !o.jsx_fragment_factory.is_empty() {
            if o.jsx_factory.is_empty() {
                self.for_option_name(without_1, "jsxFragmentFactory", "jsxFactory", &[]);
            }
            if react_jsx {
                self.for_option_name(when_jsx, "jsxFragmentFactory", jsx_name(o.jsx), &[]);
            }
            if !is_isolated_entity_name(&o.jsx_fragment_factory) {
                self.option_value(
                    "jsxFragmentFactory",
                    &messages::INVALID_VALUE_FOR_JSXFRAGMENTFACTORY_0_IS_NOT_A_VALID_IDENTIFIER_OR_QUALIFIED_NAME,
                    std::slice::from_ref(&o.jsx_fragment_factory),
                );
            }
        }
        if !o.react_namespace.is_empty() && react_jsx {
            self.for_option_name(when_jsx, "reactNamespace", jsx_name(o.jsx), &[]);
        }
        if !o.jsx_import_source.is_empty() && o.jsx == JsxEmit::React {
            self.for_option_name(when_jsx, "jsxImportSource", jsx_name(o.jsx), &[]);
        }

        let module_kind = o.emit_module_kind();
        if o.allow_importing_ts_extensions.is_true()
            && !(o.no_emit.is_true()
                || o.emit_declaration_only.is_true()
                || o.rewrite_relative_import_extensions.is_true())
        {
            self.option_value(
                "allowImportingTsExtensions",
                &messages::OPTION_ALLOWIMPORTINGTSEXTENSIONS_CAN_ONLY_BE_USED_WHEN_ONE_OF_NOEMIT_EMITDECLARATIONONLY_OR_REWRITERELATIVEIMPORTEXTENSIONS_IS_SET,
                &[],
            );
        }
        let module_resolution = o.module_resolution_kind();
        let supports_exports = matches!(
            module_resolution,
            ModuleResolutionKind::Node16
                | ModuleResolutionKind::NodeNext
                | ModuleResolutionKind::Bundler
        );
        let exports_message = &messages::OPTION_0_CAN_ONLY_BE_USED_WHEN_MODULERESOLUTION_IS_SET_TO_NODE16_NODENEXT_OR_BUNDLER;
        if o.resolve_package_json_exports.is_true() && !supports_exports {
            self.for_option_name(exports_message, "resolvePackageJsonExports", "", &[]);
        }
        if o.resolve_package_json_imports.is_true() && !supports_exports {
            self.for_option_name(exports_message, "resolvePackageJsonImports", "", &[]);
        }
        if !o.custom_conditions.is_empty() && !supports_exports {
            self.for_option_name(exports_message, "customConditions", "", &[]);
        }
        let non_node_esm = matches!(
            module_kind,
            ModuleKind::ES2015 | ModuleKind::ES2020 | ModuleKind::ES2022 | ModuleKind::ESNext
        );
        if module_resolution == ModuleResolutionKind::Bundler
            && !non_node_esm
            && module_kind != ModuleKind::Preserve
            && module_kind != ModuleKind::CommonJS
        {
            self.option_value(
                "moduleResolution",
                &messages::OPTION_0_CAN_ONLY_BE_USED_WHEN_MODULE_IS_SET_TO_PRESERVE_COMMONJS_OR_ES2015_OR_LATER,
                &["bundler".to_string()],
            );
        }
        let node_module = (ModuleKind::Node16..=ModuleKind::NodeNext).contains(&module_kind);
        let node_resolution = matches!(
            module_resolution,
            ModuleResolutionKind::Node16 | ModuleResolutionKind::NodeNext
        );
        if node_module && !node_resolution {
            let resolution_name = match module_kind {
                ModuleKind::NodeNext => "NodeNext",
                _ => "Node16",
            };
            self.option_value(
                "moduleResolution",
                &messages::OPTION_MODULERESOLUTION_MUST_BE_SET_TO_0_OR_LEFT_UNSPECIFIED_WHEN_OPTION_MODULE_IS_SET_TO_1,
                &[resolution_name.to_string(), module_kind_name(module_kind).to_string()],
            );
        } else if node_resolution && !node_module {
            let name = module_resolution_name(module_resolution).to_string();
            self.option_value(
                "module",
                &messages::OPTION_MODULE_MUST_BE_SET_TO_0_WHEN_OPTION_MODULERESOLUTION_IS_SET_TO_1,
                &[name.clone(), name],
            );
        }

        if !o.no_emit.is_true() && !self.input.suppress_output_path_check.is_true() {
            self.verify_emit_file_paths();
        }
    }

    /// The `options.Paths` loop (`program.go:1001`) with
    /// `createDiagnosticForOptionPaths` and `createDiagnosticForOptionPathKeyValue`.
    fn verify_paths(&mut self) {
        let entries: Vec<(String, Vec<String>)> = self
            .options
            .paths
            .entries()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect();
        for (key, value) in entries {
            if !has_zero_or_one_asterisk(&key) {
                self.for_option_paths(
                    true,
                    &key,
                    &messages::PATTERN_0_CAN_HAVE_AT_MOST_ONE_ASTERISK_CHARACTER,
                    std::slice::from_ref(&key),
                );
            }
            if value.is_empty() {
                self.for_option_paths(
                    false,
                    &key,
                    &messages::SUBSTITUTIONS_FOR_PATTERN_0_SHOULDN_T_BE_AN_EMPTY_ARRAY,
                    std::slice::from_ref(&key),
                );
            }
            for (i, subst) in value.iter().enumerate() {
                if !has_zero_or_one_asterisk(subst) {
                    self.for_option_path_key_value(
                        &key,
                        i,
                        &messages::SUBSTITUTION_0_IN_PATTERN_1_CAN_HAVE_AT_MOST_ONE_ASTERISK_CHARACTER,
                        &[subst.clone(), key.clone()],
                    );
                }
                if !tsr_path::path_is_relative(subst) && tsr_path::get_root_length(subst) == 0 {
                    self.for_option_path_key_value(&key, i, &messages::NON_RELATIVE_PATHS_ARE_NOT_ALLOWED_DID_YOU_FORGET_A_LEADING_SLASH, &[]);
                }
            }
        }
    }

    /// `forEachOptionPathsSyntax`: the `paths` object's properties when
    /// `compilerOptions.paths` is the first `paths` key and an object literal.
    fn paths_object(&self) -> Option<Vec<PropertySyntax>> {
        let paths = for_each_property_assignment(self.compiler_options_object(), "paths", None)?;
        paths.initializer.as_ref()?.as_object().map(<[PropertySyntax]>::to_vec)
    }

    fn for_option_paths(
        &mut self,
        on_key: bool,
        key: &str,
        message: &'static Message,
        args: &[String],
    ) {
        let object = self.paths_object();
        let found = object.and_then(|object| {
            self.option_in_object(Some(&object), on_key, key, None, message, args)
        });
        if found.is_none() {
            self.compiler_options_diagnostic(message, args);
        }
    }

    fn for_option_path_key_value(
        &mut self,
        key: &str,
        index: usize,
        message: &'static Message,
        args: &[String],
    ) {
        let span = self.paths_object().and_then(|object| {
            let property = for_each_property_assignment(Some(&object), key, None)?;
            match &property.initializer.as_ref()?.kind {
                InitializerKind::Array(elements) => elements.get(index).copied(),
                _ => None,
            }
        });
        match span {
            Some(span) => {
                self.push_in_config(span, message, args);
            }
            None => {
                self.compiler_options_diagnostic(message, args);
            }
        }
    }

    /// `Program.CommonSourceDirectory` (`program.go:1585`) through
    /// `outputpaths.GetCommonSourceDirectory`, without the
    /// `checkSourceFilesBelongToPath` callback (see the module docs).
    fn common_source_directory(&self) -> String {
        let o = self.options;
        let dir = if !o.root_dir.is_empty() {
            o.root_dir.clone()
        } else if !o.config_file_path.is_empty() {
            tsr_path::get_directory_path(&o.config_file_path).to_string()
        } else {
            let files: Vec<&str> = (0..self.program.files.len())
                .filter(|&i| {
                    self.program.source_file_may_be_emitted(i)
                        && !tsr_path::is_declaration_file_name(self.program.files[i].file_name())
                })
                .map(|i| self.program.files[i].file_name())
                .collect();
            common_source_directory_of_file_names(
                &files,
                &self.program.current_directory,
                self.program.use_case_sensitive_file_names,
            )
        };
        if dir.is_empty() { dir } else { tsr_path::ensure_trailing_directory_separator(&dir) }
    }

    /// The inferred-`rootDir` layout check (`program.go:1068`).
    fn verify_inferred_root_dir(&mut self, common_source_directory: Option<String>) {
        let o = self.options;
        let dir = common_source_directory.unwrap_or_else(|| self.common_source_directory());
        let emitted: Vec<&str> = (0..self.program.files.len())
            .filter(|&i| {
                let name = self.program.files[i].file_name();
                !tsr_path::is_declaration_file_name(name)
                    && self.program.source_file_may_be_emitted(i)
            })
            .map(|i| self.program.files[i].file_name())
            .collect();
        let sensitive = self.program.use_case_sensitive_file_names;
        let mut dir59 = common_source_directory_of_file_names(
            &emitted,
            &self.program.current_directory,
            sensitive,
        );
        if !dir59.is_empty() {
            dir59 = tsr_path::ensure_trailing_directory_separator(&dir59);
        }
        if dir59.is_empty()
            || tsr_path::get_canonical_file_name(&dir, sensitive)
                == tsr_path::get_canonical_file_name(&dir59, sensitive)
        {
            return;
        }
        let option1 = if !o.out_file.is_empty() {
            "outFile"
        } else if !o.out_dir.is_empty() {
            "outDir"
        } else {
            "declarationDir"
        };
        let option2 = (o.out_file.is_empty() && !o.out_dir.is_empty()).then_some("declarationDir");
        let args = [
            tsr_path::get_base_file_name(&o.config_file_path).to_string(),
            get_relative_path_from_file(&o.config_file_path, &dir59, &self.compare_options()),
        ];
        let index = self.for_option(
            true,
            option1,
            option2,
            &messages::THE_COMMON_SOURCE_DIRECTORY_OF_0_IS_1_THE_ROOTDIR_SETTING_MUST_BE_EXPLICITLY_SET_TO_THIS_OR_ANOTHER_PATH_TO_ADJUST_YOUR_OUTPUT_S_FILE_LAYOUT,
            &args,
        );
        self.out[index].1.add_message_chain(Some(compiler_diagnostic(
            &messages::VISIT_HTTPS_COLON_SLASH_SLASHAKA_MS_SLASHTS6_FOR_MIGRATION_INFORMATION,
            &[],
        )));
    }

    /// The emit-path uniqueness check (`program.go:1199`): `verifyEmitFilePath`
    /// over `ForEachEmittedFile(getSourceFilesToEmit)` and the build-info file.
    fn verify_emit_file_paths(&mut self) {
        let program = self.program;
        let o = self.options;
        let common = self.common_source_directory();
        let mut seen: HashSet<String> = HashSet::new();
        let mut emit_paths: Vec<String> = Vec::new();
        for i in 0..program.files.len() {
            if !program.source_file_may_be_emitted(i) {
                continue;
            }
            let paths =
                output_paths_for(program.files[i].file_name(), o, &common, &self.compare_options());
            emit_paths.extend(paths.into_iter().filter(|p| !p.is_empty()));
        }
        emit_paths.push(build_info_file_name(o, &self.compare_options()));
        let without_config = self.config_file_path().is_empty();
        for emit_file_name in emit_paths.into_iter().filter(|p| !p.is_empty()) {
            let emit_path = program.to_path(&emit_file_name);
            if program.files_by_path.contains_key(&emit_path) {
                let mut diag = compiler_diagnostic(
                    &messages::CANNOT_WRITE_FILE_0_BECAUSE_IT_WOULD_OVERWRITE_INPUT_FILE,
                    std::slice::from_ref(&emit_file_name),
                );
                if without_config {
                    diag.add_message_chain(Some(compiler_diagnostic(
                        &messages::ADDING_A_TSCONFIG_JSON_FILE_WILL_HELP_ORGANIZE_PROJECTS_THAT_CONTAIN_BOTH_TYPESCRIPT_AND_JAVASCRIPT_FILES_LEARN_MORE_AT_HTTPS_COLON_SLASH_SLASHAKA_MS_SLASHTSCONFIG,
                        &[],
                    )));
                }
                self.out.push((String::new(), diag));
            }
            let key = if program.use_case_sensitive_file_names {
                emit_path.as_str().to_string()
            } else {
                emit_path.as_str().to_ascii_lowercase()
            };
            if !seen.insert(key) {
                self.out.push((
                    String::new(),
                    compiler_diagnostic(
                        &messages::CANNOT_WRITE_FILE_0_BECAUSE_IT_WOULD_BE_OVERWRITTEN_BY_MULTIPLE_INPUT_FILES,
                        std::slice::from_ref(&emit_file_name),
                    ),
                ));
            }
        }
    }
}

/// `ast.NewCompilerDiagnostic`.
fn compiler_diagnostic(message: &'static Message, args: &[String]) -> Diagnostic {
    Diagnostic::with_args(message, Span::default(), args.iter().cloned())
}

/// `outputpaths.GetOutputPathsFor` without forced emit: the JS, source map,
/// declaration and declaration map paths, empty where not emitted.
fn output_paths_for(
    file_name: &str,
    o: &CompilerOptions,
    common: &str,
    compare: &ComparePathsOptions,
) -> [String; 4] {
    let own = own_emit_output_file_path(
        file_name,
        o,
        common,
        compare,
        output_extension(file_name, o.jsx),
    );
    let is_json = tsr_path::file_extension_is(file_name, ".json");
    let json_to_same_location =
        is_json && tsr_path::compare_paths(file_name, &own, compare) == std::cmp::Ordering::Equal;
    let mut paths: [String; 4] = Default::default();
    if !o.emit_declaration_only.is_true() && !json_to_same_location {
        if !is_json && o.source_map.is_true() && !o.inline_source_map.is_true() {
            paths[1] = format!("{own}.map");
        }
        paths[0] = own;
    }
    if (o.declaration.is_true() || o.composite.is_true()) && !is_json {
        paths[2] = declaration_emit_output_file_path(file_name, o, common, compare);
        if o.declaration_map.is_true() {
            paths[3] = format!("{}.map", paths[2]);
        }
    }
    paths
}

/// `outputpaths.getOwnEmitOutputFilePath`.
fn own_emit_output_file_path(
    file_name: &str,
    o: &CompilerOptions,
    common: &str,
    compare: &ComparePathsOptions,
    extension: &str,
) -> String {
    let without_extension = if o.out_dir.is_empty() {
        tsr_path::remove_file_extension(file_name).to_string()
    } else {
        tsr_path::remove_file_extension(&source_file_path_in_new_dir(
            file_name, &o.out_dir, common, compare,
        ))
        .to_string()
    };
    without_extension + extension
}

/// `outputpaths.GetOutputExtension`.
fn output_extension(file_name: &str, jsx: JsxEmit) -> &'static str {
    use tsr_path::file_extension_is_one_of as is_one_of;
    if tsr_path::file_extension_is(file_name, ".json") {
        ".json"
    } else if jsx == JsxEmit::Preserve && is_one_of(file_name, &[".jsx", ".tsx"]) {
        ".jsx"
    } else if is_one_of(file_name, &[".mts", ".mjs"]) {
        ".mjs"
    } else if is_one_of(file_name, &[".cts", ".cjs"]) {
        ".cjs"
    } else {
        ".js"
    }
}

/// `outputpaths.GetDeclarationEmitOutputFilePath`.
fn declaration_emit_output_file_path(
    file_name: &str,
    o: &CompilerOptions,
    common: &str,
    compare: &ComparePathsOptions,
) -> String {
    let output_dir = if o.declaration_dir.is_empty() { &o.out_dir } else { &o.declaration_dir };
    let path = if output_dir.is_empty() {
        file_name.to_string()
    } else {
        source_file_path_in_new_dir_worker(file_name, output_dir, common, compare)
    };
    let extension = declaration_emit_extension_for_path(&path);
    format!("{}{extension}", tsr_path::remove_file_extension(&path))
}

/// `tspath.GetDeclarationEmitExtensionForPath`.
fn declaration_emit_extension_for_path(path: &str) -> String {
    use tsr_path::file_extension_is_one_of as is_one_of;
    if is_one_of(path, &[".mjs", ".mts"]) {
        ".d.mts".to_string()
    } else if is_one_of(path, &[".cjs", ".cts"]) {
        ".d.cts".to_string()
    } else if is_one_of(path, &[".ts", ".tsx", ".js", ".jsx"]) {
        ".d.ts".to_string()
    } else {
        let extension = tsr_path::get_any_extension_from_path(path);
        if extension.is_empty() { ".d.ts".to_string() } else { format!(".d{extension}.ts") }
    }
}

/// `outputpaths.GetSourceFilePathInNewDir`.
fn source_file_path_in_new_dir(
    file_name: &str,
    new_dir: &str,
    common: &str,
    compare: &ComparePathsOptions,
) -> String {
    let mut source = tsr_path::get_normalized_absolute_path(file_name, &compare.current_directory);
    let common = tsr_path::ensure_trailing_directory_separator(common);
    if tsr_path::contains_path(&common, &source, compare) {
        source = source[common.len().min(source.len())..].to_string();
    }
    tsr_path::combine_paths(new_dir, &[&source])
}

/// `outputpaths.GetSourceFilePathInNewDirWorker`.
fn source_file_path_in_new_dir_worker(
    file_name: &str,
    new_dir: &str,
    common: &str,
    compare: &ComparePathsOptions,
) -> String {
    let mut source = tsr_path::get_normalized_absolute_path(file_name, &compare.current_directory);
    let sensitive = compare.use_case_sensitive_file_names;
    if tsr_path::get_canonical_file_name(&source, sensitive)
        .starts_with(&tsr_path::get_canonical_file_name(common, sensitive))
    {
        source = source[common.len()..].to_string();
    }
    tsr_path::combine_paths(new_dir, &[&source])
}

/// `outputpaths.GetBuildInfoFileName`.
fn build_info_file_name(o: &CompilerOptions, compare: &ComparePathsOptions) -> String {
    if !(o.incremental.is_true() || o.composite.is_true()) {
        return String::new();
    }
    if !o.ts_build_info_file.is_empty() {
        return o.ts_build_info_file.clone();
    }
    if o.config_file_path.is_empty() {
        return String::new();
    }
    let config_without_extension = tsr_path::remove_file_extension(&o.config_file_path);
    let stem = if o.out_dir.is_empty() {
        config_without_extension.to_string()
    } else if o.root_dir.is_empty() {
        tsr_path::combine_paths(
            &o.out_dir,
            &[tsr_path::get_base_file_name(config_without_extension)],
        )
    } else {
        resolve_path(
            &o.out_dir,
            &tsr_path::get_relative_path_from_directory(
                &o.root_dir,
                config_without_extension,
                compare,
            ),
        )
    };
    stem + ".tsbuildinfo"
}

/// `outputpaths.computeCommonSourceDirectoryOfFilenames`.
fn common_source_directory_of_file_names(
    file_names: &[&str],
    current_directory: &str,
    sensitive: bool,
) -> String {
    let mut common: Option<Vec<String>> = None;
    for file_name in file_names {
        let mut components = tsr_path::reduce_path_components(&tsr_path::get_path_components(
            file_name,
            current_directory,
        ));
        components.pop();
        let Some(common) = common.as_mut() else {
            common = Some(components);
            continue;
        };
        let n = common.len().min(components.len());
        for i in 0..n {
            if tsr_path::get_canonical_file_name(&common[i], sensitive)
                != tsr_path::get_canonical_file_name(&components[i], sensitive)
            {
                if i == 0 {
                    return String::new();
                }
                common.truncate(i);
                break;
            }
        }
        if components.len() < common.len() {
            common.truncate(components.len());
        }
    }
    match common {
        Some(common) if !common.is_empty() => tsr_path::get_path_from_path_components(&common),
        _ => current_directory.to_string(),
    }
}

/// `tspath.ResolvePath(path, relative)`.
fn resolve_path(path: &str, relative: &str) -> String {
    tsr_path::normalize_path(&tsr_path::combine_paths(path, &[relative]))
}

/// `tspath.GetRelativePathFromFile`.
fn get_relative_path_from_file(from: &str, to: &str, compare: &ComparePathsOptions) -> String {
    let relative =
        tsr_path::get_relative_path_from_directory(tsr_path::get_directory_path(from), to, compare);
    // `EnsurePathIsNonModuleName`.
    if !tsr_path::path_is_relative(&relative) && tsr_path::get_root_length(&relative) == 0 {
        format!("./{relative}")
    } else {
        relative
    }
}

/// `hasZeroOrOneAsteriskCharacter`.
fn has_zero_or_one_asterisk(text: &str) -> bool {
    text.matches('*').count() <= 1
}

/// `scanner.IsIdentifierText` under `LanguageVariantStandard`.
fn is_identifier_text(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(tsr_scanner::is_identifier_start)
        && chars.all(tsr_scanner::is_identifier_part)
}

/// `parser.ParseIsolatedEntityName(text) != nil`: an identifier name, then any
/// number of `.`-separated identifier names, then the end of the text, with no
/// scanner diagnostic.
fn is_isolated_entity_name(text: &str) -> bool {
    use tsr_ast::SyntaxKind;
    let mut scanner = tsr_scanner::Scanner::new(text);
    let mut expect_name = true;
    loop {
        let kind = scanner.scan().kind;
        if expect_name {
            // `tokenIsIdentifierOrKeyword`.
            if !(kind == SyntaxKind::Identifier || kind.is_keyword()) {
                return false;
            }
            expect_name = false;
        } else if kind == SyntaxKind::DotToken {
            expect_name = true;
        } else {
            return kind == SyntaxKind::EndOfFile && scanner.diagnostics().is_empty();
        }
    }
}

/// `JsxEmit.String()` (`core/compileroptions.go:540`).
fn jsx_name(jsx: JsxEmit) -> &'static str {
    match jsx {
        JsxEmit::None => "",
        JsxEmit::Preserve => "preserve",
        JsxEmit::React => "react",
        JsxEmit::ReactNative => "react-native",
        JsxEmit::ReactJsx => "react-jsx",
        JsxEmit::ReactJsxDev => "react-jsxdev",
    }
}

/// The generated `ModuleKind.String()`.
fn module_kind_name(kind: ModuleKind) -> &'static str {
    match kind {
        ModuleKind::None => "None",
        ModuleKind::CommonJS => "CommonJS",
        ModuleKind::AMD => "AMD",
        ModuleKind::UMD => "UMD",
        ModuleKind::System => "System",
        ModuleKind::ES2015 => "ES2015",
        ModuleKind::ES2020 => "ES2020",
        ModuleKind::ES2022 => "ES2022",
        ModuleKind::ESNext => "ESNext",
        ModuleKind::Node16 => "Node16",
        ModuleKind::Node18 => "Node18",
        ModuleKind::Node20 => "Node20",
        ModuleKind::NodeNext => "NodeNext",
        ModuleKind::Preserve => "Preserve",
    }
}

/// `ModuleResolutionKind.String()` (`core/compileroptions.go:459`).
fn module_resolution_name(kind: ModuleResolutionKind) -> &'static str {
    match kind {
        ModuleResolutionKind::Unknown => "",
        ModuleResolutionKind::Classic => "Classic",
        ModuleResolutionKind::Node10 => "Node10",
        ModuleResolutionKind::Node16 => "Node16",
        ModuleResolutionKind::NodeNext => "NodeNext",
        ModuleResolutionKind::Bundler => "Bundler",
    }
}

/// `json.Marshal` of a string: quoted, with Go's HTML-safe escapes.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if matches!(c, '<' | '>' | '&' | '\u{2028}' | '\u{2029}') || (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
