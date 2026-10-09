//! `package.json`, as module resolution reads it.
//!
//! Ported from `internal/packagejson` at the pinned commit.
//!
//! # Why fields are "expected" rather than typed
//!
//! Every field here is attacker-shaped: a `package.json` on disk can say
//! `"main": 42` or `"typings": null`, and the compiler must keep resolving. So a
//! field records three things rather than one — whether it was *present*, whether
//! it had the *expected type*, and what the value was — because the trace
//! distinguishes all three:
//!
//! ```text
//! 'package.json' does not have a 'typings' field.
//! Expected type of 'main' field in 'package.json' to be 'string', got 'number'.
//! 'package.json' had a falsy 'main' field.
//! ```
//!
//! `"typings": null` in particular is load-bearing: `types-publisher` writes it
//! for packages that ship no types, and
//! [`crate::resolver::get_automatic_type_directive_names`] skips those.

use std::cell::OnceCell;

use tsr_core::OrderedMap;

use crate::{
    json::Json,
    messages::{self, Trace},
    semver::{Version, VersionRange},
};

/// The compiler version `typesVersions` ranges are tested against
/// (`core.Version()`).
///
/// Baselines print `FakeTSVersion` wherever this appears, so its *text* never
/// reaches a baseline — but its *value* decides which `typesVersions` entry wins,
/// and that does.
pub const TYPESCRIPT_VERSION: &str = "7.1.0-dev";

/// The `major.minor` part, for the "no matching entry" trace line
/// (`core.VersionMajorMinor()`).
pub const TYPESCRIPT_VERSION_MAJOR_MINOR: &str = "7.1";

/// A field that may be absent, present-but-wrong-typed, or valid
/// (`packagejson.Expected[T]`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expected<T> {
    /// The JSON type actually found, or `None` if the field was absent.
    actual_json_type: Option<&'static str>,
    /// Whether the field was explicitly `null`.
    pub null: bool,
    /// The value, if it had the expected type.
    pub value: Option<T>,
}

impl<T> Expected<T> {
    /// Whether the field appeared at all.
    #[must_use]
    pub fn is_present(&self) -> bool {
        self.actual_json_type.is_some()
    }

    /// Whether the field had the expected type.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.value.is_some()
    }

    /// The JSON type found, for the "expected type of" trace line.
    #[must_use]
    pub fn actual_json_type(&self) -> &'static str {
        self.actual_json_type.unwrap_or("")
    }
}

impl Expected<String> {
    /// The expected type name, for the trace line.
    #[must_use]
    pub const fn expected_json_type() -> &'static str {
        "string"
    }

    fn from_json(value: Option<&Json>) -> Self {
        let Some(value) = value else { return Self::default() };
        Self {
            actual_json_type: Some(value.type_name()),
            null: matches!(value, Json::Null),
            value: value.as_str().map(str::to_string),
        }
    }
}

/// A dependency map field: `{ "name": "range" }`.
pub type DependencyMap = Vec<(String, String)>;

impl Expected<DependencyMap> {
    /// The expected type name, for the trace line.
    #[must_use]
    pub const fn expected_json_type() -> &'static str {
        "object"
    }

    fn from_json(value: Option<&Json>) -> Self {
        let Some(value) = value else { return Self::default() };
        let map = match value {
            Json::Object(entries) => Some(
                entries
                    .iter()
                    .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string())))
                    .collect(),
            ),
            _ => None,
        };
        Self {
            actual_json_type: Some(value.type_name()),
            null: matches!(value, Json::Null),
            value: map,
        }
    }
}

/// What an `exports`/`imports` object's keys mean
/// (`packagejson.objectKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    /// Keys are subpaths (`"."`, `"./sub"`).
    Subpaths,
    /// Keys are conditions (`"import"`, `"types"`).
    Conditions,
    /// Keys are `#`-prefixed import specifiers.
    Imports,
    /// Keys mix the two, which is meaningless.
    Invalid,
}

/// The `exports` or `imports` field (`packagejson.ExportsOrImports`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportsOrImports {
    /// The raw value, or `None` if the field was absent.
    pub value: Option<Json>,
}

impl ExportsOrImports {
    fn from_json(value: Option<&Json>) -> Self {
        Self { value: value.cloned() }
    }

    /// Whether the field appeared.
    #[must_use]
    pub fn is_present(&self) -> bool {
        self.value.is_some()
    }

    /// Whether the field is absent, `null`, `""`, `0`, or `false`
    /// (`JSONValue.IsFalsy`).
    ///
    /// Upstream comments that this falsy check "seems wrong" — a `false` here is
    /// treated as "no exports" rather than "block everything". Ported as written:
    /// changing it would change resolution for packages that do this.
    #[must_use]
    pub fn is_falsy(&self) -> bool {
        match &self.value {
            None | Some(Json::Null) => true,
            Some(Json::String(text)) => text.is_empty(),
            Some(Json::Number(text)) => text == "0",
            Some(Json::Bool(value)) => !value,
            _ => false,
        }
    }

    /// The entries, if this is an object.
    #[must_use]
    pub fn as_object(&self) -> Option<&[(String, Json)]> {
        match &self.value {
            Some(Json::Object(entries)) => Some(entries),
            _ => None,
        }
    }

    /// What kind of object this is (`ExportsOrImports.initObjectKind`).
    ///
    /// `None` when the field is not an object at all.
    #[must_use]
    pub fn object_kind(&self) -> Option<ObjectKind> {
        let entries = self.as_object()?;
        if entries.is_empty() {
            return Some(ObjectKind::Conditions);
        }
        let (mut dot, mut hash, mut other) = (false, false, false);
        for (key, _) in entries {
            if let Some(first) = key.as_bytes().first() {
                dot = dot || *first == b'.';
                hash = hash || *first == b'#';
                other = other || (*first != b'.' && *first != b'#');
                if other && (dot || hash) {
                    return Some(ObjectKind::Invalid);
                }
            }
        }
        Some(if dot {
            ObjectKind::Subpaths
        } else if hash {
            ObjectKind::Imports
        } else {
            ObjectKind::Conditions
        })
    }

    /// Whether this is a subpath map (`ExportsOrImports.IsSubpaths`).
    #[must_use]
    pub fn is_subpaths(&self) -> bool {
        self.object_kind() == Some(ObjectKind::Subpaths)
    }

    /// Whether this is a condition map (`ExportsOrImports.IsConditions`).
    #[must_use]
    pub fn is_conditions(&self) -> bool {
        self.object_kind() == Some(ObjectKind::Conditions)
    }
}

/// The `typesVersions` entry that matched, and its `paths` table
/// (`packagejson.VersionPaths`).
#[derive(Debug, Clone, Default)]
pub struct VersionPaths {
    /// The matching key, e.g. `>=3.1.0-0`. Empty when nothing matched.
    pub version: String,
    /// The mapping, in declaration order.
    pub paths: OrderedMap<Vec<String>>,
}

impl VersionPaths {
    /// Whether a `typesVersions` entry matched.
    #[must_use]
    pub fn exists(&self) -> bool {
        !self.version.is_empty()
    }
}

/// A parsed `package.json` (`packagejson.PackageJson`).
#[derive(Debug)]
pub struct PackageJson {
    /// `name`.
    pub name: Expected<String>,
    /// `version`.
    pub version: Expected<String>,
    /// `type`: `"module"` or `"commonjs"`.
    pub package_type: Expected<String>,
    /// `tsconfig`, for config resolution.
    pub tsconfig: Expected<String>,
    /// `main`.
    pub main: Expected<String>,
    /// `types`.
    pub types: Expected<String>,
    /// `typings`.
    pub typings: Expected<String>,
    /// `typesVersions`, unparsed.
    pub types_versions: Option<Json>,
    /// `imports`.
    pub imports: ExportsOrImports,
    /// `exports`.
    pub exports: ExportsOrImports,
    /// `peerDependencies`.
    pub peer_dependencies: Expected<DependencyMap>,
    /// `dependencies`.
    pub dependencies: Expected<DependencyMap>,
    /// `optionalDependencies`.
    pub optional_dependencies: Expected<DependencyMap>,
    /// Whether the file parsed as JSON at all.
    pub parseable: bool,
    /// Memoised `typesVersions` resolution and the traces it produced.
    version_paths: OnceCell<(VersionPaths, Vec<Trace>)>,
}

impl PackageJson {
    /// `DependencyFields.GetRuntimeDependencyNames`
    /// (`packagejson/packagejson.go:89`): the names in `dependencies`,
    /// `peerDependencies` and `optionalDependencies`, each once, in that
    /// order (upstream's is a set; its one consumer is order-insensitive).
    #[must_use]
    pub fn runtime_dependency_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::new();
        for field in [&self.dependencies, &self.peer_dependencies, &self.optional_dependencies] {
            for (name, _) in field.value.iter().flatten() {
                if !names.contains(&name.as_str()) {
                    names.push(name);
                }
            }
        }
        names
    }

    /// Parse a `package.json`'s text (`packagejson.Parse`).
    ///
    /// Unparseable input yields an all-absent record with `parseable` false,
    /// because "there is a broken package.json here" is a different resolution
    /// state from "there is none".
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let root = crate::json::parse(text);
        let get = |key: &str| root.as_ref().and_then(|value| value.get(key));
        Self {
            name: Expected::<String>::from_json(get("name")),
            version: Expected::<String>::from_json(get("version")),
            package_type: Expected::<String>::from_json(get("type")),
            tsconfig: Expected::<String>::from_json(get("tsconfig")),
            main: Expected::<String>::from_json(get("main")),
            types: Expected::<String>::from_json(get("types")),
            typings: Expected::<String>::from_json(get("typings")),
            types_versions: get("typesVersions").cloned(),
            imports: ExportsOrImports::from_json(get("imports")),
            exports: ExportsOrImports::from_json(get("exports")),
            peer_dependencies: Expected::<DependencyMap>::from_json(get("peerDependencies")),
            dependencies: Expected::<DependencyMap>::from_json(get("dependencies")),
            optional_dependencies: Expected::<DependencyMap>::from_json(get(
                "optionalDependencies",
            )),
            parseable: root.is_some(),
            version_paths: OnceCell::new(),
        }
    }

    /// The `typesVersions` entry matching this compiler, and the trace lines
    /// explaining the choice (`PackageJson.GetVersionPaths`).
    ///
    /// The traces are computed once and **replayed on every call**, which is
    /// upstream's behaviour and is visible in the baselines: a package consulted
    /// twice logs its `typesVersions` lines twice.
    pub fn get_version_paths(&self) -> &(VersionPaths, Vec<Trace>) {
        self.version_paths.get_or_init(|| self.compute_version_paths())
    }

    fn compute_version_paths(&self) -> (VersionPaths, Vec<Trace>) {
        let mut traces = Vec::new();
        let mut trace = |message: &messages::Message, args: &[&str]| {
            traces.push(Trace { code: message.code, text: message.format(args) });
        };

        let Some(types_versions) = &self.types_versions else {
            trace(&messages::X_PACKAGE_JSON_DOES_NOT_HAVE_A_0_FIELD, &["typesVersions"]);
            return (VersionPaths::default(), traces);
        };
        let Json::Object(entries) = types_versions else {
            trace(
                &messages::EXPECTED_TYPE_OF_0_FIELD_IN_PACKAGE_JSON_TO_BE_1_GOT_2,
                &["typesVersions", "object", types_versions.type_name()],
            );
            return (VersionPaths::default(), traces);
        };

        trace(
            &messages::X_PACKAGE_JSON_HAS_A_TYPESVERSIONS_FIELD_WITH_VERSION_SPECIFIC_PATH_MAPPINGS,
            &[],
        );

        let compiler_version = Version::must_parse(TYPESCRIPT_VERSION);
        for (key, value) in entries {
            let Some(range) = VersionRange::parse(key) else {
                trace(
                    &messages::X_PACKAGE_JSON_HAS_A_TYPESVERSIONS_ENTRY_0_THAT_IS_NOT_A_VALID_SEMVER_RANGE,
                    &[key],
                );
                continue;
            };
            if !range.test(&compiler_version) {
                continue;
            }
            let Json::Object(paths) = value else {
                trace(
                    &messages::EXPECTED_TYPE_OF_0_FIELD_IN_PACKAGE_JSON_TO_BE_1_GOT_2,
                    &[&format!("typesVersions['{key}']"), "object", value.type_name()],
                );
                // Upstream returns here rather than continuing: a matching entry
                // with the wrong shape ends the search, it does not fall through
                // to a later entry.
                return (VersionPaths::default(), traces);
            };
            let mut mapping = OrderedMap::new();
            for (pattern, substitutions) in paths {
                let Json::Array(elements) = substitutions else { continue };
                // A non-string element becomes an empty substitution rather than
                // being dropped: upstream sizes the slice first and leaves the
                // slot zeroed, so the *positions* of the others are preserved.
                mapping.set(
                    pattern.clone(),
                    elements
                        .iter()
                        .map(|element| element.as_str().unwrap_or("").to_string())
                        .collect::<Vec<_>>(),
                );
            }
            return (VersionPaths { version: key.clone(), paths: mapping }, traces);
        }

        trace(
            &messages::X_PACKAGE_JSON_DOES_NOT_HAVE_A_TYPESVERSIONS_ENTRY_THAT_MATCHES_VERSION_0,
            &[TYPESCRIPT_VERSION_MAJOR_MINOR],
        );
        (VersionPaths::default(), traces)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_distinguishes_absent_from_wrong_typed_from_valid() {
        let package = PackageJson::parse(r#"{"main":42,"types":"./i.d.ts"}"#);
        assert!(!package.typings.is_present(), "absent");
        assert!(package.main.is_present() && !package.main.is_valid(), "present, wrong type");
        assert_eq!(package.main.actual_json_type(), "number");
        assert_eq!(package.types.value.as_deref(), Some("./i.d.ts"));
    }

    #[test]
    fn an_explicit_null_typings_is_recorded_as_null_not_as_absent() {
        // `types-publisher` writes this for packages with no types, and
        // automatic type directive discovery skips them because of it.
        let package = PackageJson::parse(r#"{"typings":null}"#);
        assert!(package.typings.is_present());
        assert!(package.typings.null);
        assert!(!package.typings.is_valid());
    }

    #[test]
    fn unparseable_input_is_a_present_but_empty_package() {
        let package = PackageJson::parse("{ not json");
        assert!(!package.parseable);
        assert!(!package.name.is_present());
    }

    #[test]
    fn exports_object_kinds_are_told_apart_by_their_first_character() {
        assert!(PackageJson::parse(r#"{"exports":{".":"./i.js"}}"#).exports.is_subpaths());
        assert!(PackageJson::parse(r#"{"exports":{"import":"./i.js"}}"#).exports.is_conditions());
        assert_eq!(
            PackageJson::parse(r##"{"imports":{"#a":"./a.js"}}"##).imports.object_kind(),
            Some(ObjectKind::Imports)
        );
        // Mixing them is meaningless and upstream says so explicitly.
        assert_eq!(
            PackageJson::parse(r#"{"exports":{".":"./i.js","import":"./j.js"}}"#)
                .exports
                .object_kind(),
            Some(ObjectKind::Invalid)
        );
        // An empty object reads as conditions, not subpaths.
        assert_eq!(
            PackageJson::parse(r#"{"exports":{}}"#).exports.object_kind(),
            Some(ObjectKind::Conditions)
        );
    }

    #[test]
    fn a_matching_types_versions_entry_wins_and_traces_the_choice() {
        let package = PackageJson::parse(
            r#"{"typesVersions":{"<4":{"*":["old/*"]},">=3.1.0-0":{"*":["ts3.1/*"]}}}"#,
        );
        let (paths, traces) = package.get_version_paths();
        assert_eq!(paths.version, ">=3.1.0-0");
        assert_eq!(
            paths.paths.get("*").map(Vec::as_slice),
            Some(["ts3.1/*".to_string()].as_slice())
        );
        // The non-matching `<4` produced no line; only the header did.
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].code, 6206);
    }

    #[test]
    fn an_absent_or_unmatched_types_versions_traces_why() {
        let absent = PackageJson::parse("{}");
        assert_eq!(absent.get_version_paths().1[0].code, 6100);

        let unmatched = PackageJson::parse(r#"{"typesVersions":{"<4":{"*":["old/*"]}}}"#);
        let (paths, traces) = unmatched.get_version_paths();
        assert!(!paths.exists());
        assert_eq!(traces.last().map(|t| t.code), Some(6207));

        let invalid = PackageJson::parse(r#"{"typesVersions":{"1.":{"*":["old/*"]}}}"#);
        assert!(invalid.get_version_paths().1.iter().any(|t| t.code == 6209));
    }

    #[test]
    fn version_path_traces_replay_on_every_call() {
        // Upstream memoises the *computation* and replays the *traces*, so a
        // package consulted twice logs twice. Dropping the second set would
        // silently shorten every trace that revisits a package.
        let package = PackageJson::parse("{}");
        assert_eq!(package.get_version_paths().1.len(), 1);
        assert_eq!(package.get_version_paths().1.len(), 1);
    }
}
