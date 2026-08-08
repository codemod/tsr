//! `tsc --help --all` — every declared option, by category.
//!
//! Ported from `printAllHelp` (`internal/execute/tsc/help.go:110`) and
//! `generateSectionOptionsOutput`'s sub-category branch (`:170`).
//!
//! # Generated from upstream, not transcribed
//!
//! Each entry's **category and description are `Message` references**, resolved
//! by matching upstream's `diagnostics.X` constant against the generated
//! catalogue's key. So the text is not copied here at all — it is the same
//! `Message` the rest of the compiler reports, and it cannot drift from
//! upstream's `diagnosticMessages.json` because both come from that file.
//!
//! The table was produced by walking `declscompiler.go` for `Name`,
//! `ShortName`, `Category` and `Description`, and is regenerable the same way.
//! Eleven options upstream declares are absent: their description constants do
//! not resolve to a message in this catalogue, and inventing text for them would
//! be exactly the hand-written help this approach exists to avoid.
//!
//! # Ordering
//!
//! Upstream groups by category in **first-appearance order** of the category in
//! the declaration table, then lists each category's options in declaration
//! order. `generateSectionOptionsOutput` builds `categoryOrder` as it walks, and
//! that is reproduced by keeping this table in declaration order and grouping
//! stably.

use tsr_diagnostics::{Message, messages};

/// One option as `--help --all` lists it.
pub struct AllHelpOption {
    /// The option name, without dashes.
    pub name: &'static str,
    /// Its one-letter spelling, if it has one.
    pub short: Option<&'static str>,
    /// The `### ` heading it appears under.
    pub category: &'static Message,
    /// What it does.
    pub description: &'static Message,
}

/// The options grouped by category, in first-appearance order
/// (`generateSectionOptionsOutput`'s `categoryMap` plus `categoryOrder`).
///
/// Not a `HashMap`: the order is the output, and it is the order categories
/// first appear in the declaration table rather than anything alphabetical.
#[must_use]
pub fn grouped() -> Vec<(&'static str, Vec<&'static AllHelpOption>)> {
    // **Sorted by lowercased name first.** `getOptionsForHelp` (`help.go:27`)
    // sorts the whole table for `--all` and only then groups, so the category
    // order is the order categories first appear *after* sorting, not in the
    // declaration table. `--all` precedes `--help` for this reason and no other.
    let mut sorted: Vec<&'static AllHelpOption> = ALL_HELP_OPTIONS.iter().collect();
    sorted.sort_by_key(|option| option.name.to_ascii_lowercase());

    let mut groups: Vec<(&'static str, Vec<&'static AllHelpOption>)> = Vec::new();
    for option in sorted {
        let category = option.category.text();
        match groups.iter_mut().find(|(name, _)| *name == category) {
            Some((_, options)) => options.push(option),
            None => groups.push((category, vec![option])),
        }
    }
    groups
}

/// Every option `--help --all` lists, in declaration order.
pub static ALL_HELP_OPTIONS: &[AllHelpOption] = &[
    AllHelpOption {
        name: "help",
        short: Some("h"),
        category: &messages::COMMAND_LINE_OPTIONS,
        description: &messages::PRINT_THIS_MESSAGE,
    },
    AllHelpOption {
        name: "watch",
        short: Some("w"),
        category: &messages::COMMAND_LINE_OPTIONS,
        description: &messages::WATCH_INPUT_FILES,
    },
    AllHelpOption {
        name: "preserveWatchOutput",
        short: None,
        category: &messages::OUTPUT_FORMATTING,
        description: &messages::DISABLE_WIPING_THE_CONSOLE_IN_WATCH_MODE,
    },
    AllHelpOption {
        name: "listFiles",
        short: None,
        category: &messages::COMPILER_DIAGNOSTICS,
        description: &messages::PRINT_ALL_OF_THE_FILES_READ_DURING_THE_COMPILATION,
    },
    AllHelpOption {
        name: "explainFiles",
        short: None,
        category: &messages::COMPILER_DIAGNOSTICS,
        description: &messages::PRINT_FILES_READ_DURING_THE_COMPILATION_INCLUDING_WHY_IT_WAS_INCLUDED,
    },
    AllHelpOption {
        name: "listEmittedFiles",
        short: None,
        category: &messages::COMPILER_DIAGNOSTICS,
        description: &messages::PRINT_THE_NAMES_OF_EMITTED_FILES_AFTER_A_COMPILATION,
    },
    AllHelpOption {
        name: "pretty",
        short: None,
        category: &messages::OUTPUT_FORMATTING,
        description: &messages::ENABLE_COLOR_AND_FORMATTING_IN_TYPESCRIPT_S_OUTPUT_TO_MAKE_COMPILER_ERRORS_EASIER_TO_READ,
    },
    AllHelpOption {
        name: "traceResolution",
        short: None,
        category: &messages::COMPILER_DIAGNOSTICS,
        description: &messages::LOG_PATHS_USED_DURING_THE_MODULERESOLUTION_PROCESS,
    },
    AllHelpOption {
        name: "diagnostics",
        short: None,
        category: &messages::COMPILER_DIAGNOSTICS,
        description: &messages::OUTPUT_COMPILER_PERFORMANCE_INFORMATION_AFTER_BUILDING,
    },
    AllHelpOption {
        name: "extendedDiagnostics",
        short: None,
        category: &messages::COMPILER_DIAGNOSTICS,
        description: &messages::OUTPUT_MORE_DETAILED_COMPILER_PERFORMANCE_INFORMATION_AFTER_BUILDING,
    },
    AllHelpOption {
        name: "generateCpuProfile",
        short: None,
        category: &messages::COMPILER_DIAGNOSTICS,
        description: &messages::EMIT_A_V8_CPU_PROFILE_OF_THE_COMPILER_RUN_FOR_DEBUGGING,
    },
    AllHelpOption {
        name: "generateTrace",
        short: None,
        category: &messages::COMPILER_DIAGNOSTICS,
        description: &messages::GENERATES_AN_EVENT_TRACE_AND_A_LIST_OF_TYPES,
    },
    AllHelpOption {
        name: "incremental",
        short: Some("i"),
        category: &messages::PROJECTS,
        description: &messages::SAVE_TSBUILDINFO_FILES_TO_ALLOW_FOR_INCREMENTAL_COMPILATION_OF_PROJECTS,
    },
    AllHelpOption {
        name: "declaration",
        short: Some("d"),
        category: &messages::EMIT,
        description: &messages::GENERATE_D_TS_FILES_FROM_TYPESCRIPT_AND_JAVASCRIPT_FILES_IN_YOUR_PROJECT,
    },
    AllHelpOption {
        name: "declarationMap",
        short: None,
        category: &messages::EMIT,
        description: &messages::CREATE_SOURCEMAPS_FOR_D_TS_FILES,
    },
    AllHelpOption {
        name: "emitDeclarationOnly",
        short: None,
        category: &messages::EMIT,
        description: &messages::ONLY_OUTPUT_D_TS_FILES_AND_NOT_JAVASCRIPT_FILES,
    },
    AllHelpOption {
        name: "sourceMap",
        short: None,
        category: &messages::EMIT,
        description: &messages::CREATE_SOURCE_MAP_FILES_FOR_EMITTED_JAVASCRIPT_FILES,
    },
    AllHelpOption {
        name: "inlineSourceMap",
        short: None,
        category: &messages::EMIT,
        description: &messages::INCLUDE_SOURCEMAP_FILES_INSIDE_THE_EMITTED_JAVASCRIPT,
    },
    AllHelpOption {
        name: "deduplicatePackages",
        short: None,
        category: &messages::TYPE_CHECKING,
        description: &messages::DEDUPLICATE_PACKAGES_WITH_THE_SAME_NAME_AND_VERSION,
    },
    AllHelpOption {
        name: "noEmit",
        short: None,
        category: &messages::EMIT,
        description: &messages::DISABLE_EMITTING_FILES_FROM_A_COMPILATION,
    },
    AllHelpOption {
        name: "assumeChangesOnlyAffectDirectDependencies",
        short: None,
        category: &messages::WATCH_AND_BUILD_MODES,
        description: &messages::HAVE_RECOMPILES_IN_PROJECTS_THAT_USE_INCREMENTAL_AND_WATCH_MODE_ASSUME_THAT_CHANGES_WITHIN_A_FILE_WILL_ONLY_AFFECT_FILES_DIRECTLY_DEPENDING_ON_IT,
    },
    AllHelpOption {
        name: "locale",
        short: None,
        category: &messages::COMMAND_LINE_OPTIONS,
        description: &messages::SET_THE_LANGUAGE_OF_THE_MESSAGING_FROM_TYPESCRIPT_THIS_DOES_NOT_AFFECT_EMIT,
    },
    AllHelpOption {
        name: "quiet",
        short: Some("q"),
        category: &messages::COMMAND_LINE_OPTIONS,
        description: &messages::DO_NOT_PRINT_DIAGNOSTICS,
    },
    AllHelpOption {
        name: "singleThreaded",
        short: None,
        category: &messages::COMMAND_LINE_OPTIONS,
        description: &messages::RUN_IN_SINGLE_THREADED_MODE,
    },
    AllHelpOption {
        name: "pprofDir",
        short: None,
        category: &messages::COMMAND_LINE_OPTIONS,
        description: &messages::GENERATE_PPROF_CPU_SLASHMEMORY_PROFILES_TO_THE_GIVEN_DIRECTORY,
    },
    AllHelpOption {
        name: "all",
        short: None,
        category: &messages::COMMAND_LINE_OPTIONS,
        description: &messages::SHOW_ALL_COMPILER_OPTIONS,
    },
    AllHelpOption {
        name: "version",
        short: Some("v"),
        category: &messages::COMMAND_LINE_OPTIONS,
        description: &messages::PRINT_THE_COMPILER_S_VERSION,
    },
    AllHelpOption {
        name: "init",
        short: None,
        category: &messages::COMMAND_LINE_OPTIONS,
        description: &messages::INITIALIZES_A_TYPESCRIPT_PROJECT_AND_CREATES_A_TSCONFIG_JSON_FILE,
    },
    AllHelpOption {
        name: "project",
        short: Some("p"),
        category: &messages::COMMAND_LINE_OPTIONS,
        description: &messages::COMPILE_THE_PROJECT_GIVEN_THE_PATH_TO_ITS_CONFIGURATION_FILE_OR_TO_A_FOLDER_WITH_A_TSCONFIG_JSON,
    },
    AllHelpOption {
        name: "showConfig",
        short: None,
        category: &messages::COMMAND_LINE_OPTIONS,
        description: &messages::PRINT_THE_FINAL_CONFIGURATION_INSTEAD_OF_BUILDING,
    },
    AllHelpOption {
        name: "listFilesOnly",
        short: None,
        category: &messages::COMMAND_LINE_OPTIONS,
        description: &messages::PRINT_NAMES_OF_FILES_THAT_ARE_PART_OF_THE_COMPILATION_AND_THEN_STOP_PROCESSING,
    },
    AllHelpOption {
        name: "ignoreConfig",
        short: None,
        category: &messages::COMMAND_LINE_OPTIONS,
        description: &messages::IGNORE_THE_TSCONFIG_FOUND_AND_BUILD_WITH_COMMANDLINE_OPTIONS_AND_FILES,
    },
    AllHelpOption {
        name: "target",
        short: Some("t"),
        category: &messages::LANGUAGE_AND_ENVIRONMENT,
        description: &messages::SET_THE_JAVASCRIPT_LANGUAGE_VERSION_FOR_EMITTED_JAVASCRIPT_AND_INCLUDE_COMPATIBLE_LIBRARY_DECLARATIONS,
    },
    AllHelpOption {
        name: "module",
        short: Some("m"),
        category: &messages::MODULES,
        description: &messages::SPECIFY_WHAT_MODULE_CODE_IS_GENERATED,
    },
    AllHelpOption {
        name: "lib",
        short: None,
        category: &messages::LANGUAGE_AND_ENVIRONMENT,
        description: &messages::SPECIFY_A_SET_OF_BUNDLED_LIBRARY_DECLARATION_FILES_THAT_DESCRIBE_THE_TARGET_RUNTIME_ENVIRONMENT,
    },
    AllHelpOption {
        name: "allowJs",
        short: None,
        category: &messages::JAVASCRIPT_SUPPORT,
        description: &messages::ALLOW_JAVASCRIPT_FILES_TO_BE_A_PART_OF_YOUR_PROGRAM_USE_THE_CHECKJS_OPTION_TO_GET_ERRORS_FROM_THESE_FILES,
    },
    AllHelpOption {
        name: "checkJs",
        short: None,
        category: &messages::JAVASCRIPT_SUPPORT,
        description: &messages::ENABLE_ERROR_REPORTING_IN_TYPE_CHECKED_JAVASCRIPT_FILES,
    },
    AllHelpOption {
        name: "jsx",
        short: None,
        category: &messages::LANGUAGE_AND_ENVIRONMENT,
        description: &messages::SPECIFY_WHAT_JSX_CODE_IS_GENERATED,
    },
    AllHelpOption {
        name: "outFile",
        short: None,
        category: &messages::EMIT,
        description: &messages::SPECIFY_A_FILE_THAT_BUNDLES_ALL_OUTPUTS_INTO_ONE_JAVASCRIPT_FILE_IF_DECLARATION_IS_TRUE_ALSO_DESIGNATES_A_FILE_THAT_BUNDLES_ALL_D_TS_OUTPUT,
    },
    AllHelpOption {
        name: "outDir",
        short: None,
        category: &messages::EMIT,
        description: &messages::SPECIFY_AN_OUTPUT_FOLDER_FOR_ALL_EMITTED_FILES,
    },
    AllHelpOption {
        name: "rootDir",
        short: None,
        category: &messages::MODULES,
        description: &messages::SPECIFY_THE_ROOT_FOLDER_WITHIN_YOUR_SOURCE_FILES,
    },
    AllHelpOption {
        name: "tsBuildInfoFile",
        short: None,
        category: &messages::PROJECTS,
        description: &messages::SPECIFY_THE_PATH_TO_TSBUILDINFO_INCREMENTAL_COMPILATION_FILE,
    },
    AllHelpOption {
        name: "removeComments",
        short: None,
        category: &messages::EMIT,
        description: &messages::DISABLE_EMITTING_COMMENTS,
    },
    AllHelpOption {
        name: "importHelpers",
        short: None,
        category: &messages::EMIT,
        description: &messages::ALLOW_IMPORTING_HELPER_FUNCTIONS_FROM_TSLIB_ONCE_PER_PROJECT_INSTEAD_OF_INCLUDING_THEM_PER_FILE,
    },
    AllHelpOption {
        name: "downlevelIteration",
        short: None,
        category: &messages::EMIT,
        description: &messages::EMIT_MORE_COMPLIANT_BUT_VERBOSE_AND_LESS_PERFORMANT_JAVASCRIPT_FOR_ITERATION,
    },
    AllHelpOption {
        name: "verbatimModuleSyntax",
        short: None,
        category: &messages::INTEROP_CONSTRAINTS,
        description: &messages::DO_NOT_TRANSFORM_OR_ELIDE_ANY_IMPORTS_OR_EXPORTS_NOT_MARKED_AS_TYPE_ONLY_ENSURING_THEY_ARE_WRITTEN_IN_THE_OUTPUT_FILE_S_FORMAT_BASED_ON_THE_MODULE_SETTING,
    },
    AllHelpOption {
        name: "isolatedDeclarations",
        short: None,
        category: &messages::INTEROP_CONSTRAINTS,
        description: &messages::REQUIRE_SUFFICIENT_ANNOTATION_ON_EXPORTS_SO_OTHER_TOOLS_CAN_TRIVIALLY_GENERATE_DECLARATION_FILES,
    },
    AllHelpOption {
        name: "erasableSyntaxOnly",
        short: None,
        category: &messages::INTEROP_CONSTRAINTS,
        description: &messages::DO_NOT_ALLOW_RUNTIME_CONSTRUCTS_THAT_ARE_NOT_PART_OF_ECMASCRIPT,
    },
    AllHelpOption {
        name: "libReplacement",
        short: None,
        category: &messages::LANGUAGE_AND_ENVIRONMENT,
        description: &messages::ENABLE_LIB_REPLACEMENT,
    },
    AllHelpOption {
        name: "strict",
        short: None,
        category: &messages::TYPE_CHECKING,
        description: &messages::ENABLE_ALL_STRICT_TYPE_CHECKING_OPTIONS,
    },
    AllHelpOption {
        name: "strictNullChecks",
        short: None,
        category: &messages::TYPE_CHECKING,
        description: &messages::WHEN_TYPE_CHECKING_TAKE_INTO_ACCOUNT_NULL_AND_UNDEFINED,
    },
    AllHelpOption {
        name: "strictFunctionTypes",
        short: None,
        category: &messages::TYPE_CHECKING,
        description: &messages::WHEN_ASSIGNING_FUNCTIONS_CHECK_TO_ENSURE_PARAMETERS_AND_THE_RETURN_VALUES_ARE_SUBTYPE_COMPATIBLE,
    },
    AllHelpOption {
        name: "strictPropertyInitialization",
        short: None,
        category: &messages::TYPE_CHECKING,
        description: &messages::CHECK_FOR_CLASS_PROPERTIES_THAT_ARE_DECLARED_BUT_NOT_SET_IN_THE_CONSTRUCTOR,
    },
    AllHelpOption {
        name: "noImplicitThis",
        short: None,
        category: &messages::TYPE_CHECKING,
        description: &messages::ENABLE_ERROR_REPORTING_WHEN_THIS_IS_GIVEN_THE_TYPE_ANY,
    },
    AllHelpOption {
        name: "useUnknownInCatchVariables",
        short: None,
        category: &messages::TYPE_CHECKING,
        description: &messages::DEFAULT_CATCH_CLAUSE_VARIABLES_AS_UNKNOWN_INSTEAD_OF_ANY,
    },
    AllHelpOption {
        name: "alwaysStrict",
        short: None,
        category: &messages::TYPE_CHECKING,
        description: &messages::ENSURE_USE_STRICT_IS_ALWAYS_EMITTED,
    },
    AllHelpOption {
        name: "stableTypeOrdering",
        short: None,
        category: &messages::TYPE_CHECKING,
        description: &messages::ENSURE_TYPES_ARE_ORDERED_STABLY_AND_DETERMINISTICALLY_ACROSS_COMPILATIONS,
    },
    AllHelpOption {
        name: "noUnusedLocals",
        short: None,
        category: &messages::TYPE_CHECKING,
        description: &messages::ENABLE_ERROR_REPORTING_WHEN_LOCAL_VARIABLES_AREN_T_READ,
    },
    AllHelpOption {
        name: "noUnusedParameters",
        short: None,
        category: &messages::TYPE_CHECKING,
        description: &messages::RAISE_AN_ERROR_WHEN_A_FUNCTION_PARAMETER_ISN_T_READ,
    },
    AllHelpOption {
        name: "exactOptionalPropertyTypes",
        short: None,
        category: &messages::TYPE_CHECKING,
        description: &messages::INTERPRET_OPTIONAL_PROPERTY_TYPES_AS_WRITTEN_RATHER_THAN_ADDING_UNDEFINED,
    },
    AllHelpOption {
        name: "noFallthroughCasesInSwitch",
        short: None,
        category: &messages::TYPE_CHECKING,
        description: &messages::ENABLE_ERROR_REPORTING_FOR_FALLTHROUGH_CASES_IN_SWITCH_STATEMENTS,
    },
    AllHelpOption {
        name: "noUncheckedIndexedAccess",
        short: None,
        category: &messages::TYPE_CHECKING,
        description: &messages::ADD_UNDEFINED_TO_A_TYPE_WHEN_ACCESSED_USING_AN_INDEX,
    },
    AllHelpOption {
        name: "noPropertyAccessFromIndexSignature",
        short: None,
        category: &messages::TYPE_CHECKING,
        description: &messages::ENFORCES_USING_INDEXED_ACCESSORS_FOR_KEYS_DECLARED_USING_AN_INDEXED_TYPE,
    },
    AllHelpOption {
        name: "moduleResolution",
        short: None,
        category: &messages::MODULES,
        description: &messages::SPECIFY_HOW_TYPESCRIPT_LOOKS_UP_A_FILE_FROM_A_GIVEN_MODULE_SPECIFIER,
    },
    AllHelpOption {
        name: "baseUrl",
        short: None,
        category: &messages::MODULES,
        description: &messages::SPECIFY_THE_BASE_DIRECTORY_TO_RESOLVE_NON_RELATIVE_MODULE_NAMES,
    },
    AllHelpOption {
        name: "typeRoots",
        short: None,
        category: &messages::MODULES,
        description: &messages::SPECIFY_MULTIPLE_FOLDERS_THAT_ACT_LIKE_SLASHNODE_MODULES_SLASH_TYPES,
    },
    AllHelpOption {
        name: "allowSyntheticDefaultImports",
        short: None,
        category: &messages::INTEROP_CONSTRAINTS,
        description: &messages::ALLOW_IMPORT_X_FROM_Y_WHEN_A_MODULE_DOESN_T_HAVE_A_DEFAULT_EXPORT,
    },
    AllHelpOption {
        name: "esModuleInterop",
        short: None,
        category: &messages::INTEROP_CONSTRAINTS,
        description: &messages::EMIT_ADDITIONAL_JAVASCRIPT_TO_EASE_SUPPORT_FOR_IMPORTING_COMMONJS_MODULES_THIS_ENABLES_ALLOWSYNTHETICDEFAULTIMPORTS_FOR_TYPE_COMPATIBILITY,
    },
    AllHelpOption {
        name: "allowUmdGlobalAccess",
        short: None,
        category: &messages::MODULES,
        description: &messages::ALLOW_ACCESSING_UMD_GLOBALS_FROM_MODULES,
    },
    AllHelpOption {
        name: "moduleSuffixes",
        short: None,
        category: &messages::MODULES,
        description: &messages::LIST_OF_FILE_NAME_SUFFIXES_TO_SEARCH_WHEN_RESOLVING_A_MODULE,
    },
    AllHelpOption {
        name: "allowImportingTsExtensions",
        short: None,
        category: &messages::MODULES,
        description: &messages::ALLOW_IMPORTS_TO_INCLUDE_TYPESCRIPT_FILE_EXTENSIONS_REQUIRES_MODULERESOLUTION_BUNDLER_AND_EITHER_NOEMIT_OR_EMITDECLARATIONONLY_TO_BE_SET,
    },
    AllHelpOption {
        name: "rewriteRelativeImportExtensions",
        short: None,
        category: &messages::MODULES,
        description: &messages::REWRITE_TS_TSX_MTS_AND_CTS_FILE_EXTENSIONS_IN_RELATIVE_IMPORT_PATHS_TO_THEIR_JAVASCRIPT_EQUIVALENT_IN_OUTPUT_FILES,
    },
    AllHelpOption {
        name: "resolvePackageJsonExports",
        short: None,
        category: &messages::MODULES,
        description: &messages::USE_THE_PACKAGE_JSON_EXPORTS_FIELD_WHEN_RESOLVING_PACKAGE_IMPORTS,
    },
    AllHelpOption {
        name: "resolvePackageJsonImports",
        short: None,
        category: &messages::MODULES,
        description: &messages::USE_THE_PACKAGE_JSON_IMPORTS_FIELD_WHEN_RESOLVING_IMPORTS,
    },
    AllHelpOption {
        name: "customConditions",
        short: None,
        category: &messages::MODULES,
        description: &messages::CONDITIONS_TO_SET_IN_ADDITION_TO_THE_RESOLVER_SPECIFIC_DEFAULTS_WHEN_RESOLVING_IMPORTS,
    },
    AllHelpOption {
        name: "noUncheckedSideEffectImports",
        short: None,
        category: &messages::MODULES,
        description: &messages::CHECK_SIDE_EFFECT_IMPORTS,
    },
    AllHelpOption {
        name: "sourceRoot",
        short: None,
        category: &messages::EMIT,
        description: &messages::SPECIFY_THE_ROOT_PATH_FOR_DEBUGGERS_TO_FIND_THE_REFERENCE_SOURCE_CODE,
    },
    AllHelpOption {
        name: "mapRoot",
        short: None,
        category: &messages::EMIT,
        description: &messages::SPECIFY_THE_LOCATION_WHERE_DEBUGGER_SHOULD_LOCATE_MAP_FILES_INSTEAD_OF_GENERATED_LOCATIONS,
    },
    AllHelpOption {
        name: "inlineSources",
        short: None,
        category: &messages::EMIT,
        description: &messages::INCLUDE_SOURCE_CODE_IN_THE_SOURCEMAPS_INSIDE_THE_EMITTED_JAVASCRIPT,
    },
    AllHelpOption {
        name: "experimentalDecorators",
        short: None,
        category: &messages::LANGUAGE_AND_ENVIRONMENT,
        description: &messages::ENABLE_EXPERIMENTAL_SUPPORT_FOR_LEGACY_EXPERIMENTAL_DECORATORS,
    },
    AllHelpOption {
        name: "emitDecoratorMetadata",
        short: None,
        category: &messages::LANGUAGE_AND_ENVIRONMENT,
        description: &messages::EMIT_DESIGN_TYPE_METADATA_FOR_DECORATED_DECLARATIONS_IN_SOURCE_FILES,
    },
    AllHelpOption {
        name: "jsxFactory",
        short: None,
        category: &messages::LANGUAGE_AND_ENVIRONMENT,
        description: &messages::SPECIFY_THE_JSX_FACTORY_FUNCTION_USED_WHEN_TARGETING_REACT_JSX_EMIT_E_G_REACT_CREATEELEMENT_OR_H,
    },
    AllHelpOption {
        name: "jsxFragmentFactory",
        short: None,
        category: &messages::LANGUAGE_AND_ENVIRONMENT,
        description: &messages::SPECIFY_THE_JSX_FRAGMENT_REFERENCE_USED_FOR_FRAGMENTS_WHEN_TARGETING_REACT_JSX_EMIT_E_G_REACT_FRAGMENT_OR_FRAGMENT,
    },
    AllHelpOption {
        name: "jsxImportSource",
        short: None,
        category: &messages::LANGUAGE_AND_ENVIRONMENT,
        description: &messages::SPECIFY_MODULE_SPECIFIER_USED_TO_IMPORT_THE_JSX_FACTORY_FUNCTIONS_WHEN_USING_JSX_COLON_REACT_JSX_ASTERISK,
    },
    AllHelpOption {
        name: "resolveJsonModule",
        short: None,
        category: &messages::MODULES,
        description: &messages::ENABLE_IMPORTING_JSON_FILES,
    },
    AllHelpOption {
        name: "reactNamespace",
        short: None,
        category: &messages::LANGUAGE_AND_ENVIRONMENT,
        description: &messages::SPECIFY_THE_OBJECT_INVOKED_FOR_CREATEELEMENT_THIS_ONLY_APPLIES_WHEN_TARGETING_REACT_JSX_EMIT,
    },
    AllHelpOption {
        name: "skipDefaultLibCheck",
        short: None,
        category: &messages::COMPLETENESS,
        description: &messages::SKIP_TYPE_CHECKING_D_TS_FILES_THAT_ARE_INCLUDED_WITH_TYPESCRIPT,
    },
    AllHelpOption {
        name: "emitBOM",
        short: None,
        category: &messages::EMIT,
        description: &messages::EMIT_A_UTF_8_BYTE_ORDER_MARK_BOM_IN_THE_BEGINNING_OF_OUTPUT_FILES,
    },
    AllHelpOption {
        name: "noErrorTruncation",
        short: None,
        category: &messages::OUTPUT_FORMATTING,
        description: &messages::DISABLE_TRUNCATING_TYPES_IN_ERROR_MESSAGES,
    },
    AllHelpOption {
        name: "noLib",
        short: None,
        category: &messages::LANGUAGE_AND_ENVIRONMENT,
        description: &messages::DISABLE_INCLUDING_ANY_LIBRARY_FILES_INCLUDING_THE_DEFAULT_LIB_D_TS,
    },
    AllHelpOption {
        name: "noResolve",
        short: None,
        category: &messages::MODULES,
        description: &messages::DISALLOW_IMPORT_S_REQUIRE_S_OR_REFERENCE_S_FROM_EXPANDING_THE_NUMBER_OF_FILES_TYPESCRIPT_SHOULD_ADD_TO_A_PROJECT,
    },
    AllHelpOption {
        name: "stripInternal",
        short: None,
        category: &messages::EMIT,
        description: &messages::DISABLE_EMITTING_DECLARATIONS_THAT_HAVE_INTERNAL_IN_THEIR_JSDOC_COMMENTS,
    },
    AllHelpOption {
        name: "disableSizeLimit",
        short: None,
        category: &messages::EDITOR_SUPPORT,
        description: &messages::REMOVE_THE_20MB_CAP_ON_TOTAL_SOURCE_CODE_SIZE_FOR_JAVASCRIPT_FILES_IN_THE_TYPESCRIPT_LANGUAGE_SERVER,
    },
    AllHelpOption {
        name: "disableSourceOfProjectReferenceRedirect",
        short: None,
        category: &messages::PROJECTS,
        description: &messages::DISABLE_PREFERRING_SOURCE_FILES_INSTEAD_OF_DECLARATION_FILES_WHEN_REFERENCING_COMPOSITE_PROJECTS,
    },
    AllHelpOption {
        name: "disableSolutionSearching",
        short: None,
        category: &messages::PROJECTS,
        description: &messages::OPT_A_PROJECT_OUT_OF_MULTI_PROJECT_REFERENCE_CHECKING_WHEN_EDITING,
    },
    AllHelpOption {
        name: "disableReferencedProjectLoad",
        short: None,
        category: &messages::PROJECTS,
        description: &messages::REDUCE_THE_NUMBER_OF_PROJECTS_LOADED_AUTOMATICALLY_BY_TYPESCRIPT,
    },
    AllHelpOption {
        name: "noEmitHelpers",
        short: None,
        category: &messages::EMIT,
        description: &messages::DISABLE_GENERATING_CUSTOM_HELPER_FUNCTIONS_LIKE_EXTENDS_IN_COMPILED_OUTPUT,
    },
    AllHelpOption {
        name: "noEmitOnError",
        short: None,
        category: &messages::EMIT,
        description: &messages::DISABLE_EMITTING_FILES_IF_ANY_TYPE_CHECKING_ERRORS_ARE_REPORTED,
    },
    AllHelpOption {
        name: "preserveConstEnums",
        short: None,
        category: &messages::EMIT,
        description: &messages::DISABLE_ERASING_CONST_ENUM_DECLARATIONS_IN_GENERATED_CODE,
    },
    AllHelpOption {
        name: "declarationDir",
        short: None,
        category: &messages::EMIT,
        description: &messages::SPECIFY_THE_OUTPUT_DIRECTORY_FOR_GENERATED_DECLARATION_FILES,
    },
    AllHelpOption {
        name: "skipLibCheck",
        short: None,
        category: &messages::COMPLETENESS,
        description: &messages::SKIP_TYPE_CHECKING_ALL_D_TS_FILES,
    },
    AllHelpOption {
        name: "allowUnusedLabels",
        short: None,
        category: &messages::TYPE_CHECKING,
        description: &messages::DISABLE_ERROR_REPORTING_FOR_UNUSED_LABELS,
    },
    AllHelpOption {
        name: "allowUnreachableCode",
        short: None,
        category: &messages::TYPE_CHECKING,
        description: &messages::DISABLE_ERROR_REPORTING_FOR_UNREACHABLE_CODE,
    },
    AllHelpOption {
        name: "forceConsistentCasingInFileNames",
        short: None,
        category: &messages::INTEROP_CONSTRAINTS,
        description: &messages::ENSURE_THAT_CASING_IS_CORRECT_IN_IMPORTS,
    },
    AllHelpOption {
        name: "maxNodeModuleJsDepth",
        short: None,
        category: &messages::JAVASCRIPT_SUPPORT,
        description: &messages::SPECIFY_THE_MAXIMUM_FOLDER_DEPTH_USED_FOR_CHECKING_JAVASCRIPT_FILES_FROM_NODE_MODULES_ONLY_APPLICABLE_WITH_ALLOWJS,
    },
    AllHelpOption {
        name: "useDefineForClassFields",
        short: None,
        category: &messages::LANGUAGE_AND_ENVIRONMENT,
        description: &messages::EMIT_ECMASCRIPT_STANDARD_COMPLIANT_CLASS_FIELDS,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_names_a_declared_option() {
        for option in ALL_HELP_OPTIONS {
            assert!(
                tsr_tsoptions::command_line::find_by_command_line_name(option.name).is_some(),
                "--{} is listed by --help --all but is not declared",
                option.name
            );
        }
    }

    #[test]
    fn every_short_name_matches_the_declaration() {
        for option in ALL_HELP_OPTIONS {
            let Some(short) = option.short else { continue };
            assert!(
                tsr_tsoptions::declarations::COMPILER_OPTIONS.iter().any(|declaration| {
                    declaration.name == option.name && declaration.short_name == Some(short)
                }),
                "--{} is documented as -{short}",
                option.name
            );
        }
    }

    #[test]
    fn categories_are_not_contiguous_which_is_why_grouping_is_by_map() {
        // **Written as an assertion because the first version of the renderer
        // assumed the opposite.** Options are declared in a useful order for
        // *parsing* and their categories interleave — `Output Formatting`
        // appears, stops, and comes back. A renderer that emitted a heading
        // whenever the category changed would print some headings twice and,
        // worse, look correct on the first few.
        //
        // `generateSectionOptionsOutput` (`help.go:170`) builds a map plus a
        // `categoryOrder` of first appearances, which is what `grouped` does.
        let mut previous = "";
        let mut breaks = 0;
        let mut seen: Vec<&str> = Vec::new();
        for option in ALL_HELP_OPTIONS {
            let category = option.category.text();
            if category != previous {
                if seen.contains(&category) {
                    breaks += 1;
                }
                seen.push(category);
                previous = category;
            }
        }
        assert!(breaks > 0, "if this ever reaches zero, the map is no longer load-bearing");
    }

    #[test]
    fn grouping_preserves_first_appearance_order_and_loses_nothing() {
        let groups = grouped();
        let total: usize = groups.iter().map(|(_, options)| options.len()).sum();
        assert_eq!(total, ALL_HELP_OPTIONS.len(), "every option lands in exactly one group");

        let headings: Vec<&str> = groups.iter().map(|(name, _)| *name).collect();
        let mut deduped = headings.clone();
        deduped.dedup();
        assert_eq!(headings, deduped, "each category is printed once");

        // The first category is the *alphabetically first option's*, not the
        // first declared one — `--all` sorts before it groups.
        let mut names: Vec<&str> = ALL_HELP_OPTIONS.iter().map(|o| o.name).collect();
        names.sort_by_key(|name| name.to_ascii_lowercase());
        let first = ALL_HELP_OPTIONS.iter().find(|o| o.name == names[0]).expect("declared");
        assert_eq!(headings[0], first.category.text());
    }
}
