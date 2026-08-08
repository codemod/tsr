//! `tsc --help`.
//!
//! Ported from `internal/execute/tsc/help.go` at the pinned commit —
//! `printEasyHelp` (`:67`), `generateSectionOptionsOutput` (`:149`),
//! `generateOptionOutput` (`:222`) and `getHeader` (`:44`).
//!
//! # Two width regimes, and only one is ported
//!
//! `generateOptionOutput` branches on `terminalWidth >= 80`. Above it, each
//! option is a two-column layout with the description wrapped and aligned
//! against the widest option name; below it — and at the harness's **default
//! width of 0** (`tsctests/sys.go:227`) — each option is three plain lines.
//!
//! The narrow form is ported. The wide one is not: it needs `getPrettyOutput`'s
//! wrapping and the blue-background "TS" icon in the header, and exactly one
//! baseline exercises it (`show-help-with-ExitStatus.DiagnosticsPresent_OutputsSkipped.js`,
//! which sets `TS_TEST_TERMINAL_WIDTH`). Named here rather than approximated.
//!
//! # `--help --all` is not ported
//!
//! It lists all ~135 compiler options plus the watch and build sections, each
//! with a description, a category and a default. This port declares 71 options
//! and has no watch or build table, so the output could only be a subset
//! pretending to be the whole. `--all` falls back to the simplified view.

use std::fmt::Write as _;

use crate::system::System;

/// One line of the simplified help (`showInSimplifiedHelpView` options).
struct HelpOption {
    /// The option name, without dashes.
    name: &'static str,
    /// Its one-letter spelling, if it has one.
    short: Option<&'static str>,
    /// The description, verbatim from the diagnostic catalogue.
    description: &'static str,
    /// The `type:` or `one of:` line, when the option has one.
    value_line: Option<&'static str>,
    /// The `default:` line, when the option has one.
    default_line: Option<&'static str>,
}

/// `tsc --help`'s command-line section, in upstream's order.
///
/// # Why the text is here and not on the declaration
///
/// Upstream hangs `Description`, `Category`, `DefaultValueDescription` and
/// `ShowInSimplifiedHelpView` off every `CommandLineOption`. Carrying four more
/// fields on all 71 declarations to populate 27 of them would put help
/// presentation into the table the *parser* reads, and the parser is the part
/// that has to stay small and obviously correct.
///
/// The cost is that option names appear twice, which is exactly the duplication
/// this project distrusts — so
/// [`tests::every_help_option_is_a_real_option`] fails the build if a name here
/// is not a declared option, and
/// [`tests::every_description_is_a_real_diagnostic_message`] fails it if a
/// description is not verbatim from the generated catalogue. Neither can drift
/// silently.
const COMMAND_LINE_OPTIONS: &[HelpOption] = &[
    HelpOption {
        name: "help",
        short: Some("h"),
        description: "Print this message.",
        value_line: None,
        default_line: None,
    },
    HelpOption {
        name: "watch",
        short: Some("w"),
        description: "Watch input files.",
        value_line: None,
        default_line: None,
    },
    HelpOption {
        name: "all",
        short: None,
        description: "Show all compiler options.",
        value_line: None,
        default_line: None,
    },
    HelpOption {
        name: "version",
        short: Some("v"),
        description: "Print the compiler's version.",
        value_line: None,
        default_line: None,
    },
    HelpOption {
        name: "init",
        short: None,
        description: "Initializes a TypeScript project and creates a tsconfig.json file.",
        value_line: None,
        default_line: None,
    },
    HelpOption {
        name: "project",
        short: Some("p"),
        description: "Compile the project given the path to its configuration file, or to a folder with a 'tsconfig.json'.",
        value_line: None,
        default_line: None,
    },
    HelpOption {
        name: "showConfig",
        short: None,
        description: "Print the final configuration instead of building.",
        value_line: None,
        default_line: None,
    },
    HelpOption {
        name: "ignoreConfig",
        short: None,
        description: "Ignore the tsconfig found and build with commandline options and files.",
        value_line: None,
        default_line: None,
    },
    HelpOption {
        name: "build",
        short: Some("b"),
        description: "Build one or more projects and their dependencies, if out of date",
        value_line: None,
        default_line: None,
    },
];

/// `tsc --help`'s compiler-option section, in upstream's order.
const COMMON_COMPILER_OPTIONS: &[HelpOption] = &[
    HelpOption {
        name: "pretty",
        short: None,
        description: "Enable color and formatting in TypeScript's output to make compiler errors easier to read.",
        value_line: Some("type: boolean"),
        default_line: Some("default: true"),
    },
    HelpOption {
        name: "declaration",
        short: Some("d"),
        description: "Generate .d.ts files from TypeScript and JavaScript files in your project.",
        value_line: Some("type: boolean"),
        default_line: Some("default: `false`, unless `composite` is set"),
    },
    HelpOption {
        name: "declarationMap",
        short: None,
        description: "Create sourcemaps for d.ts files.",
        value_line: Some("type: boolean"),
        default_line: Some("default: false"),
    },
    HelpOption {
        name: "emitDeclarationOnly",
        short: None,
        description: "Only output d.ts files and not JavaScript files.",
        value_line: Some("type: boolean"),
        default_line: Some("default: false"),
    },
    HelpOption {
        name: "sourceMap",
        short: None,
        description: "Create source map files for emitted JavaScript files.",
        value_line: Some("type: boolean"),
        default_line: Some("default: false"),
    },
    HelpOption {
        name: "noEmit",
        short: None,
        description: "Disable emitting files from a compilation.",
        value_line: Some("type: boolean"),
        default_line: Some("default: false"),
    },
    HelpOption {
        name: "target",
        short: Some("t"),
        description: "Set the JavaScript language version for emitted JavaScript and include compatible library declarations.",
        value_line: Some(
            "one of: es6/es2015, es2016, es2017, es2018, es2019, es2020, es2021, es2022, es2023, es2024, es2025, esnext",
        ),
        default_line: Some("default: es2025"),
    },
    HelpOption {
        name: "module",
        short: Some("m"),
        description: "Specify what module code is generated.",
        value_line: Some(
            "one of: commonjs, es6/es2015, es2020, es2022, esnext, node16, node18, node20, nodenext, preserve",
        ),
        default_line: Some("default: undefined"),
    },
    HelpOption {
        name: "lib",
        short: None,
        description: "Specify a set of bundled library declaration files that describe the target runtime environment.",
        value_line: Some(LIB_VALUES),
        default_line: Some("default: undefined"),
    },
    HelpOption {
        name: "allowJs",
        short: None,
        description: "Allow JavaScript files to be a part of your program. Use the 'checkJs' option to get errors from these files.",
        value_line: Some("type: boolean"),
        default_line: Some("default: `false`, unless `checkJs` is set"),
    },
    HelpOption {
        name: "checkJs",
        short: None,
        description: "Enable error reporting in type-checked JavaScript files.",
        value_line: Some("type: boolean"),
        default_line: Some("default: false"),
    },
    HelpOption {
        name: "jsx",
        short: None,
        description: "Specify what JSX code is generated.",
        value_line: Some("one of: preserve, react-native, react-jsx, react-jsxdev, react"),
        default_line: Some("default: undefined"),
    },
    HelpOption {
        name: "outFile",
        short: None,
        description: "Specify a file that bundles all outputs into one JavaScript file. If 'declaration' is true, also designates a file that bundles all .d.ts output.",
        value_line: None,
        default_line: None,
    },
    HelpOption {
        name: "outDir",
        short: None,
        description: "Specify an output folder for all emitted files.",
        value_line: None,
        default_line: None,
    },
    HelpOption {
        name: "removeComments",
        short: None,
        description: "Disable emitting comments.",
        value_line: Some("type: boolean"),
        default_line: Some("default: false"),
    },
    HelpOption {
        name: "strict",
        short: None,
        description: "Enable all strict type-checking options.",
        value_line: Some("type: boolean"),
        default_line: Some("default: true"),
    },
    HelpOption {
        name: "types",
        short: None,
        description: "Specify type package names to be included without being referenced in a source file.",
        value_line: None,
        default_line: None,
    },
    HelpOption {
        name: "esModuleInterop",
        short: None,
        description: "Emit additional JavaScript to ease support for importing CommonJS modules. This enables 'allowSyntheticDefaultImports' for type compatibility.",
        value_line: Some("type: boolean"),
        default_line: Some("default: true"),
    },
];

/// `--lib`'s accepted values, as `--help` lists them.
///
/// Upstream builds this from the `libMap` enum, whose keys carry `/`-joined
/// aliases (`es2018.asynciterable/esnext.asynciterable`) that only the help
/// printer renders. One string rather than a list because nothing else reads it.
const LIB_VALUES: &str = "one or more: es5, es6/es2015, es7/es2016, es2017, es2018, es2019, es2020, es2021, es2022, es2023, es2024, es2025, esnext, dom, dom.iterable, dom.asynciterable, webworker, webworker.importscripts, webworker.iterable, webworker.asynciterable, scripthost, es2015.core, es2015.collection, es2015.generator, es2015.iterable, es2015.promise, es2015.proxy, es2015.reflect, es2015.symbol, es2015.symbol.wellknown, es2016.array.include, es2016.intl, es2017.arraybuffer, es2017.date, es2017.object, es2017.sharedmemory, es2017.string, es2017.intl, es2017.typedarrays, es2018.asyncgenerator, es2018.asynciterable/esnext.asynciterable, es2018.intl, es2018.promise, es2018.regexp, es2019.array, es2019.object, es2019.string, es2019.symbol/esnext.symbol, es2019.intl, es2020.bigint/esnext.bigint, es2020.date, es2020.promise, es2020.sharedmemory, es2020.string, es2020.symbol.wellknown, es2020.intl, es2020.number, es2021.promise, es2021.string, es2021.weakref/esnext.weakref, es2021.intl, es2022.array, es2022.error, es2022.intl, es2022.object, es2022.string, es2022.regexp, es2023.array, es2023.collection, es2023.intl, es2024.arraybuffer, es2024.collection, es2024.object/esnext.object, es2024.promise, es2024.regexp/esnext.regexp, es2024.sharedmemory, es2024.string/esnext.string, es2025.collection, es2025.float16/esnext.float16, es2025.intl, es2025.iterator/esnext.iterator, es2025.promise/esnext.promise, es2025.regexp, esnext.array, esnext.collection, esnext.date, esnext.decorators, esnext.disposable, esnext.error, esnext.intl, esnext.sharedmemory, esnext.temporal, esnext.typedarrays, decorators, decorators.legacy";

/// Bold on, bold off (`colors.bold`).
const BOLD: (&str, &str) = ("\u{1b}[1m", "\u{1b}[22m");
/// Blue foreground, and the reset that follows it (`colors.blue`).
const BLUE: (&str, &str) = ("\u{1b}[94m", "\u{1b}[39m");

/// Whether this run should colour its help (`createColors`).
///
/// `NO_COLOR` wins over `FORCE_COLOR`, which is the order the baseline
/// `does-not-add-color-when-NO_COLOR-is-set-even-if-FORCE_COLOR-is-set.js`
/// exists to pin.
fn use_colors(sys: &dyn System) -> bool {
    if !sys.environment_variable("NO_COLOR").is_empty() {
        return false;
    }
    if !sys.environment_variable("FORCE_COLOR").is_empty() {
        return true;
    }
    true
}

/// `PrintHelp` — the simplified view.
///
/// `--all` is accepted and prints the same thing; see the module docs.
pub fn print_help(sys: &mut dyn System, _all: bool) {
    let colors = use_colors(sys);
    let paint = |text: &str, (on, off): (&str, &str)| {
        if colors { format!("{on}{text}{off}") } else { text.to_string() }
    };

    let mut output = String::new();
    let _ = writeln!(output, "tsc: The TypeScript Compiler - Version {}\n", sys.version());

    let _ = writeln!(output, "{}\n", paint("COMMON COMMANDS", BOLD));

    // `printEasyHelp`'s seven examples, in order. The last has two commands
    // under one description, which is why this takes a slice.
    for (commands, description) in [
        (&["tsc"][..], "Compiles the current project (tsconfig.json in the working directory.)"),
        (
            &["tsc app.ts util.ts"][..],
            "Ignoring tsconfig.json, compiles the specified files with default compiler options.",
        ),
        (&["tsc -b"][..], "Build a composite project in the working directory."),
        (
            &["tsc --init"][..],
            "Creates a tsconfig.json with the recommended settings in the working directory.",
        ),
        (
            &["tsc -p ./path/to/tsconfig.json"][..],
            "Compiles the TypeScript project located at the specified path.",
        ),
        (
            &["tsc --help --all"][..],
            "An expanded version of this information, showing all possible compiler options",
        ),
        (
            &["tsc --noEmit", "tsc --target esnext"][..],
            "Compiles the current project, with additional settings.",
        ),
    ] {
        for command in commands {
            let _ = writeln!(output, "  {}", paint(command, BLUE));
        }
        let _ = writeln!(output, "  {description}\n");
    }

    write_section(&mut output, "COMMAND LINE FLAGS", COMMAND_LINE_OPTIONS, colors, None);
    write_section(
        &mut output,
        "COMMON COMPILER OPTIONS",
        COMMON_COMPILER_OPTIONS,
        colors,
        Some("You can learn about all of the compiler options at https://aka.ms/tsc"),
    );

    sys.write(&output);
}

/// One `generateSectionOptionsOutput` section, in the narrow layout.
fn write_section(
    output: &mut String,
    title: &str,
    options: &[HelpOption],
    colors: bool,
    after: Option<&str>,
) {
    let paint = |text: &str, (on, off): (&str, &str)| {
        if colors { format!("{on}{text}{off}") } else { text.to_string() }
    };

    let _ = writeln!(output, "{}\n", paint(title, BOLD));

    for option in options {
        let mut name = format!("--{}", option.name);
        if let Some(short) = option.short {
            let _ = write!(name, ", -{short}");
        }
        let _ = writeln!(output, "{}", paint(&name, BLUE));
        let _ = writeln!(output, "{}", option.description);
        // `showAdditionalInfoOutput`: the type/values line and the default are
        // one block, and an option with neither prints no blank line between
        // them — which is why these are not two independent `if`s.
        if option.value_line.is_some() || option.default_line.is_some() {
            if let Some(values) = option.value_line {
                let _ = writeln!(output, "{values}");
            }
            if let Some(default) = option.default_line {
                let _ = writeln!(output, "{default}");
            }
        }
        output.push('\n');
    }

    if let Some(after) = after {
        let _ = writeln!(output, "{after}\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_options() -> impl Iterator<Item = &'static HelpOption> {
        COMMAND_LINE_OPTIONS.iter().chain(COMMON_COMPILER_OPTIONS.iter())
    }

    #[test]
    fn every_help_option_is_a_real_option() {
        // The guard against this table drifting from the parser's. `build` is
        // the one exception: it is a *mode*, dispatched before the option table
        // is consulted (`execute::command_line`), so it has no declaration.
        for option in all_options() {
            if option.name == "build" {
                continue;
            }
            assert!(
                tsr_tsoptions::command_line::find_by_command_line_name(option.name).is_some(),
                "--{} is in the help but is not a declared option",
                option.name
            );
        }
    }

    #[test]
    fn every_short_name_matches_the_declaration() {
        // A help that advertises `-x` for an option the parser spells `-y` is
        // worse than one that omits it.
        for option in all_options() {
            let Some(short) = option.short else { continue };
            if option.name == "build" {
                continue;
            }
            let found = tsr_tsoptions::declarations::COMPILER_OPTIONS.iter().any(|declaration| {
                declaration.name == option.name && declaration.short_name == Some(short)
            });
            assert!(found, "--{} is documented as -{short}", option.name);
        }
    }

    #[test]
    fn every_description_is_a_real_diagnostic_message() {
        // Proof that no description here was written by hand: each is the
        // verbatim text of a message in the generated catalogue, which is
        // generated from upstream's `diagnosticMessages.json`.
        for option in all_options() {
            assert!(
                tsr_diagnostics::messages::ALL
                    .iter()
                    .any(|message| message.text() == option.description),
                "--{}'s description is not a diagnostic message: {:?}",
                option.name,
                option.description
            );
        }
    }

    #[test]
    fn the_option_order_is_upstreams() {
        // Taken from `ignoreConfig/without-any-options-when-config-file-absent.js`,
        // which is the expected output byte for byte.
        let names: Vec<&str> = all_options().map(|option| option.name).collect();
        assert_eq!(
            names,
            [
                "help",
                "watch",
                "all",
                "version",
                "init",
                "project",
                "showConfig",
                "ignoreConfig",
                "build",
                "pretty",
                "declaration",
                "declarationMap",
                "emitDeclarationOnly",
                "sourceMap",
                "noEmit",
                "target",
                "module",
                "lib",
                "allowJs",
                "checkJs",
                "jsx",
                "outFile",
                "outDir",
                "removeComments",
                "strict",
                "types",
                "esModuleInterop",
            ]
        );
    }
}
