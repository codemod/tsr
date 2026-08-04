//! Locating test cases and the baselines that judge them.
//!
//! # Layout
//!
//! Cases live in the nested TypeScript submodule; baselines live in
//! typescript-go's checked-in reference output:
//!
//! ```text
//! vendor/typescript-go/_submodules/TypeScript/tests/cases/compiler/2dArrays.ts
//! vendor/typescript-go/testdata/baselines/reference/submodule/compiler/2dArrays.types
//! vendor/typescript-go/testdata/baselines/reference/submodule/compiler/2dArrays.symbols
//! vendor/typescript-go/testdata/baselines/reference/submodule/compiler/2dArrays.js
//! ```
//!
//! # The load-bearing convention
//!
//! **A missing `.errors.txt` means "no diagnostics expected".** Upstream only
//! writes that baseline when a case produces errors. That asymmetry is what makes
//! a diagnostics-producing stage measurable at all: for the ~40% of cases with no
//! `.errors.txt`, the expected output is exactly "nothing", which a parser can be
//! held to long before a checker exists.
//!
//! # Configuration-varied baselines
//!
//! A case declaring `// @target: es5, es2015` is compiled once per configuration,
//! and each run writes its own baseline: `case(target=es5).errors.txt`. **793
//! cases in the corpus have only varied baselines and no plain `.errors.txt`.**
//!
//! Missing that would silently resolve those cases to "expects no diagnostics",
//! inflating any parser pass rate by up to 793 cases. The baseline index below
//! therefore matches variants, not just exact names.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};

use crate::case::TestCase;

/// The file names present in one suite's baseline directory.
///
/// Built once per suite rather than stat-ing per case: `conformance/` alone holds
/// 22,813 baseline files, and a variant lookup is a prefix scan.
#[derive(Debug, Default)]
pub struct BaselineIndex {
    dir: PathBuf,
    names: HashSet<String>,
}

impl BaselineIndex {
    /// Index a baseline directory. A missing directory yields an empty index.
    pub fn load(dir: &Path) -> Result<Self> {
        let mut names = HashSet::new();
        if dir.is_dir() {
            for entry in
                std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?
            {
                if let Some(name) = entry?.file_name().to_str() {
                    names.insert(name.to_string());
                }
            }
        }
        Ok(Self { dir: dir.to_path_buf(), names })
    }

    /// The directory this index covers.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Whether the directory exists and was indexed.
    #[must_use]
    pub fn is_present(&self) -> bool {
        self.dir.is_dir()
    }

    /// Whether `<stem>.<extension>` exists exactly.
    #[must_use]
    pub fn has_exact(&self, stem: &str, extension: &str) -> bool {
        self.names.contains(&format!("{stem}.{extension}"))
    }

    /// Whether any `<stem>(...).<extension>` variant exists.
    ///
    /// This is what distinguishes "this case genuinely produces no diagnostics"
    /// from "its diagnostics live under a configuration-suffixed name".
    #[must_use]
    pub fn has_variant(&self, stem: &str, extension: &str) -> bool {
        let prefix = format!("{stem}(");
        let suffix = format!(".{extension}");
        self.names.iter().any(|name| name.starts_with(&prefix) && name.ends_with(&suffix))
    }

    /// Whether a baseline of this extension exists under any configuration.
    #[must_use]
    pub fn has_any(&self, stem: &str, extension: &str) -> bool {
        self.has_exact(stem, extension) || self.has_variant(stem, extension)
    }
}

/// The suites we enumerate from the corpus.
///
/// `fourslash` (editor-behaviour tests) and `project` are excluded: they are not
/// file-in/baseline-out compiler tests and need their own runners.
pub const SUITE_DIRS: &[&str] = &["compiler", "conformance"];

/// Paths into the vendored upstream checkout.
#[derive(Debug, Clone)]
pub struct Corpus {
    /// `vendor/typescript-go`.
    pub upstream: PathBuf,
}

/// A discovered case: where its source is and where its baselines are.
#[derive(Debug, Clone)]
pub struct CaseEntry {
    /// Suite-relative name without extension, e.g. `compiler/2dArrays`.
    pub name: String,
    /// Absolute path to the case source.
    pub path: PathBuf,
    /// Index of the baseline directory this case's outputs live in.
    pub baselines: Arc<BaselineIndex>,
}

impl CaseEntry {
    /// The unit name given to content before the first `@filename`.
    #[must_use]
    pub fn default_unit_name(&self) -> String {
        self.path
            .file_name()
            .map_or_else(|| "input.ts".to_string(), |n| n.to_string_lossy().into_owned())
    }

    /// Read and parse the case.
    pub fn load(&self) -> Result<TestCase> {
        let source = read_lossy(&self.path)
            .with_context(|| format!("reading case {}", self.path.display()))?;
        Ok(TestCase::parse(&self.name, &self.default_unit_name(), &source))
    }

    /// The case's basename, which is how baselines are keyed.
    #[must_use]
    pub fn stem(&self) -> &str {
        self.name.rsplit('/').next().unwrap_or(&self.name)
    }

    /// Path to a baseline of the given extension, whether or not it exists.
    #[must_use]
    pub fn baseline_path(&self, extension: &str) -> PathBuf {
        self.baselines.dir().join(format!("{}.{extension}", self.stem()))
    }

    /// Whether this case's diagnostics live under configuration-suffixed names.
    ///
    /// True for the 793 cases that vary by `@target` and friends. Such a case
    /// *does* produce diagnostics, so it must never be mistaken for one that
    /// produces none.
    #[must_use]
    pub fn has_varied_errors(&self) -> bool {
        self.baselines.has_variant(self.stem(), "errors.txt")
    }

    /// The expected diagnostics baseline.
    ///
    /// `Ok(None)` means the case is expected to produce **no** diagnostics — see
    /// the module docs. That is a real expectation, not missing data.
    /// Returns `Err` for a configuration-varied case: there is no single expected
    /// output, so callers must not treat the absence of a plain baseline as
    /// "expects nothing". Use [`CaseEntry::has_varied_errors`] to detect that case
    /// before calling this.
    pub fn expected_errors(&self) -> Result<Option<String>> {
        let path = self.baseline_path("errors.txt");
        if path.exists() {
            Ok(Some(read_lossy(&path).with_context(|| format!("reading {}", path.display()))?))
        } else {
            Ok(None)
        }
    }

    /// Whether the `.types` baseline is configuration-varied.
    ///
    /// 2,032 of the 12,155 are. Matching `case(target=es5).types` against a single
    /// default compilation is not a comparison — the same trap `has_varied_errors`
    /// guards. See `bd tsr-bb4.1`.
    #[must_use]
    pub fn has_varied_types(&self) -> bool {
        self.baselines.has_variant(self.stem(), "types")
    }

    /// The `.types` baseline, if upstream recorded one.
    ///
    /// The checker's oracle; see [`crate::types_baseline`].
    #[must_use]
    pub fn expected_types(&self) -> Option<String> {
        let path = self.baseline_path("types");
        path.exists().then(|| read_lossy(&path).ok()).flatten()
    }

    /// Whether the `.symbols` baseline is configuration-varied.
    ///
    /// Mirrors [`CaseEntry::has_varied_types`]; the two baselines are written in
    /// lockstep upstream, 12,155 of each.
    #[must_use]
    pub fn has_varied_symbols(&self) -> bool {
        self.baselines.has_variant(self.stem(), "symbols")
    }

    /// The `.symbols` baseline, if upstream recorded one.
    ///
    /// The binder's oracle; see [`crate::symbols_baseline`].
    #[must_use]
    pub fn expected_symbols(&self) -> Option<String> {
        let path = self.baseline_path("symbols");
        path.exists().then(|| read_lossy(&path).ok()).flatten()
    }

    /// Whether upstream recorded *any* output for this case.
    ///
    /// 617 cases have none — upstream never ran them, so there is no evidence of
    /// what they should produce. A missing `.errors.txt` means "no diagnostics"
    /// only when some other baseline proves the case was run; without that, the
    /// absence proves nothing, and treating it as a clean expectation both
    /// inflates the denominator and hands free passes to whatever we happen to
    /// accept.
    ///
    /// The same trap as configuration-varied baselines, in a different guise.
    #[must_use]
    pub fn has_any_baseline(&self) -> bool {
        let stem = self.stem();
        ["errors.txt", "types", "symbols", "js", "diff"]
            .iter()
            .any(|extension| self.baselines.has_any(stem, extension))
    }

    /// Whether upstream recorded a known divergence from TypeScript for this case.
    ///
    /// typescript-go writes `.diff` baselines where its output intentionally
    /// differs from the TypeScript reference. Those cases cannot be judged against
    /// TypeScript's expectations without accounting for the divergence.
    #[must_use]
    pub fn has_known_divergence(&self) -> bool {
        let stem = self.stem();
        self.baselines.has_any(stem, "errors.txt.diff")
            || self.baselines.has_any(stem, "types.diff")
            || self.baselines.has_any(stem, "js.diff")
    }
}

impl Corpus {
    /// Point the harness at a vendored upstream checkout.
    #[must_use]
    pub fn new(upstream: impl Into<PathBuf>) -> Self {
        Self { upstream: upstream.into() }
    }

    /// Locate the corpus relative to the repository root.
    #[must_use]
    pub fn from_repo_root(root: &Path) -> Self {
        Self::new(root.join("vendor/typescript-go"))
    }

    /// Where the TypeScript submodule's test cases live.
    #[must_use]
    pub fn cases_root(&self) -> PathBuf {
        self.upstream.join("_submodules/TypeScript/tests/cases")
    }

    /// Where typescript-go's reference baselines for those cases live.
    #[must_use]
    pub fn baselines_root(&self) -> PathBuf {
        self.upstream.join("testdata/baselines/reference/submodule")
    }

    /// Whether the vendored corpus is present.
    #[must_use]
    pub fn is_available(&self) -> bool {
        self.cases_root().is_dir() && self.baselines_root().is_dir()
    }

    /// Enumerate every case in [`SUITE_DIRS`], sorted for stable output.
    ///
    /// Sorting matters: snapshots are committed and diffed, so a directory-order
    /// dependent listing would produce spurious churn across machines.
    pub fn discover(&self) -> Result<Vec<CaseEntry>> {
        let mut entries = Vec::new();
        for suite in SUITE_DIRS {
            let root = self.cases_root().join(suite);
            if !root.is_dir() {
                continue;
            }
            let index = Arc::new(BaselineIndex::load(&self.baselines_root().join(suite))?);
            collect(&root, suite, &index, &mut entries)?;
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }
}

/// Recursively collect `.ts`/`.tsx` cases.
///
/// `conformance/` nests several levels deep; baselines are flat within the suite,
/// keyed by basename only, so `baseline_dir` does not mirror the nesting.
fn collect(
    dir: &Path,
    suite: &str,
    baselines: &Arc<BaselineIndex>,
    out: &mut Vec<CaseEntry>,
) -> Result<()> {
    let read = std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    for entry in read {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect(&path, suite, baselines, out)?;
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else { continue };
        if ext != "ts" && ext != "tsx" {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        out.push(CaseEntry {
            name: format!("{suite}/{stem}"),
            path: path.clone(),
            baselines: Arc::clone(baselines),
        });
    }
    Ok(())
}

/// Read a file as UTF-8, replacing invalid sequences.
///
/// A handful of corpus files are deliberately malformed to exercise encoding
/// handling; refusing to read them would remove them from the denominator, which
/// is the opposite of what a conformance harness should do.
fn read_lossy(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_with(names: &[&str]) -> BaselineIndex {
        BaselineIndex {
            dir: PathBuf::from("/baselines"),
            names: names.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    #[test]
    fn exact_baselines_are_found() {
        let index = index_with(&["a.errors.txt", "a.types"]);
        assert!(index.has_exact("a", "errors.txt"));
        assert!(!index.has_exact("b", "errors.txt"));
    }

    #[test]
    fn configuration_varied_baselines_are_found() {
        // The bug this guards: a case whose only diagnostics baseline is
        // `a(target=es2015).errors.txt` must not read as "expects no diagnostics".
        // Missing this inflated the reachable-target count by 793 cases.
        let index = index_with(&["a(target=es2015).errors.txt", "a(target=es5).errors.txt"]);
        assert!(!index.has_exact("a", "errors.txt"));
        assert!(index.has_variant("a", "errors.txt"));
        assert!(index.has_any("a", "errors.txt"));
    }

    #[test]
    fn variant_matching_does_not_bleed_across_similar_stems() {
        // `a` must not match `ab(target=es5).errors.txt`; the open paren is what
        // delimits the stem.
        let index = index_with(&["ab(target=es5).errors.txt"]);
        assert!(!index.has_variant("a", "errors.txt"));
        assert!(index.has_variant("ab", "errors.txt"));
    }

    #[test]
    fn variant_matching_respects_the_extension() {
        let index = index_with(&["a(target=es5).types"]);
        assert!(!index.has_variant("a", "errors.txt"));
        assert!(index.has_variant("a", "types"));
    }

    #[test]
    fn a_missing_baseline_directory_indexes_empty_rather_than_erroring() {
        let index = BaselineIndex::load(Path::new("/definitely/not/here")).expect("no error");
        assert!(!index.is_present());
        assert!(!index.has_any("a", "errors.txt"));
    }
}
