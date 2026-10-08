//! Expanding a case into the configurations upstream's runner compiles it under.
//!
//! Ported from `GetFileBasedTestConfigurations`,
//! `getFileBasedTestConfigurationDescription`, `splitOptionValues`,
//! `tryGetValueOfOptionString`, `getAllValuesForOption` and
//! `computeFileBasedTestConfigurationVariations`
//! (`internal/testutil/harnessutil/harnessutil.go:995`–`:1195`), the vary-by set
//! `compilerVaryBy` (`internal/testrunner/compiler_runner.go:161`), and the
//! settings extraction `extractCompilerSettings`
//! (`internal/testrunner/test_case_parser.go:274`), all at the pinned commit.
//!
//! # What upstream does
//!
//! A directive whose option is in the vary-by set and whose value lists more
//! than one distinct value — `// @target: es5, es2015`, `// @strict: *` — makes
//! the runner compile the case once per combination. Each compilation is named
//! by its varying options, sorted by name, as `name=value` with the value
//! lowercased and joined by commas (`target=es2015,jsx=…` sorts to
//! `jsx=…,target=es2015`), and writes its baselines as
//! `case(<name>).errors.txt`, `case(<name>).types`, …
//! (`newCompilerTest`, `compiler_runner.go:254`). A case with no varying
//! directive is compiled once, under the unnamed configuration, and keeps its
//! plain baseline names.
//!
//! Three details decide which compilations exist, and all three are mirrored:
//!
//! - **Values are deduplicated by meaning, not spelling**: `es6, es2015` is one
//!   compilation, named by whichever spelling came first, because the
//!   variations are keyed by `tryGetValueOfOptionString`'s parsed value.
//! - **`*` means every value of the option** — `true`/`false`, or every key of
//!   the option's enum map in the map's order — and `-v`/`!v` excludes one.
//! - **A single-valued vary-by directive is normalised too**: `@declaration:
//!   true,` and `@declaration: true;` compile as `true` (`nonVaryingOptions`
//!   takes `entries[0]`, and `extractCompilerSettings` strips one `;`).
//!
//! An unknown value in an included entry is `t.Fatalf` upstream: the test
//! fails before compiling and writes no baseline. That is
//! [`Configurations::Rejected`].
//!
//! # Why the vary-by set is a table here
//!
//! Upstream computes it from `tsoptions.OptionsDeclarations`: every boolean or
//! enum option that is not command-line-only and has any `Affects*` flag, plus
//! `noEmit` and `isolatedModules`. `tsr_tsoptions` declares only the options
//! `CompilerOptions` has a field for and carries no `Affects*` flags, so the set
//! cannot be derived on this side. The table below is that computation's
//! output at the pinned commit (72 names). ADR-0047 records how it was taken
//! and how the whole expansion was checked against native's own over the
//! corpus.

use std::collections::BTreeMap;

/// The lowercased names of the options a test directive may vary
/// (`compilerVaryBy`), sorted.
pub const VARY_BY: &[&str] = &[
    "allowarbitraryextensions",
    "allowimportingtsextensions",
    "allowjs",
    "allowsyntheticdefaultimports",
    "allowumdglobalaccess",
    "allowunreachablecode",
    "allowunusedlabels",
    "alwaysstrict",
    "assumechangesonlyaffectdirectdependencies",
    "checkjs",
    "composite",
    "declaration",
    "declarationmap",
    "deduplicatepackages",
    "disablesizelimit",
    "downleveliteration",
    "emitbom",
    "emitdeclarationonly",
    "emitdecoratormetadata",
    "erasablesyntaxonly",
    "esmoduleinterop",
    "exactoptionalpropertytypes",
    "experimentaldecorators",
    "forceconsistentcasinginfilenames",
    "importhelpers",
    "inlinesourcemap",
    "inlinesources",
    "isolateddeclarations",
    "isolatedmodules",
    "jsx",
    "libreplacement",
    "module",
    "moduledetection",
    "moduleresolution",
    "newline",
    "noemit",
    "noemithelpers",
    "noemitonerror",
    "noerrortruncation",
    "nofallthroughcasesinswitch",
    "noimplicitany",
    "noimplicitoverride",
    "noimplicitreturns",
    "noimplicitthis",
    "nolib",
    "nopropertyaccessfromindexsignature",
    "noresolve",
    "nouncheckedindexedaccess",
    "nouncheckedsideeffectimports",
    "nounusedlocals",
    "nounusedparameters",
    "preserveconstenums",
    "removecomments",
    "resolvejsonmodule",
    "resolvepackagejsonexports",
    "resolvepackagejsonimports",
    "rewriterelativeimportextensions",
    "skipdefaultlibcheck",
    "skiplibcheck",
    "sourcemap",
    "stabletypeordering",
    "strict",
    "strictbindcallapply",
    "strictbuiltiniteratorreturn",
    "strictfunctiontypes",
    "strictnullchecks",
    "strictpropertyinitialization",
    "stripinternal",
    "target",
    "usedefineforclassfields",
    "useunknownincatchvariables",
    "verbatimmodulesyntax",
];

/// The enum maps of the enum-kind vary-by options, in upstream's order
/// (`internal/tsoptions/enummaps.go`): `(key, value)`, where entries sharing a
/// value are spellings of one setting. The value is only an identity here.
const ENUM_MAPS: &[(&str, &[(&str, &str)])] = &[
    (
        "jsx",
        &[
            ("preserve", "preserve"),
            ("react-native", "react-native"),
            ("react-jsx", "react-jsx"),
            ("react-jsxdev", "react-jsxdev"),
            ("react", "react"),
        ],
    ),
    (
        "module",
        &[
            ("commonjs", "CommonJS"),
            ("amd", "AMD"),
            ("system", "System"),
            ("umd", "UMD"),
            ("es6", "ES2015"),
            ("es2015", "ES2015"),
            ("es2020", "ES2020"),
            ("es2022", "ES2022"),
            ("esnext", "ESNext"),
            ("node16", "Node16"),
            ("node18", "Node18"),
            ("node20", "Node20"),
            ("nodenext", "NodeNext"),
            ("preserve", "Preserve"),
        ],
    ),
    ("moduledetection", &[("auto", "auto"), ("legacy", "legacy"), ("force", "force")]),
    (
        "moduleresolution",
        &[
            ("node16", "Node16"),
            ("nodenext", "NodeNext"),
            ("bundler", "Bundler"),
            ("classic", "Classic"),
            ("node", "Node10"),
            ("node10", "Node10"),
        ],
    ),
    ("newline", &[("crlf", "crlf"), ("lf", "lf")]),
    (
        "target",
        &[
            ("es5", "ES5"),
            ("es6", "ES2015"),
            ("es2015", "ES2015"),
            ("es2016", "ES2016"),
            ("es2017", "ES2017"),
            ("es2018", "ES2018"),
            ("es2019", "ES2019"),
            ("es2020", "ES2020"),
            ("es2021", "ES2021"),
            ("es2022", "ES2022"),
            ("es2023", "ES2023"),
            ("es2024", "ES2024"),
            ("es2025", "ES2025"),
            ("esnext", "ESNext"),
        ],
    ),
];

/// Upstream's cap on the product of the variation counts.
const MAX_VARIATIONS: usize = 25;

/// One compilation of a case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Configuration {
    /// `target=es2015,jsx=preserve`-style description, or empty for the single
    /// unnamed configuration of a case that varies nothing.
    pub name: String,
    /// Every setting this compilation runs with, names lowercased: the varying
    /// options' chosen values plus every other setting
    /// (`maps.Copy(varyingConfig, nonVaryingOptions)`).
    pub values: BTreeMap<String, String>,
}

/// What the runner does with a case's directives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Configurations {
    /// No directive at all: compiled once with no configuration.
    None,
    /// Nothing varies: compiled once, under plain baseline names, with these
    /// (normalised) settings.
    Single(Configuration),
    /// One compilation per named configuration, sorted by name. Upstream
    /// writes no plain baseline for such a case.
    Varied(Vec<Configuration>),
    /// Upstream's test fails before compiling (`t.Fatal` or `panic`), so no
    /// baseline exists for any configuration.
    Rejected(String),
}

/// The settings `extractCompilerSettings` reads from a case: the case parser's
/// options with one trailing `;` removed from each value.
///
/// [`crate::TestCase::options`] already lowercases names, keeps the last
/// occurrence and trims values, as upstream's settings map does.
#[must_use]
pub fn settings(options: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    options
        .iter()
        .map(|(name, value)| (name.clone(), value.strip_suffix(';').unwrap_or(value).to_string()))
        .collect()
}

/// `GetFileBasedTestConfigurations` over `compilerVaryBy`.
#[must_use]
pub fn file_based_test_configurations(settings: &BTreeMap<String, String>) -> Configurations {
    let mut option_entries: Vec<(&str, Vec<String>)> = Vec::new();
    let mut variation_count = 1usize;
    let mut non_varying: BTreeMap<String, String> = BTreeMap::new();
    for (option, value) in settings {
        if !VARY_BY.contains(&option.as_str()) {
            non_varying.insert(option.clone(), value.clone());
            continue;
        }
        let entries = match split_option_values(value, option) {
            Ok(entries) => entries,
            Err(reason) => return Configurations::Rejected(reason),
        };
        match entries.len() {
            0 => {}
            1 => {
                non_varying.insert(option.clone(), entries[0].clone());
            }
            count => {
                variation_count *= count;
                if variation_count > MAX_VARIATIONS {
                    return Configurations::Rejected(
                        "Provided test options exceeded the maximum number of variations".into(),
                    );
                }
                option_entries.push((option, entries));
            }
        }
    }

    if option_entries.is_empty() {
        if non_varying.is_empty() {
            return Configurations::None;
        }
        return Configurations::Single(Configuration { name: String::new(), values: non_varying });
    }

    let mut configurations = Vec::with_capacity(variation_count);
    let mut state = BTreeMap::new();
    variations(&option_entries, 0, &mut state, &mut configurations);
    let mut configurations: Vec<Configuration> = configurations
        .into_iter()
        .map(|varying| {
            let name = description(&varying);
            let mut values = varying;
            values.extend(non_varying.iter().map(|(k, v)| (k.clone(), v.clone())));
            Configuration { name, values }
        })
        .collect();
    // Upstream's order is Go map iteration order, i.e. unspecified; nothing
    // depends on it but the order rows are produced in, so sort by name.
    configurations.sort_by(|a, b| a.name.cmp(&b.name));
    Configurations::Varied(configurations)
}

/// `computeFileBasedTestConfigurationVariationsWorker`.
fn variations(
    option_entries: &[(&str, Vec<String>)],
    index: usize,
    state: &mut BTreeMap<String, String>,
    out: &mut Vec<BTreeMap<String, String>>,
) {
    let Some((option, entries)) = option_entries.get(index) else {
        out.push(state.clone());
        return;
    };
    for entry in entries {
        state.insert((*option).to_string(), entry.clone());
        variations(option_entries, index + 1, state, out);
    }
}

/// `getFileBasedTestConfigurationDescription`: sorted keys, lowercased values.
fn description(config: &BTreeMap<String, String>) -> String {
    config
        .iter()
        .map(|(key, value)| format!("{key}={}", value.to_lowercase()))
        .collect::<Vec<_>>()
        .join(",")
}

/// `splitOptionValues`: the distinct settings a directive value names, each
/// spelled as first written. `Err` is upstream's `t.Fatalf` / `panic`.
fn split_option_values(value: &str, option: &str) -> Result<Vec<String>, String> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    let mut star = false;
    let mut includes = Vec::new();
    let mut excludes = Vec::new();
    for part in value.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if part == "*" {
            star = true;
        } else if let Some(excluded) = part.strip_prefix('-').or_else(|| part.strip_prefix('!')) {
            excludes.push(excluded);
        } else {
            includes.push(part);
        }
    }
    if includes.is_empty() && !star && excludes.is_empty() {
        return Ok(Vec::new());
    }

    // `variations` is a Go map keyed by the parsed value; insertion order is
    // kept here only so the result is deterministic.
    let mut variations: Vec<(String, String)> = Vec::new();
    let mut add = |identity: String, spelling: &str| {
        if !variations.iter().any(|(seen, _)| *seen == identity) {
            variations.push((identity, spelling.to_string()));
        }
    };
    for include in &includes {
        let Some(identity) = value_of_option_string(option, include) else {
            return Err(format!("Unknown value '{include}' for option '{option}'"));
        };
        add(identity, include);
    }
    if star {
        for include in all_values_for_option(option) {
            if let Some(identity) = value_of_option_string(option, include) {
                add(identity, include);
            }
        }
    }
    for exclude in excludes {
        // An unrecognised exclusion has nothing to remove.
        if let Some(identity) = value_of_option_string(option, exclude) {
            variations.retain(|(seen, _)| *seen != identity);
        }
    }
    if variations.is_empty() {
        return Err(format!("Variations in test option '@{option}' resulted in an empty set."));
    }
    Ok(variations.into_iter().map(|(_, spelling)| spelling).collect())
}

/// `tryGetValueOfOptionString`, reduced to an identity: the enum value, the
/// boolean, or the string itself for any other kind.
fn value_of_option_string(option: &str, value: &str) -> Option<String> {
    if let Some((_, map)) = ENUM_MAPS.iter().find(|(name, _)| *name == option) {
        let lower = value.to_lowercase();
        return map.iter().find(|(key, _)| *key == lower).map(|(_, value)| (*value).to_string());
    }
    // Every other vary-by option is boolean.
    match value.to_lowercase().as_str() {
        "true" => Some("true".into()),
        "false" => Some("false".into()),
        _ => None,
    }
}

/// `getAllValuesForOption`.
fn all_values_for_option(option: &str) -> Vec<&'static str> {
    if let Some((_, map)) = ENUM_MAPS.iter().find(|(name, _)| *name == option) {
        return map.iter().map(|(key, _)| *key).collect();
    }
    vec!["true", "false"]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configurations(directives: &[(&str, &str)]) -> Configurations {
        let options: BTreeMap<String, String> =
            directives.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
        file_based_test_configurations(&settings(&options))
    }

    fn names(directives: &[(&str, &str)]) -> Vec<String> {
        match configurations(directives) {
            Configurations::Varied(list) => list.into_iter().map(|c| c.name).collect(),
            other => panic!("expected variations, got {other:?}"),
        }
    }

    #[test]
    fn the_vary_by_table_is_sorted_and_has_upstreams_size() {
        assert_eq!(VARY_BY.len(), 72);
        assert!(VARY_BY.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn a_multi_valued_target_varies_and_values_are_lowercased_in_the_name() {
        assert_eq!(
            names(&[("target", "ES5, ES2015"), ("strict", "true")]),
            ["target=es2015", "target=es5"]
        );
    }

    #[test]
    fn spellings_of_one_value_are_one_configuration_named_by_the_first() {
        assert_eq!(names(&[("target", "es6, es2015, esnext")]), ["target=es6", "target=esnext"]);
    }

    #[test]
    fn several_varying_options_are_named_in_sorted_key_order() {
        assert_eq!(
            names(&[("target", "es2015,esnext"), ("jsx", "react,preserve")]),
            [
                "jsx=preserve,target=es2015",
                "jsx=preserve,target=esnext",
                "jsx=react,target=es2015",
                "jsx=react,target=esnext",
            ]
        );
    }

    #[test]
    fn star_expands_and_exclusions_remove() {
        assert_eq!(names(&[("strict", "*")]), ["strict=false", "strict=true"]);
        assert_eq!(
            configurations(&[("strict", "*, -true")]),
            Configurations::Single(Configuration {
                name: String::new(),
                values: [("strict".to_string(), "false".to_string())].into(),
            })
        );
    }

    #[test]
    fn options_outside_the_vary_by_set_never_vary() {
        // `lib` is a list option, so its commas are a list, not variations.
        let result = configurations(&[("lib", "es2015,dom")]);
        assert_eq!(
            result,
            Configurations::Single(Configuration {
                name: String::new(),
                values: [("lib".to_string(), "es2015,dom".to_string())].into(),
            })
        );
    }

    #[test]
    fn a_single_valued_vary_by_directive_is_normalised() {
        let Configurations::Single(config) = configurations(&[("declaration", "true;")]) else {
            panic!("one configuration");
        };
        assert_eq!(config.values["declaration"], "true");
        let Configurations::Single(config) = configurations(&[("declaration", "true,")]) else {
            panic!("one configuration");
        };
        assert_eq!(config.values["declaration"], "true");
    }

    #[test]
    fn varied_configurations_carry_every_other_setting() {
        let Configurations::Varied(list) =
            configurations(&[("target", "es2015,esnext"), ("lib", "dom")])
        else {
            panic!("varied");
        };
        assert_eq!(list[0].values["lib"], "dom");
        assert_eq!(list[0].values["target"], "es2015");
    }

    #[test]
    fn an_unknown_value_rejects_the_case_as_upstream_fails_it() {
        assert!(matches!(configurations(&[("module", "none")]), Configurations::Rejected(_)));
        assert!(matches!(
            configurations(&[
                ("target", "es2015,es2016,es2017,es2018,es2019,es2020"),
                ("jsx", "react,preserve,react-jsx,react-jsxdev,react-native")
            ]),
            Configurations::Rejected(_)
        ));
    }

    #[test]
    fn no_directive_is_no_configuration() {
        assert_eq!(configurations(&[]), Configurations::None);
    }
}
