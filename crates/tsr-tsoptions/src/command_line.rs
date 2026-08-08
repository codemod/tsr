//! `tsc`'s command line, parsed.
//!
//! Ported from `internal/tsoptions/commandlineparser.go` (`ParseCommandLine`,
//! `parseStrings`, `parseOptionValue`, `parseResponseFile`) and
//! `internal/tsoptions/errors.go` (`createUnknownOptionError`,
//! `createDiagnosticForInvalidEnumType`) at the pinned commit.
//!
//! # This is not a general-purpose argument parser, and `clap` was refused
//!
//! Recorded in `STATUS-cli.md` §6 with its reasons. The short version is that
//! `tsc`'s surface has five behaviours a derive-macro parser cannot express, and
//! every one of them is exercised by the baselines:
//!
//! 1. **A boolean takes an optional following value.** `--strict false` consumes
//!    the token; `--strict --noEmit` does not; `--strict wibble` sets it to
//!    *true* and leaves `wibble` as a file name.
//! 2. **At most two leading dashes are stripped**, so `-target`, `--target` and
//!    `---target` are one option and a *three*-dash spelling is `-target`.
//! 3. **`@file` expands inline**, with its own quoting rules, recursively.
//! 4. **Unknown options get a spelling suggestion** from the same Levenshtein
//!    variant the checker uses, and the suggestion is in the baselined output.
//! 5. **A `tsconfig`-only option is a diagnostic, not an unknown option**, and
//!    which diagnostic depends on the option's kind and on what followed it.
//!
//! # Everything here goes through the one option table
//!
//! [`crate::declarations::COMPILER_OPTIONS`] carries each option's name, kind
//! and setter together. The parser converts an argument into a
//! [`ConfigValue`] and hands it to the same `apply` a `tsconfig.json` key would
//! reach, so the command line and the config file cannot disagree about what
//! `--target es2015` means. Upstream needs `ParseCompilerOptions`, a 400-case
//! switch, to do the same job.

use tsr_core::CompilerOptions;
use tsr_diagnostics::{Diagnostic, messages};
use tsr_path::get_normalized_absolute_path;
use tsr_vfs::FileSystem;

use crate::declarations::{COMPILER_OPTIONS, OptionDeclaration, OptionKind};
use crate::value::ConfigValue;

/// A command line, parsed (`tsoptions.ParsedCommandLine` for the argv case).
///
/// Deliberately *not* [`crate::ParsedCommandLine`], which a config file
/// produces: that one carries `literal_file_count` and the config's raw keys,
/// neither of which a command line has. The two meet in the driver, where
/// command-line options are layered over the config's.
#[derive(Debug, Default)]
pub struct ParsedCommandLine {
    /// The options the arguments set.
    pub compiler_options: CompilerOptions,
    /// Which options were written at all, by name.
    ///
    /// Needed because the driver has to layer the command line *over* a config
    /// file, and `Tristate::Unknown` cannot distinguish "not written" from
    /// "written as the default" for a `String` or a number. Upstream keeps the
    /// same information in `ParsedCommandLine.Raw`.
    pub set_options: Vec<String>,
    /// Everything that was not an option — the root files.
    pub file_names: Vec<String>,
    /// What was wrong with the command line.
    pub errors: Vec<Diagnostic>,
}

impl ParsedCommandLine {
    /// Whether `name` was written on the command line.
    #[must_use]
    pub fn was_set(&self, name: &str) -> bool {
        self.set_options.iter().any(|written| written == name)
    }
}

/// Look an option up by the name as typed (`NameMap.GetOptionDeclarationFromName`).
///
/// Case-**in**sensitive, unlike [`crate::declarations::find`], which a config
/// file uses: `tsc --TARGET es5` works and `{"TARGET": "es5"}` is an unknown
/// option. That asymmetry is upstream's, and it is why the two lookups are two
/// functions rather than one with a flag.
///
/// A short name is tried first, and only as an exact whole-argument match: `-p`
/// is `project`, but `-pr` is not.
#[must_use]
pub fn find_by_command_line_name(name: &str) -> Option<&'static OptionDeclaration> {
    let lowered = name.to_ascii_lowercase();
    if let Some(option) = COMPILER_OPTIONS
        .iter()
        .find(|option| option.short_name.is_some_and(|short| short == lowered))
    {
        return Some(option);
    }
    COMPILER_OPTIONS.iter().find(|option| option.name.eq_ignore_ascii_case(&lowered))
}

/// Parse `args` as a `tsc` command line (`ParseCommandLine`).
///
/// `fs` is needed only for `@response` files; a caller with no filesystem may
/// pass one that reads nothing, and a response file then reports
/// `Cannot read file '…'` exactly as upstream's nil-FS branch does.
#[must_use]
pub fn parse_command_line(
    args: &[String],
    fs: &dyn FileSystem,
    current_directory: &str,
) -> ParsedCommandLine {
    let mut parser = Parser {
        fs,
        current_directory,
        result: ParsedCommandLine::default(),
        response_file_depth: 0,
    };
    parser.parse_strings(args);
    parser.result
}

struct Parser<'a> {
    fs: &'a dyn FileSystem,
    current_directory: &'a str,
    result: ParsedCommandLine,
    /// How deep the `@file` expansion currently is.
    ///
    /// **No upstream counterpart.** Upstream recurses without a bound and a
    /// response file naming itself is a stack overflow; a compiler that a build
    /// system invokes should not be crashable by a file it was pointed at. The
    /// limit is deliberately far above any real usage, so no legitimate command
    /// line can reach it, and hitting it reports the same `Cannot read file`
    /// diagnostic rather than inventing a new message.
    response_file_depth: u32,
}

/// How deep `@file` expansion may nest. See [`Parser::response_file_depth`].
const MAX_RESPONSE_FILE_DEPTH: u32 = 32;

impl Parser<'_> {
    /// `parseStrings` — walk the arguments, dispatching on the first byte.
    fn parse_strings(&mut self, args: &[String]) {
        let mut index = 0;
        while index < args.len() {
            let argument = args[index].clone();
            index += 1;
            if argument.is_empty() {
                continue;
            }
            match argument.as_bytes()[0] {
                b'@' => self.parse_response_file(&argument[1..]),
                b'-' => {
                    let name = input_option_name(&argument);
                    if let Some(option) = find_by_command_line_name(name) {
                        index = self.parse_option_value(args, index, option);
                    } else {
                        // Upstream tries the *watch* option table here before
                        // erroring (`commandlineparser.go:150`). This port has
                        // no watch options — `WatchOptions` is not ported — so a
                        // `--watchFile` reports as unknown where upstream
                        // accepts it. Recorded in STATUS-cli.md rather than
                        // faked with a table of names that set nothing.
                        let error = Self::unknown_option_error(name, &argument);
                        self.result.errors.push(error);
                    }
                }
                _ => self.result.file_names.push(argument),
            }
        }
    }

    /// `parseResponseFile` — expand `@file` in place.
    ///
    /// The tokeniser is upstream's and is not shell quoting: a double-quoted run
    /// is one argument with the quotes stripped, anything else runs to the next
    /// character at or below `' '`, and there is no escape character. A
    /// backslash is an ordinary byte, which is what makes Windows paths work
    /// unquoted.
    fn parse_response_file(&mut self, file_name: &str) {
        if self.response_file_depth >= MAX_RESPONSE_FILE_DEPTH {
            self.result.errors.push(Diagnostic::with_args(
                &messages::CANNOT_READ_FILE_0,
                tsr_core::Span::new(0, 0),
                [file_name.to_string()],
            ));
            return;
        }

        let absolute = get_normalized_absolute_path(file_name, self.current_directory);
        let Some(contents) = self.fs.read_file(&absolute) else {
            // Upstream notes a divergence of its own here: Go's error is not
            // reported, only the file name.
            self.result.errors.push(Diagnostic::with_args(
                &messages::CANNOT_READ_FILE_0,
                tsr_core::Span::new(0, 0),
                [absolute],
            ));
            return;
        };
        if contents.is_empty() {
            return;
        }

        let text: Vec<char> = contents.chars().collect();
        let mut args = Vec::new();
        let mut pos = 0;
        while pos < text.len() {
            while pos < text.len() && text[pos] <= ' ' {
                pos += 1;
            }
            if pos >= text.len() {
                break;
            }
            if text[pos] == '"' {
                pos += 1;
                let start = pos;
                while pos < text.len() && text[pos] != '"' {
                    pos += 1;
                }
                if pos < text.len() {
                    args.push(text[start..pos].iter().collect::<String>());
                    pos += 1;
                } else {
                    self.result.errors.push(Diagnostic::with_args(
                        &messages::UNTERMINATED_QUOTED_STRING_IN_RESPONSE_FILE_0,
                        tsr_core::Span::new(0, 0),
                        [absolute.clone()],
                    ));
                }
            } else {
                let start = pos;
                while pos < text.len() && text[pos] > ' ' {
                    pos += 1;
                }
                args.push(text[start..pos].iter().collect::<String>());
            }
        }

        self.response_file_depth += 1;
        self.parse_strings(&args);
        self.response_file_depth -= 1;
    }

    /// `parseOptionValue` — consume however many arguments this option takes.
    ///
    /// Returns the index of the next unconsumed argument. Every arm's decision
    /// about whether to advance is upstream's and is load-bearing: an option
    /// that wrongly consumes its successor turns a following flag into a value,
    /// and one that wrongly does not turns a value into a file name.
    fn parse_option_value(
        &mut self,
        args: &[String],
        mut index: usize,
        option: &'static OptionDeclaration,
    ) -> usize {
        if option.is_tsconfig_only {
            return self.parse_tsconfig_only_option(args, index, option);
        }

        // No argument left at all (`--locale` as the final token).
        if index >= args.len() {
            if option.kind == OptionKind::Boolean {
                self.set(option, &ConfigValue::Bool(true));
            } else {
                self.result.errors.push(Diagnostic::with_args(
                    &messages::COMPILER_OPTION_0_EXPECTS_AN_ARGUMENT,
                    tsr_core::Span::new(0, 0),
                    [option.name.to_string(), value_type_string(option)],
                ));
                if let OptionKind::List(_) = option.kind {
                    self.set(option, &ConfigValue::List(Vec::new()));
                } else if option.kind == OptionKind::Enum {
                    let error = invalid_enum_error(option);
                    self.result.errors.push(error);
                }
            }
            return index;
        }

        let argument = args[index].clone();
        if argument == "null" {
            // `--outDir null` clears the option rather than setting it to the
            // string "null".
            self.result.set_options.push(option.name.to_string());
            return index + 1;
        }

        match option.kind {
            OptionKind::Number => {
                if let Ok(number) = argument.parse::<f64>() {
                    self.set(option, &ConfigValue::Number(number));
                } else {
                    self.result.errors.push(Diagnostic::with_args(
                        &messages::COMPILER_OPTION_0_EXPECTS_AN_ARGUMENT,
                        tsr_core::Span::new(0, 0),
                        [option.name.to_string(), "number".to_string()],
                    ));
                }
                index += 1;
            }
            OptionKind::Boolean => {
                // The optional-value rule, and the one most easily got wrong:
                // anything that is not exactly `false` sets the option to
                // **true**, and only exactly `true` or `false` is consumed.
                self.set(option, &ConfigValue::Bool(argument != "false"));
                if argument == "false" || argument == "true" {
                    index += 1;
                }
            }
            OptionKind::String => {
                self.set(option, &ConfigValue::String(argument));
                index += 1;
            }
            OptionKind::List(_) | OptionKind::PathMap => {
                let (values, consumed) = Self::parse_list_option(&argument);
                self.set(option, &ConfigValue::List(values));
                if consumed {
                    index += 1;
                }
            }
            OptionKind::Enum => {
                let trimmed = argument.trim().to_string();
                if !self.set(option, &ConfigValue::String(trimmed)) {
                    let error = invalid_enum_error(option);
                    self.result.errors.push(error);
                }
                index += 1;
            }
        }
        index
    }

    /// The `IsTSConfigOnly` arm of `parseOptionValue`.
    ///
    /// Three outcomes, and the third is the surprising one: a boolean written as
    /// `--composite false` is **accepted**, because turning a config's option
    /// off from the command line is legitimate, while `--composite` or
    /// `--composite true` is an error.
    fn parse_tsconfig_only_option(
        &mut self,
        args: &[String],
        mut index: usize,
        option: &'static OptionDeclaration,
    ) -> usize {
        let value = args.get(index).cloned().unwrap_or_default();
        if value == "null" {
            self.result.set_options.push(option.name.to_string());
            return index + 1;
        }
        if option.kind == OptionKind::Boolean {
            if value == "false" {
                self.set(option, &ConfigValue::Bool(false));
                return index + 1;
            }
            if value == "true" {
                index += 1;
            }
            self.result.errors.push(Diagnostic::with_args(
                &messages::OPTION_0_CAN_ONLY_BE_SPECIFIED_IN_TSCONFIG_JSON_FILE_OR_SET_TO_FALSE_OR_NULL_ON_COMMAND_LINE,
                tsr_core::Span::new(0, 0),
                [option.name.to_string()],
            ));
            return index;
        }
        self.result.errors.push(Diagnostic::with_args(
            &messages::OPTION_0_CAN_ONLY_BE_SPECIFIED_IN_TSCONFIG_JSON_FILE_OR_SET_TO_NULL_ON_COMMAND_LINE,
            tsr_core::Span::new(0, 0),
            [option.name.to_string()],
        ));
        // Swallow the value unless it looks like the next option.
        if !value.is_empty() && !value.starts_with('-') {
            index += 1;
        }
        index
    }

    /// `ParseListTypeOption` — a comma-separated list.
    ///
    /// Returns whether the argument was consumed. A value starting with `-` is
    /// the next option rather than this one's list, so it is left in place and
    /// the list is empty — which is how `tsc --lib --strict` reports a missing
    /// argument for `--lib` instead of swallowing `--strict`.
    fn parse_list_option(value: &str) -> (Vec<ConfigValue>, bool) {
        let trimmed = value.trim();
        if trimmed.starts_with('-') {
            return (Vec::new(), false);
        }
        if trimmed.is_empty() {
            return (Vec::new(), true);
        }
        let values: Vec<ConfigValue> = trimmed
            .split(',')
            .map(|entry| ConfigValue::String(entry.trim().to_string()))
            .filter(|entry| entry.as_str().is_some_and(|text| !text.is_empty()))
            .collect();
        (values, true)
    }

    /// Hand the converted value to the option's own setter, and remember that it
    /// was written.
    fn set(&mut self, option: &'static OptionDeclaration, value: &ConfigValue) -> bool {
        // `convertToOptionsWithAbsolutePaths` (`parsinghelpers.go:633`): a
        // file-path option is resolved against the current directory *before* it
        // reaches the setter, so `-p .` becomes the directory itself rather than
        // the empty string `normalize_path(".")` would leave.
        let value = &if option.is_file_path {
            match value {
                ConfigValue::String(text) if !text.is_empty() => {
                    ConfigValue::String(get_normalized_absolute_path(text, self.current_directory))
                }
                ConfigValue::List(entries) => ConfigValue::List(
                    entries
                        .iter()
                        .map(|entry| match entry.as_str() {
                            Some(text) if !text.is_empty() => ConfigValue::String(
                                get_normalized_absolute_path(text, self.current_directory),
                            ),
                            _ => entry.clone(),
                        })
                        .collect(),
                ),
                other => other.clone(),
            }
        } else {
            value.clone()
        };
        let applied = (option.apply)(&mut self.result.compiler_options, value);
        if applied {
            self.result.set_options.push(option.name.to_string());
        }
        applied
    }

    /// `createUnknownOptionError` — with a spelling suggestion when one is close.
    fn unknown_option_error(name: &str, as_written: &str) -> Diagnostic {
        if let Some(suggestion) = tsr_core::get_spelling_suggestion(
            name,
            COMPILER_OPTIONS.iter(),
            |option| option.name,
            |a, b| a.name.cmp(b.name),
        ) {
            return Diagnostic::with_args(
                &messages::UNKNOWN_COMPILER_OPTION_0_DID_YOU_MEAN_1,
                tsr_core::Span::new(0, 0),
                [as_written.to_string(), suggestion.name.to_string()],
            );
        }
        Diagnostic::with_args(
            &messages::UNKNOWN_COMPILER_OPTION_0,
            tsr_core::Span::new(0, 0),
            [as_written.to_string()],
        )
    }
}

/// `getInputOptionName` — strip at most two leading dashes.
///
/// Exactly two. `---target` keeps one and is looked up as `-target`, which finds
/// nothing, which is upstream's behaviour and not an oversight.
fn input_option_name(input: &str) -> &str {
    input.strip_prefix('-').map_or(input, |once| once.strip_prefix('-').unwrap_or(once))
}

/// `getCompilerOptionValueTypeString` — the word used in a type-mismatch error.
fn value_type_string(option: &OptionDeclaration) -> String {
    match option.kind {
        OptionKind::Boolean => "boolean".to_string(),
        OptionKind::Number => "number".to_string(),
        // An enum reads as `string` here. Upstream returns `string(option.Kind)`
        // and its enum kinds are spelled with the *type name* the values map to,
        // which renders as "string" for every option this port declares.
        OptionKind::String | OptionKind::Enum => "string".to_string(),
        OptionKind::List(_) | OptionKind::PathMap => "Array".to_string(),
    }
}

/// `createDiagnosticForInvalidEnumType` — list what the option does accept.
fn invalid_enum_error(option: &OptionDeclaration) -> Diagnostic {
    let names = option.enum_names.join("', '");
    Diagnostic::with_args(
        &messages::ARGUMENT_FOR_0_OPTION_MUST_BE_COLON_1,
        tsr_core::Span::new(0, 0),
        [format!("--{}", option.name), format!("'{names}'")],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsr_core::{ScriptTarget, Tristate};
    use tsr_vfs::InMemoryFileSystem;

    fn empty_fs() -> InMemoryFileSystem {
        InMemoryFileSystem::new([], [], true)
    }

    fn parse(args: &[&str]) -> ParsedCommandLine {
        let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        parse_command_line(&owned, &empty_fs(), "/home/project")
    }

    #[test]
    fn a_bare_name_is_a_file() {
        let parsed = parse(&["a.ts", "b.ts"]);
        assert_eq!(parsed.file_names, ["a.ts", "b.ts"]);
        assert!(parsed.errors.is_empty());
    }

    #[test]
    fn a_boolean_without_a_value_is_true() {
        let parsed = parse(&["--noEmit"]);
        assert_eq!(parsed.compiler_options.no_emit, Tristate::True);
        assert!(parsed.file_names.is_empty());
    }

    #[test]
    fn a_boolean_consumes_only_true_or_false() {
        // `false` is consumed and turns it off.
        let parsed = parse(&["--noEmit", "false", "a.ts"]);
        assert_eq!(parsed.compiler_options.no_emit, Tristate::False);
        assert_eq!(parsed.file_names, ["a.ts"]);

        // A following *flag* is not consumed.
        let parsed = parse(&["--noEmit", "--strict"]);
        assert_eq!(parsed.compiler_options.no_emit, Tristate::True);
        assert_eq!(parsed.compiler_options.strict, Tristate::True);

        // Anything else sets it true and stays a file name — the arm that a
        // "boolean takes an optional value" reading gets wrong.
        let parsed = parse(&["--noEmit", "a.ts"]);
        assert_eq!(parsed.compiler_options.no_emit, Tristate::True);
        assert_eq!(parsed.file_names, ["a.ts"]);
    }

    #[test]
    fn one_or_two_dashes_name_the_same_option() {
        assert_eq!(parse(&["-noEmit"]).compiler_options.no_emit, Tristate::True);
        assert_eq!(parse(&["--noEmit"]).compiler_options.no_emit, Tristate::True);
        // Three dashes leaves one, which names nothing.
        assert!(!parse(&["---noEmit"]).errors.is_empty());
    }

    #[test]
    fn an_option_name_is_case_insensitive_on_the_command_line() {
        // Unlike a config-file key, which is not.
        assert_eq!(parse(&["--NOEMIT"]).compiler_options.no_emit, Tristate::True);
        assert_eq!(parse(&["--nOeMiT"]).compiler_options.no_emit, Tristate::True);
    }

    #[test]
    fn a_short_name_resolves() {
        let parsed = parse(&["-p", "./tsconfig.json"]);
        // Absolute: `project` is a file-path option, so it is resolved against
        // the current directory before the setter sees it. `-p .` depends on
        // this — `normalize_path(".")` alone is the empty string.
        assert_eq!(parsed.compiler_options.project, "/home/project/tsconfig.json");
        assert!(parsed.errors.is_empty());
        // But only as a whole argument.
        assert!(!parse(&["-pr"]).errors.is_empty());
    }

    #[test]
    fn an_enum_takes_its_value() {
        let parsed = parse(&["--target", "es2015"]);
        assert_eq!(parsed.compiler_options.target, ScriptTarget::ES2015);
        assert!(parsed.errors.is_empty());
    }

    #[test]
    fn an_invalid_enum_lists_what_is_accepted() {
        let parsed = parse(&["--target", "es2099"]);
        assert_eq!(parsed.errors.len(), 1);
        let text = parsed.errors[0].text();
        assert!(text.starts_with("Argument for '--target' option must be:"), "{text}");
        assert!(text.contains("'es5'"), "{text}");
    }

    #[test]
    fn a_missing_argument_is_reported() {
        let parsed = parse(&["--target"]);
        // Two: the missing argument, then the list of what an enum accepts.
        assert_eq!(parsed.errors.len(), 2);
        assert_eq!(parsed.errors[0].text(), "Compiler option 'target' expects an argument.");
        assert!(parsed.errors[1].text().starts_with("Argument for '--target' option must be:"));
    }

    #[test]
    fn an_unknown_option_suggests_a_near_one() {
        let parsed = parse(&["--noEmitt"]);
        assert_eq!(parsed.errors.len(), 1);
        let text = parsed.errors[0].text();
        assert!(text.contains("Did you mean"), "{text}");
        assert!(text.contains("noEmit"), "{text}");
    }

    #[test]
    fn an_unknown_option_with_no_near_name_just_says_unknown() {
        let parsed = parse(&["--wibbleflargle"]);
        assert_eq!(parsed.errors.len(), 1);
        let text = parsed.errors[0].text();
        assert!(text.starts_with("Unknown compiler option"), "{text}");
        assert!(!text.contains("Did you mean"), "{text}");
        // Reported as written, dashes included.
        assert!(text.contains("--wibbleflargle"), "{text}");
    }

    #[test]
    fn a_tsconfig_only_option_may_only_be_turned_off() {
        let parsed = parse(&["--composite"]);
        assert_eq!(parsed.errors.len(), 1);
        assert!(parsed.errors[0].text().contains("tsconfig.json"), "{}", parsed.errors[0].text());

        let parsed = parse(&["--composite", "false"]);
        assert!(parsed.errors.is_empty());
        assert_eq!(parsed.compiler_options.composite, Tristate::False);
    }

    #[test]
    fn a_list_splits_on_commas() {
        let parsed = parse(&["--lib", "es5,dom"]);
        assert_eq!(parsed.compiler_options.lib, ["es5", "dom"]);
        assert!(parsed.errors.is_empty());
    }

    #[test]
    fn a_list_does_not_swallow_the_next_option() {
        let parsed = parse(&["--lib", "--strict"]);
        assert_eq!(parsed.compiler_options.strict, Tristate::True);
        assert!(parsed.compiler_options.lib.is_empty());
    }

    #[test]
    fn null_clears_an_option_without_setting_it() {
        let parsed = parse(&["--outDir", "null"]);
        assert!(parsed.errors.is_empty());
        assert_eq!(parsed.compiler_options.out_dir, "");
        // Still recorded as written, so the driver knows not to take the
        // config's value.
        assert!(parsed.was_set("outDir"));
    }

    #[test]
    fn a_response_file_expands_in_place() {
        let fs = InMemoryFileSystem::new(
            [("/home/project/args.txt".to_string(), "--noEmit --target es2015 a.ts".to_string())],
            [],
            true,
        );
        let args = ["@args.txt".to_string(), "b.ts".to_string()];
        let parsed = parse_command_line(&args, &fs, "/home/project");
        assert!(parsed.errors.is_empty());
        assert_eq!(parsed.compiler_options.no_emit, Tristate::True);
        assert_eq!(parsed.compiler_options.target, ScriptTarget::ES2015);
        // Expanded where it stood, so the file order is the response file's
        // first.
        assert_eq!(parsed.file_names, ["a.ts", "b.ts"]);
    }

    #[test]
    fn a_response_file_honours_double_quotes() {
        let fs = InMemoryFileSystem::new(
            [(
                "/home/project/args.txt".to_string(),
                "--outDir \"some dir/out\"\n\"a file.ts\"".to_string(),
            )],
            [],
            true,
        );
        let parsed = parse_command_line(&["@args.txt".to_string()], &fs, "/home/project");
        assert!(parsed.errors.is_empty());
        // Absolute, as every file-path option is.
        assert_eq!(parsed.compiler_options.out_dir, "/home/project/some dir/out");
        assert_eq!(parsed.file_names, ["a file.ts"]);
    }

    #[test]
    fn an_unterminated_quote_in_a_response_file_is_reported() {
        let fs = InMemoryFileSystem::new(
            [("/home/project/args.txt".to_string(), "--outDir \"unclosed".to_string())],
            [],
            true,
        );
        let parsed = parse_command_line(&["@args.txt".to_string()], &fs, "/home/project");
        // Two, and the second is a consequence of the first: the unterminated
        // run never becomes an argument, so `--outDir` is left without one.
        assert_eq!(parsed.errors.len(), 2);
        assert!(
            parsed.errors[0].text().contains("Unterminated quoted string"),
            "{}",
            parsed.errors[0].text()
        );
        assert_eq!(parsed.errors[1].text(), "Compiler option 'outDir' expects an argument.");
    }

    #[test]
    fn a_missing_response_file_is_reported() {
        let parsed = parse(&["@nope.txt"]);
        assert_eq!(parsed.errors.len(), 1);
        assert!(parsed.errors[0].text().starts_with("Cannot read file"));
    }

    #[test]
    fn a_self_referential_response_file_terminates() {
        // No upstream counterpart — upstream overflows its stack. See
        // `Parser::response_file_depth`.
        let fs = InMemoryFileSystem::new(
            [("/home/project/loop.txt".to_string(), "@loop.txt".to_string())],
            [],
            true,
        );
        let parsed = parse_command_line(&["@loop.txt".to_string()], &fs, "/home/project");
        assert_eq!(parsed.errors.len(), 1);
        assert!(parsed.errors[0].text().starts_with("Cannot read file"));
    }

    #[test]
    fn an_empty_argument_is_skipped() {
        let parsed = parse(&["", "a.ts"]);
        assert_eq!(parsed.file_names, ["a.ts"]);
        assert!(parsed.errors.is_empty());
    }
}
