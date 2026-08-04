//! Semantic versions and npm-style ranges.
//!
//! Ported from `internal/semver/version.go` and
//! `internal/semver/version_range.go` at the pinned commit.
//!
//! # Why module resolution needs this at all
//!
//! Two `package.json` features are version-gated, and both are load-bearing in
//! the trace baselines:
//!
//! - `typesVersions`, whose keys are ranges (`">=3.1.0-0"`), selecting a whole
//!   `paths` table by which TypeScript is compiling.
//! - `exports` conditions spelled `types@<range>`, which
//!   [`crate::util::is_applicable_versioned_types_key`] tests.
//!
//! Getting the range semantics wrong does not fail loudly: it silently picks the
//! wrong `paths` table and resolves to the wrong file.
//!
//! # Why it is hand-rolled rather than a crate
//!
//! Upstream's ranges are **not** node-semver's, and not the `semver` crate's
//! either. Two deliberate deviations, both of which the corpus exercises:
//!
//! 1. **`X` and `X.Y` are versions**, with the missing parts defaulting to `0`.
//!    Strict semver rejects both. Corpus `typesVersions` keys are overwhelmingly
//!    of this shape (`"4.0"`, `"1"`, `"3.0"`).
//! 2. **A partial range's wildcard rewrites the prerelease bound**: `>=4` becomes
//!    `>=4.0.0-0`, not `>=4.0.0`, so a prerelease of 4.0.0 satisfies it. That is
//!    why `>=3.1.0-0` is written explicitly wherever the corpus wants the same
//!    effect for a fully-specified version.
//!
//! Both would have to be layered on top of any crate, at which point the crate is
//! doing the easy part. This also matches [ADR-0004](../../../docs/adr/0004-oxc-inspiration-not-dependency.md):
//! the semantics we must match are upstream's, so upstream is the source.
//!
//! The regular expressions are transcribed as hand-written scanners, following
//! `tsr-conformance`'s `case.rs`: the patterns are fixed, and stating the rules
//! in code beats depending on a regex engine to state them.

use std::cmp::Ordering;

/// A parsed semantic version (`semver.Version`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Version {
    major: u32,
    minor: u32,
    patch: u32,
    prerelease: Vec<String>,
    build: Vec<String>,
}

impl Version {
    /// `0.0.0-0`, the floor every wildcard lower bound is rewritten to
    /// (`semver.versionZero`).
    fn zero() -> Self {
        Self { prerelease: vec!["0".to_string()], ..Self::default() }
    }

    fn increment_major(&self) -> Self {
        Self { major: self.major + 1, ..Self::default() }
    }

    fn increment_minor(&self) -> Self {
        Self { major: self.major, minor: self.minor + 1, ..Self::default() }
    }

    fn increment_patch(&self) -> Self {
        Self { major: self.major, minor: self.minor, patch: self.patch + 1, ..Self::default() }
    }

    /// Parse a version, allowing `X` and `X.Y` (`semver.TryParseVersion`).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (numbers, rest) = split_numeric_triple(text)?;
        let (major, minor, patch) = numbers;

        let mut version =
            Self { major, minor: minor.unwrap_or(0), patch: patch.unwrap_or(0), ..Self::default() };

        // A prerelease or build suffix is only legal once the patch is explicit,
        // which the triple scanner has already enforced.
        let (prerelease, build) = split_qualifiers(rest)?;
        if let Some(prerelease) = prerelease {
            if !is_valid_prerelease(prerelease) {
                return None;
            }
            version.prerelease = prerelease.split('.').map(str::to_string).collect();
        }
        if let Some(build) = build {
            if !is_valid_build(build) {
                return None;
            }
            version.build = build.split('.').map(str::to_string).collect();
        }
        Some(version)
    }

    /// Parse or panic. Used only for the compiler's own version constant.
    ///
    /// # Panics
    ///
    /// If `text` is not a version.
    #[must_use]
    pub fn must_parse(text: &str) -> Self {
        Self::parse(text).unwrap_or_else(|| panic!("could not parse version string from {text:?}"))
    }

    /// Precedence order (`semver.Version.Compare`).
    ///
    /// Build metadata is excluded, per the specification.
    #[must_use]
    pub fn compare(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
            .then_with(|| compare_prerelease_identifiers(&self.prerelease, &other.prerelease))
    }
}

/// A pre-release version has *lower* precedence than the release
/// (`semver.comparePreReleaseIdentifiers`).
fn compare_prerelease_identifiers(left: &[String], right: &[String]) -> Ordering {
    match (left.is_empty(), right.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }
    for (l, r) in left.iter().zip(right) {
        let order = compare_prerelease_identifier(l, r);
        if order != Ordering::Equal {
            return order;
        }
    }
    left.len().cmp(&right.len())
}

fn compare_prerelease_identifier(left: &str, right: &str) -> Ordering {
    let lexical = left.cmp(right);
    if lexical == Ordering::Equal {
        return lexical;
    }
    let left_numeric = is_numeric_identifier(left);
    let right_numeric = is_numeric_identifier(right);
    if left_numeric || right_numeric {
        // Numeric identifiers rank below non-numeric ones.
        if !right_numeric {
            return Ordering::Less;
        }
        if !left_numeric {
            return Ordering::Greater;
        }
        return match (left.parse::<u32>(), right.parse::<u32>()) {
            (Ok(l), Ok(r)) => l.cmp(&r),
            // Overflow only. Longer digit strings are larger; equal lengths fall
            // back to the lexical order, which is then correct.
            _ => left.len().cmp(&right.len()).then(lexical),
        };
    }
    lexical
}

/// `0` or a leading-zero-free positive integer (`semver.numericIdentifierRegExp`).
fn is_numeric_identifier(text: &str) -> bool {
    text == "0"
        || (text.starts_with(|c: char| c.is_ascii_digit() && c != '0')
            && text.bytes().all(|b| b.is_ascii_digit()))
}

/// Scan `nr(.nr(.nr)?)?` from the start, returning the numbers and the remainder.
///
/// Mirrors the leading three groups of `semver.versionRegexp`. Returns `None` if
/// the shape does not match, including a trailing `.` — which is exactly the
/// `"1."` and `"1.."` keys the corpus uses to test invalid ranges.
type NumericTriple = (u32, Option<u32>, Option<u32>);

fn split_numeric_triple(text: &str) -> Option<(NumericTriple, &str)> {
    let (major, rest) = take_number(text)?;
    let Some(rest) = rest.strip_prefix('.') else {
        return if rest.is_empty() { Some(((major, None, None), rest)) } else { None };
    };
    let (minor, rest) = take_number(rest)?;
    let Some(rest) = rest.strip_prefix('.') else {
        return if rest.is_empty() { Some(((major, Some(minor), None), rest)) } else { None };
    };
    let (patch, rest) = take_number(rest)?;
    Some(((major, Some(minor), Some(patch)), rest))
}

/// `0 | [1-9]\d*` from the start of `text`.
fn take_number(text: &str) -> Option<(u32, &str)> {
    let digits = text.len() - text.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return None;
    }
    let (number, rest) = text.split_at(digits);
    // No leading zeroes, so `01` is not a version and `0` is.
    if number.len() > 1 && number.starts_with('0') {
        return None;
    }
    Some((number.parse().ok()?, rest))
}

/// Split `-pre+build` off the end of a version's remainder.
///
/// Returns `None` if there is trailing text that is neither.
fn split_qualifiers(rest: &str) -> Option<(Option<&str>, Option<&str>)> {
    if rest.is_empty() {
        return Some((None, None));
    }
    if let Some(after) = rest.strip_prefix('-') {
        return match after.split_once('+') {
            Some((prerelease, build)) => Some((Some(prerelease), Some(build))),
            None => Some((Some(after), None)),
        };
    }
    if let Some(build) = rest.strip_prefix('+') {
        return Some((None, Some(build)));
    }
    None
}

/// `semver.prereleaseRegexp`: dot-separated parts, each a numeric identifier or
/// an alphanumeric-or-hyphen run not starting with a digit.
fn is_valid_prerelease(text: &str) -> bool {
    !text.is_empty()
        && text.split('.').all(|part| {
            !part.is_empty()
                && (is_numeric_identifier(part)
                    || (!part.starts_with(|c: char| c.is_ascii_digit())
                        && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')))
        })
}

/// `semver.buildRegExp`: dot-separated alphanumeric-or-hyphen runs.
fn is_valid_build(text: &str) -> bool {
    !text.is_empty()
        && text.split('.').all(|part| {
            !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operator {
    LessThan,
    LessThanEqual,
    Equal,
    GreaterThanEqual,
    GreaterThan,
}

#[derive(Debug, Clone)]
struct Comparator {
    operator: Operator,
    operand: Version,
}

/// A disjunction of conjunctions of comparators (`semver.VersionRange`).
///
/// An empty range means "any version", which is how `"*"` and `""` are
/// represented — and is why [`VersionRange::test`] returns `true` for a range
/// nobody wrote.
#[derive(Debug, Clone, Default)]
pub struct VersionRange {
    alternatives: Vec<Vec<Comparator>>,
}

impl VersionRange {
    /// Parse an npm-style range (`semver.TryParseVersionRange`).
    ///
    /// Supports `||`, space-separated conjunctions, hyphen ranges, and the `~`,
    /// `^`, `<`, `<=`, `=`, `>=`, `>` operators.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut alternatives = Vec::new();
        for range in text.trim().split("||") {
            let range = range.trim();
            if range.is_empty() {
                continue;
            }
            let comparators = if let Some((left, right)) = split_hyphen_range(range) {
                parse_hyphen(left, right)?
            } else {
                let mut comparators = Vec::new();
                for simple in range.split_whitespace() {
                    comparators.extend(parse_simple(simple)?);
                }
                comparators
            };
            alternatives.push(comparators);
        }
        Some(Self { alternatives })
    }

    /// Whether `version` satisfies this range (`semver.VersionRange.Test`).
    #[must_use]
    pub fn test(&self, version: &Version) -> bool {
        if self.alternatives.is_empty() {
            return true;
        }
        self.alternatives.iter().any(|alternative| {
            alternative.iter().all(|comparator| {
                let order = version.compare(&comparator.operand);
                match comparator.operator {
                    Operator::LessThan => order.is_lt(),
                    Operator::LessThanEqual => order.is_le(),
                    Operator::Equal => order.is_eq(),
                    Operator::GreaterThanEqual => order.is_ge(),
                    Operator::GreaterThan => order.is_gt(),
                }
            })
        })
    }
}

/// `partial ' - ' partial` (`semver.hyphenRegExp`).
fn split_hyphen_range(text: &str) -> Option<(&str, &str)> {
    let text = text.trim();
    // The separator is a hyphen surrounded by whitespace, which is what
    // distinguishes it from a hyphen inside a prerelease identifier.
    let index = text.match_indices(" - ").next()?.0;
    let left = text[..index].trim();
    let right = text[index + 3..].trim();
    (is_range_operand(left) && is_range_operand(right)).then_some((left, right))
}

/// The `[a-z0-9-+.*]+` operand character class, case-insensitively.
fn is_range_operand(text: &str) -> bool {
    !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'+' | b'.' | b'*'))
}

/// A partial version, remembering which parts were wildcards
/// (`semver.partialVersion`).
///
/// The strings matter as much as the numbers: `4` and `4.0.0` parse to the same
/// `Version` but bound differently, because only the first has a wildcard minor.
struct PartialVersion {
    version: Version,
    major: String,
    minor: String,
    patch: String,
}

fn is_wildcard(text: &str) -> bool {
    matches!(text, "*" | "x" | "X")
}

/// `semver.parsePartial`.
fn parse_partial(text: &str) -> Option<PartialVersion> {
    // Split the numeric head from the qualifier tail. A prerelease or build
    // suffix is only legal once all three components are written out, which is
    // what the dot count checks — `4-beta` is not a partial version.
    let head_len =
        text.find(['-', '+']).filter(|_| text.matches('.').count() >= 2).unwrap_or(text.len());
    let (head, tail) = text.split_at(head_len);

    let mut components = head.split('.');
    let major = components.next()?.to_string();
    let minor = components.next().unwrap_or("*").to_string();
    let patch = components.next().unwrap_or("*").to_string();
    if components.next().is_some() {
        return None;
    }
    for component in [&major, &minor, &patch] {
        if !is_wildcard(component) && !is_partial_number(component) {
            return None;
        }
    }

    let (major_n, minor_n, patch_n) = if is_wildcard(&major) {
        (0, 0, 0)
    } else if is_wildcard(&minor) {
        (parse_partial_number(&major)?, 0, 0)
    } else if is_wildcard(&patch) {
        (parse_partial_number(&major)?, parse_partial_number(&minor)?, 0)
    } else {
        (
            parse_partial_number(&major)?,
            parse_partial_number(&minor)?,
            parse_partial_number(&patch)?,
        )
    };

    let (prerelease, build) = split_qualifiers(tail)?;
    let version = Version {
        major: major_n,
        minor: minor_n,
        patch: patch_n,
        prerelease: prerelease
            .map(|p| p.split('.').map(str::to_string).collect())
            .unwrap_or_default(),
        build: build.map(|b| b.split('.').map(str::to_string).collect()).unwrap_or_default(),
    };
    Some(PartialVersion { version, major, minor, patch })
}

/// `[x*0]|[1-9]\d*`, the partial-range component class.
fn is_partial_number(text: &str) -> bool {
    text == "0"
        || (text.starts_with(|c: char| c.is_ascii_digit() && c != '0')
            && text.bytes().all(|b| b.is_ascii_digit()))
}

fn parse_partial_number(text: &str) -> Option<u32> {
    text.parse().ok()
}

/// `primitive | partial | tilde | caret` (`semver.rangeRegExp` + `parseComparator`).
fn parse_simple(simple: &str) -> Option<Vec<Comparator>> {
    let simple = simple.trim();
    // Two-character operators first: `<=` would otherwise scan as `<` with an
    // operand of `=1.0`, which is not a legal operand.
    let (op, rest) = if let Some(rest) = simple.strip_prefix("<=") {
        ("<=", rest)
    } else if let Some(rest) = simple.strip_prefix(">=") {
        (">=", rest)
    } else if let Some(rest) = simple.strip_prefix(['~', '^', '<', '>', '=']) {
        (&simple[..1], rest)
    } else {
        ("", simple)
    };
    let operand = rest.trim_start();
    if !is_range_operand(operand) {
        return None;
    }
    parse_comparator(op, operand)
}

/// `semver.parseComparator`.
fn parse_comparator(op: &str, text: &str) -> Option<Vec<Comparator>> {
    let result = parse_partial(text)?;

    if is_wildcard(&result.major) {
        // `<*` and `>*` are unsatisfiable; every other operator against a
        // wildcard major is unconstrained.
        return Some(if op == "<" || op == ">" {
            vec![Comparator { operator: Operator::LessThan, operand: Version::zero() }]
        } else {
            Vec::new()
        });
    }

    let comparators = match op {
        "~" => {
            let upper = if is_wildcard(&result.minor) {
                result.version.increment_major()
            } else {
                result.version.increment_minor()
            };
            vec![
                Comparator {
                    operator: Operator::GreaterThanEqual,
                    operand: result.version.clone(),
                },
                Comparator { operator: Operator::LessThan, operand: upper },
            ]
        }
        "^" => {
            let upper = if result.version.major > 0 || is_wildcard(&result.minor) {
                result.version.increment_major()
            } else if result.version.minor > 0 || is_wildcard(&result.patch) {
                result.version.increment_minor()
            } else {
                result.version.increment_patch()
            };
            vec![
                Comparator {
                    operator: Operator::GreaterThanEqual,
                    operand: result.version.clone(),
                },
                Comparator { operator: Operator::LessThan, operand: upper },
            ]
        }
        "<" | ">=" => {
            let mut version = result.version;
            // The deviation worth remembering: `>=4` means `>=4.0.0-0`, so a
            // prerelease of 4.0.0 satisfies it.
            if is_wildcard(&result.minor) || is_wildcard(&result.patch) {
                version.prerelease = vec!["0".to_string()];
            }
            let operator = if op == "<" { Operator::LessThan } else { Operator::GreaterThanEqual };
            vec![Comparator { operator, operand: version }]
        }
        "<=" | ">" => {
            let mut operator =
                if op == "<=" { Operator::LessThanEqual } else { Operator::GreaterThan };
            let mut version = result.version;
            if is_wildcard(&result.minor) {
                operator = if operator == Operator::LessThanEqual {
                    Operator::LessThan
                } else {
                    Operator::GreaterThanEqual
                };
                version = version.increment_major();
                version.prerelease = vec!["0".to_string()];
            } else if is_wildcard(&result.patch) {
                operator = if operator == Operator::LessThanEqual {
                    Operator::LessThan
                } else {
                    Operator::GreaterThanEqual
                };
                version = version.increment_minor();
                version.prerelease = vec!["0".to_string()];
            }
            vec![Comparator { operator, operand: version }]
        }
        "=" | "" => {
            if is_wildcard(&result.minor) || is_wildcard(&result.patch) {
                let mut lower = result.version.clone();
                lower.prerelease = vec!["0".to_string()];
                let mut upper = if is_wildcard(&result.minor) {
                    result.version.increment_major()
                } else {
                    result.version.increment_minor()
                };
                upper.prerelease = vec!["0".to_string()];
                vec![
                    Comparator { operator: Operator::GreaterThanEqual, operand: lower },
                    Comparator { operator: Operator::LessThan, operand: upper },
                ]
            } else {
                vec![Comparator { operator: Operator::Equal, operand: result.version }]
            }
        }
        _ => return None,
    };
    Some(comparators)
}

/// `semver.parseHyphen`.
fn parse_hyphen(left: &str, right: &str) -> Option<Vec<Comparator>> {
    let left = parse_partial(left)?;
    let right = parse_partial(right)?;
    let mut comparators = Vec::new();
    if !is_wildcard(&left.major) {
        comparators
            .push(Comparator { operator: Operator::GreaterThanEqual, operand: left.version });
    }
    if !is_wildcard(&right.major) {
        let (operator, operand) = if is_wildcard(&right.minor) {
            (Operator::LessThan, right.version.increment_major())
        } else if is_wildcard(&right.patch) {
            (Operator::LessThan, right.version.increment_minor())
        } else {
            (Operator::LessThanEqual, right.version)
        };
        comparators.push(Comparator { operator, operand });
    }
    Some(comparators)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test(range: &str, version: &str) -> bool {
        VersionRange::parse(range)
            .unwrap_or_else(|| panic!("{range:?} should parse"))
            .test(&Version::must_parse(version))
    }

    #[test]
    fn a_bare_major_or_major_minor_is_a_version() {
        // The deviation from strict semver that most corpus `typesVersions` keys
        // rely on.
        assert_eq!(Version::parse("1"), Some(Version { major: 1, ..Version::default() }));
        assert_eq!(
            Version::parse("4.0"),
            Some(Version { major: 4, minor: 0, ..Version::default() })
        );
    }

    #[test]
    fn the_corpus_invalid_keys_do_not_parse() {
        // Every one of these appears in a corpus `typesVersions` block precisely
        // to check that it is rejected.
        for invalid in ["1.", "1..", "3n", "1e0", "01"] {
            assert!(VersionRange::parse(invalid).is_none(), "{invalid:?} should not parse");
        }
        // But the *empty* range is valid and means "any version": upstream skips
        // empty alternatives rather than failing, so `""` parses to the same
        // thing as `"*"`. A `typesVersions` key of `""` therefore matches.
        assert!(test("", "7.1.0-dev"));
    }

    #[test]
    fn a_wildcard_range_admits_everything() {
        assert!(test("*", "7.1.0-dev"));
        // An empty disjunction is the same thing.
        assert!(VersionRange::default().test(&Version::must_parse("1.0.0")));
    }

    #[test]
    fn a_partial_lower_bound_admits_prereleases_of_its_own_version() {
        // `>=4` is `>=4.0.0-0`, so a 4.0.0 prerelease satisfies it. Writing
        // `>=4.0.0` would not, which is why the corpus spells out `>=3.1.0-0`
        // wherever it wants this for a full version.
        assert!(test(">=4", "4.0.0-dev"));
        assert!(!test(">=4.0.0", "4.0.0-dev"));
        assert!(test(">=3.1.0-0", "4.0.0-dev"));
    }

    #[test]
    fn the_compilers_own_version_satisfies_the_ranges_the_corpus_gates_on() {
        // 7.1.0-dev is a *prerelease*, which is the case that makes these
        // gates subtle. If this ever reads false, every `typesVersions` case
        // silently selects a different paths table.
        assert!(test(">=3.1.0-0", "7.1.0-dev"));
        assert!(test(">=4", "7.1.0-dev"));
        assert!(test(">=1", "7.1.0-dev"));
        assert!(!test("<4", "7.1.0-dev"));
        assert!(!test(">=10000", "7.1.0-dev"));
    }

    #[test]
    fn bare_versions_are_ranges_over_their_wildcard_parts() {
        assert!(test("4.0", "4.0.9"));
        assert!(!test("4.0", "4.1.0"));
        assert!(test("4", "4.9.9"));
        assert!(!test("4", "5.0.0"));
    }

    #[test]
    fn a_prerelease_ranks_below_its_release() {
        let pre = Version::must_parse("1.0.0-alpha");
        let release = Version::must_parse("1.0.0");
        assert_eq!(pre.compare(&release), Ordering::Less);
        // Numeric identifiers rank below alphanumeric ones.
        assert_eq!(
            Version::must_parse("1.0.0-1").compare(&Version::must_parse("1.0.0-alpha")),
            Ordering::Less
        );
        // And numeric identifiers compare numerically, not lexically.
        assert_eq!(
            Version::must_parse("1.0.0-2").compare(&Version::must_parse("1.0.0-10")),
            Ordering::Less
        );
    }

    #[test]
    fn build_metadata_does_not_affect_precedence() {
        assert_eq!(
            Version::must_parse("1.0.0+a").compare(&Version::must_parse("1.0.0+b")),
            Ordering::Equal
        );
    }

    #[test]
    fn disjunctions_and_conjunctions_and_hyphens() {
        assert!(test(">=1 <2 || >=4", "4.5.0"));
        assert!(test(">=1 <2", "1.5.0"));
        assert!(!test(">=1 <2", "2.5.0"));
        assert!(test("1.0.0 - 2.0.0", "1.5.0"));
        assert!(!test("1.0.0 - 2.0.0", "2.5.0"));
    }

    #[test]
    fn tilde_and_caret() {
        assert!(test("~1.2.3", "1.2.9"));
        assert!(!test("~1.2.3", "1.3.0"));
        assert!(test("^1.2.3", "1.9.0"));
        assert!(!test("^1.2.3", "2.0.0"));
        assert!(test("^0.2.3", "0.2.9"));
        assert!(!test("^0.2.3", "0.3.0"));
    }
}
