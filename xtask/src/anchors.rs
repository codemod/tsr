//! `cargo xtask anchors` — verify that upstream anchors still resolve.
//!
//! # What an anchor is for
//!
//! [ADR-0001](../../docs/adr/0001-idiomatic-rewrite.md) chose an idiomatic
//! rewrite, which means an upstream fix cannot be diffed and replayed onto this
//! tree — each one has to be re-derived by hand. The anchors are what make that
//! mechanical rather than archaeological: they let an upstream commit touching
//! `internal/checker/relations.go` resolve to the Rust items claiming to port it.
//!
//! An anchor nobody checks is worse than no anchor, because it is *believed*. This
//! session produced three unverified claims about upstream in one slice — a method
//! count wrong by 3×, a line number pointing at a blank line, and a "every child
//! list goes through `emit_list`" that two call sites did not — and all three were
//! caught by a human reading rather than by a tool.
//!
//! # One tool, two modes
//!
//! The check is the same question asked against two different trees:
//!
//! | | upstream tree | an unresolvable anchor means |
//! |---|---|---|
//! | **lint** (`bd tsr-5e7.3`) | the pinned submodule | the anchor was wrong when written |
//! | **drift** (`bd tsr-l68`) | a newer checkout, via `--upstream` | upstream moved, and *this* item is affected |
//!
//! That is why they are one program. A drift tracker that classified upstream
//! commits by package — the original sketch on `bd tsr-l68` — would file one issue
//! against a whole crate; this resolves to the item.
//!
//! # Why the coverage half does not fail the build
//!
//! `bd tsr-5e7.3` asks for a lint that "fails CI on unanchored public items".
//! Measured before building it, the two largest crates are at 8 anchors for 8,648
//! lines and 13 for 5,407. A gate that fails on every unanchored public item would
//! fail on thousands from the first run, and a gate that always fails is turned
//! off within a day.
//!
//! So the two halves are separated deliberately:
//!
//! - **Broken anchors fail.** There are few, they are always bugs, and the number
//!   can be held at zero from today.
//! - **Missing anchors are counted and printed**, per crate, the way the
//!   conformance suites report a rate. That number is a ratchet to raise, not a
//!   gate to trip.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

/// A reference to something in upstream, extracted from an anchor comment.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Reference {
    /// `internal/printer/printer.go`, or a bare `printer.go`.
    Path(String),
    /// `internal/printer/printer.go:4745`.
    Line(String, usize),
    /// `ensureType`, `ast.SourceFile`, `core.PagedLinkStore`.
    Symbol(String),
}

/// One anchor comment, and everything it claims about upstream.
#[derive(Debug)]
struct Anchor {
    file: PathBuf,
    line: usize,
    references: Vec<Reference>,
}

/// What upstream contains, indexed once.
struct Upstream {
    root: PathBuf,
    /// Every `.go` file, by path relative to the repository root.
    files: BTreeSet<String>,
    /// Line count per file, for range-checking `path:line` anchors.
    lengths: BTreeMap<String, usize>,
    /// Declared symbol name → the packages declaring it.
    symbols: BTreeMap<String, BTreeSet<String>>,
    /// Struct fields, **qualified only**: `SourceFile.GlobalExports`.
    ///
    /// A separate set rather than an entry in `symbols`, because `symbols` is
    /// looked up by the *last* dotted segment — the first being a package or a
    /// method receiver. A field must never resolve from its bare name (`Name` and
    /// `Kind` exist in dozens of structs), so it is matched on the whole string.
    fields: BTreeSet<String>,
    /// Every package (directory) name under `internal/`.
    ///
    /// Needed to tell `ast.SourceFile` — a package qualifier — from
    /// `Printer.emitSourceFile`, which is Go receiver notation for a method. Both
    /// are written with a dot and they mean different things; without this the
    /// check reported every method anchor in `tsr-printer` as "declared in
    /// `printer`, not `Printer`".
    packages: BTreeSet<String>,
}

impl Upstream {
    /// Index every Go declaration in the tree.
    ///
    /// Declarations — `func`, `type`, `const`, `var` and methods — plus struct
    /// fields, which are indexed **only** in qualified `Type.Field` form. A bare
    /// field name like `Name` or `Kind` exists in dozens of structs, so resolving
    /// one would make almost any span resolve and the check would pass vacuously;
    /// `SourceFile.GlobalExports` carries its struct and cannot.
    fn index(root: &Path) -> Result<Self> {
        let mut upstream = Self {
            root: root.to_path_buf(),
            files: BTreeSet::new(),
            lengths: BTreeMap::new(),
            symbols: BTreeMap::new(),
            fields: BTreeSet::new(),
            packages: BTreeSet::new(),
        };
        let internal = root.join("internal");
        if !internal.is_dir() {
            bail!(
                "no `internal/` under {}\n\nThe vendored submodule may not be \
                 initialized; run:\n  git submodule update --init vendor/typescript-go",
                root.display()
            );
        }
        upstream.walk(&internal)?;

        // **`cmd/` too, not only `internal/`.** Every ported item lived under
        // `internal/` until the CLI arrived, and `cmd/tsgo/main.go` and
        // `cmd/tsgo/sys.go` are the driver's counterparts — real files, correctly
        // cited, that this gate reported as unresolved because it only ever
        // looked in one directory. Widening the index is the fix; weakening the
        // anchors to point at something under `internal/` would have made the
        // gate pass by pointing the reader at the wrong file.
        let cmd = root.join("cmd");
        if cmd.is_dir() {
            upstream.walk(&cmd)?;
        }

        Ok(upstream)
    }

    fn walk(&mut self, dir: &Path) -> Result<()> {
        for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
            let path = entry?.path();
            if path.is_dir() {
                self.walk(&path)?;
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "go") {
                continue;
            }
            let relative =
                path.strip_prefix(&self.root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            let source = fs::read_to_string(&path).unwrap_or_default();
            self.lengths.insert(relative.clone(), source.lines().count());
            self.files.insert(relative.clone());

            let package = path
                .parent()
                .and_then(Path::file_name)
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            for name in declarations(&source) {
                if name.contains('.') {
                    self.fields.insert(name);
                } else {
                    self.symbols.entry(name).or_default().insert(package.clone());
                }
            }
            self.packages.insert(package);
        }
        Ok(())
    }

    /// Whether a reference resolves, and if not, why.
    fn resolve(&self, reference: &Reference) -> Option<String> {
        match reference {
            Reference::Path(path) => {
                if self.matching_paths(path).is_empty() {
                    Some(format!("no such file upstream: `{path}`"))
                } else {
                    None
                }
            }
            Reference::Line(path, line) => {
                // A bare `printer.go` matches several files upstream. The anchor is
                // satisfied if *any* of them is long enough — taking the first
                // match reported `printer.go:4745` against a 486-line namesake.
                let matches = self.matching_paths(path);
                if matches.is_empty() {
                    return Some(format!("no such file upstream: `{path}`"));
                }
                let longest = matches
                    .iter()
                    .filter_map(|full| self.lengths.get(*full).copied())
                    .max()
                    .unwrap_or(0);
                if *line == 0 || *line > longest {
                    Some(format!("`{path}:{line}` is past the end of the file ({longest} lines)"))
                } else {
                    None
                }
            }
            Reference::Symbol(symbol) => {
                // A qualified struct field is matched whole; see `fields`.
                if self.fields.contains(symbol) {
                    return None;
                }
                // `ast.SourceFile` qualifies by package; `Printer.emitSourceFile`
                // is receiver notation for a method. Only the first constrains
                // where the symbol may live.
                let (qualifier, name) = match symbol.split_once('.') {
                    Some((qualifier, name)) => (Some(qualifier), name),
                    None => (None, symbol.as_str()),
                };
                let package = qualifier.filter(|q| self.packages.contains(*q));
                match self.symbols.get(name) {
                    None => Some(format!("no such Go declaration upstream: `{symbol}`")),
                    Some(packages) => match package {
                        Some(package) if !packages.contains(package) => Some(format!(
                            "`{symbol}` is declared in {} upstream, not `{package}`",
                            packages.iter().cloned().collect::<Vec<_>>().join(", ")
                        )),
                        _ => None,
                    },
                }
            }
        }
    }

    /// Full paths matching a cited path, which may be bare (`printer.go`).
    fn matching_paths(&self, cited: &str) -> Vec<&String> {
        if self.files.contains(cited) {
            return vec![self.files.get(cited).unwrap()];
        }
        let suffix = format!("/{cited}");
        self.files.iter().filter(|path| path.ends_with(&suffix)).collect()
    }
}

/// Top-level Go declarations in one file, plus struct fields as `Type.Field`.
///
/// Fields are indexed **only in qualified form**. A bare field name is still not a
/// resolvable symbol, which is the point of the original exclusion: `Name` and
/// `Kind` exist in dozens of structs, so indexing them bare would make almost any
/// span resolve and the check would pass vacuously. `SourceFile.GlobalExports`
/// carries the struct, so it is precise and cannot resolve by accident.
///
/// Adding them was forced by measurement, not tidiness. Broadening the anchor
/// phrasings to include `Upstream:`/`Upstream's` surfaced 114 previously unchecked
/// references and 13 failures, of which five were citations of real struct fields
/// (`SourceFile.GlobalExports`, `BodyBase.EndFlowNode`) — false failures of exactly
/// the kind this module says cost more than a missed check. A struct field is a
/// legitimate thing for ported code to cite; the checker port will cite hundreds.
fn declarations(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_block = None::<&str>;
    // The `type X struct {` currently open, so its fields can be qualified.
    let mut in_struct = None::<String>;
    for line in source.lines() {
        let trimmed = line.trim_start();
        // Fields belong to the struct being read. Closing brace at column zero
        // ends it — Go's gofmt guarantees that, and the alternative is counting
        // braces through nested literals for no gain.
        if let Some(struct_name) = &in_struct {
            if line.starts_with('}') {
                in_struct = None;
            } else if leading_identifier(trimmed).is_some() {
                // `Pos, End int` declares two fields on one line: every token that
                // ends in a comma is a field, and so is the one that stops the run.
                for token in trimmed.split_whitespace() {
                    let Some(name) = leading_identifier(token) else { break };
                    names.push(format!("{struct_name}.{name}"));
                    if !token.ends_with(',') {
                        break;
                    }
                }
            }
            continue;
        }
        // `const (` / `var (` / `type (` blocks declare one name per line.
        if let Some(keyword) = in_block {
            if trimmed.starts_with(')') {
                in_block = None;
            } else if let Some(name) = leading_identifier(trimmed) {
                let _ = keyword;
                names.push(name);
            }
            continue;
        }
        for keyword in ["type ", "const ", "var "] {
            if let Some(rest) = trimmed.strip_prefix(keyword) {
                if rest.trim() == "(" {
                    in_block = Some(keyword);
                } else if let Some(name) = leading_identifier(rest) {
                    names.push(name.clone());
                    if keyword == "type " && rest.trim_end().ends_with("struct {") {
                        in_struct = Some(name);
                    }
                }
            }
        }
        if let Some(rest) = trimmed.strip_prefix("func ") {
            // `func (r *Receiver) Name(` — skip the receiver.
            let rest = if rest.starts_with('(') {
                rest.split_once(')').map_or(rest, |(_, after)| after.trim_start())
            } else {
                rest
            };
            if let Some(name) = leading_identifier(rest) {
                names.push(name);
            }
        }
    }
    names
}

/// The identifier at the start of `text`, if there is one.
fn leading_identifier(text: &str) -> Option<String> {
    let name: String = text.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    let first = name.chars().next()?;
    if first.is_ascii_digit() { None } else { Some(name) }
}

/// Anchor phrasings in use. Taken from the tree rather than invented:
/// `Corresponds to` 277, `Ported from` 234, `Stands in for` 5.
///
/// A speculative entry, `"ports "`, was removed: it is a substring of "reports",
/// "supports", "imports" and "exports", so it turned ordinary prose into anchors
/// and then reported their backticks as missing Go declarations.
///
/// **`Upstream:` and `Upstream's` were tried and rejected**, and the attempt is
/// worth recording because the motivation was real. `tsr-binder` reported 7 anchors
/// while containing 89 upstream citations, so a whole session's worth of new line
/// numbers had gone in unverified. Adding those two phrases did check 114 more
/// references — and produced 11 failures that were almost all prose, because
/// "Upstream's" introduces a sentence at least as often as a citation:
/// "Upstream's struct has 27 fields; the ones absent here are the visitors it
/// builds in its constructor" grabbed a backtick from the middle of the following
/// clause. That is the `"ports "` failure again, one layer up.
///
/// The lesson is that a *phrase* cannot make an ambiguous span checkable. What
/// solved the actual problem was [`positions_in`]: a `` `path.go:N` `` span is
/// self-identifying and is now checked wherever it appears, phrase or no phrase.
const PHRASINGS: &[&str] = &["Ported from", "Corresponds to", "Stands in for"];

/// Every `path.go` or `path.go:N` span in one comment line.
///
/// **Not gated on an anchor phrase.** A backticked span ending in `.go`, or in
/// `.go:` and digits, cannot be anything but a claim about upstream: it is not
/// prose, not a Rust path, and not a Go symbol. So it is checked wherever it
/// appears — including mid-sentence, which is where most of them are written:
/// "the message selection (`binder.go:214` block scoped → TS2451)".
///
/// This is what a phrase list cannot do. Requiring `Ported from` before a line
/// number left 27 `file.go:N` references in `tsr-binder` unchecked; broadening the
/// phrases to reach them swept in prose instead (see [`PHRASINGS`]). Positions are
/// self-identifying, so they need no introduction; symbols are ambiguous, so they
/// still do.
fn positions_in(text: &str) -> Vec<Reference> {
    backticked(text)
        .iter()
        .filter_map(|span| match classify(span) {
            Some(reference @ (Reference::Path(_) | Reference::Line(..))) => Some(reference),
            _ => None,
        })
        .collect()
}

/// Extract every anchor from one Rust file.
fn anchors_in(path: &Path, source: &str) -> Vec<Anchor> {
    let lines: Vec<&str> = source.lines().collect();
    let mut anchors = Vec::new();
    // Lines already claimed by a phrase anchor's wrap window. Without this, the
    // continuation line of `Ported from \`X\`\n/// (\`file.go:63\`)` becomes a
    // second, position-only anchor and the reference is counted twice — which is
    // how the existing extraction test caught this.
    let mut claimed = BTreeSet::new();
    for (index, line) in lines.iter().enumerate() {
        if !is_comment(line) {
            continue;
        }
        let Some(phrase) = PHRASINGS.iter().find(|phrase| line.contains(**phrase)) else {
            // No anchor phrase, but a cited file or line still has to resolve.
            let positions = positions_in(line);
            if !positions.is_empty() && !claimed.contains(&index) {
                anchors.push(Anchor {
                    file: path.to_path_buf(),
                    line: index + 1,
                    references: positions,
                });
            }
            continue;
        };
        // An anchor's claim can wrap, so the window is this comment line and up to
        // two continuations.
        let window_lines: Vec<&str> = lines[index..]
            .iter()
            .take(3)
            .take_while(|following| is_comment(following))
            .copied()
            .collect();
        claimed.extend(index..index + window_lines.len());
        let window = window_lines.join(" ");
        let after = window.find(*phrase).map_or("", |at| &window[at + phrase.len()..]);
        let references = references_in(after);
        if !references.is_empty() {
            anchors.push(Anchor { file: path.to_path_buf(), line: index + 1, references });
        }
    }
    anchors
}

fn is_comment(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("///") || trimmed.starts_with("//!") || trimmed.starts_with("//")
}

/// The upstream claims an anchor makes.
///
/// **Only what the anchor phrase introduces**, plus any Go path in the same
/// anchor. An earlier version classified *every* backticked span in the comment,
/// which reported `TS9017`, `package.json`, `None` and half a dozen corpus case
/// names as missing Go declarations — 42 failures, none of them real.
///
/// That matters more than tidiness. This module's own docs say a gate that always
/// fails is turned off within a day, and a check that doubts prose is that gate.
/// The rule is now the shape anchors are actually written in — "Ported from
/// typescript-go's `X` (`path.go:N`)" — and a span it cannot place is ignored
/// rather than doubted.
fn references_in(after_phrase: &str) -> Vec<Reference> {
    let spans = backticked(after_phrase);
    let mut references = Vec::new();
    for span in &spans {
        if let Some(reference @ (Reference::Path(_) | Reference::Line(..))) = classify(span) {
            references.push(reference);
        }
    }
    if let Some(first) = spans.first()
        && let Some(reference @ Reference::Symbol(_)) = classify(first)
    {
        references.push(reference);
    }
    references.sort();
    references.dedup();
    references
}

/// Every backticked span, in order.
fn backticked(text: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else { break };
        spans.push(after[..close].to_string());
        rest = &after[close + 1..];
    }
    spans
}

/// What one backticked span claims, if anything checkable.
///
/// Conservative on purpose: a span this cannot classify is ignored rather than
/// reported, because a false failure costs more than a missed check.
fn classify(span: &str) -> Option<Reference> {
    let span = span.trim().trim_start_matches('*');
    if span.is_empty() || span.contains(' ') {
        return None;
    }
    // `ends_with(".go")` rather than `Path::extension`: these are cited strings,
    // not filesystem paths, and Go source is `.go` exactly — a case-insensitive
    // match would accept `.GO`, which upstream never writes.
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    if let Some((path, line)) = span.rsplit_once(':')
        && path.ends_with(".go")
        && let Ok(line) = line.parse::<usize>()
    {
        return Some(Reference::Line(path.to_string(), line));
    }
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    if span.ends_with(".go") {
        return Some(Reference::Path(span.to_string()));
    }
    // Rust paths, prose and anything with a non-Go extension are not Go symbols.
    if span.contains("::") || span.contains('/') || span.contains('(') || span.starts_with('.') {
        return None;
    }
    if !span.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
        return None;
    }
    // A diagnostic code is a contract with TypeScript, not a Go declaration —
    // `tsr-dts` anchors to these deliberately (ADR-0021).
    if let Some(rest) = span.strip_prefix("TS")
        && !rest.is_empty()
        && rest.chars().all(|c| c.is_ascii_digit() || c == 'x')
    {
        return None;
    }
    // Every segment of a Go symbol is an identifier, and the last one carries an
    // uppercase letter unless it stands alone. This rejects `a.b.c`,
    // `module.exports` and `package.json` without needing a list of exceptions.
    let segments: Vec<&str> = span.split('.').collect();
    if segments.iter().any(|segment| segment.is_empty()) {
        return None;
    }
    if segments.len() > 1 && !segments.last().is_some_and(|s| s.chars().any(char::is_uppercase)) {
        return None;
    }
    if !span.chars().any(char::is_uppercase) {
        return None;
    }
    Some(Reference::Symbol(span.to_string()))
}

/// Public items and how many carry an anchor, per crate.
struct Coverage {
    items: usize,
    anchored: usize,
}

/// Count public items and those whose preceding doc comment anchors.
fn coverage_of(source: &str) -> Coverage {
    let lines: Vec<&str> = source.lines().collect();
    let mut coverage = Coverage { items: 0, anchored: 0 };
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        // `pub(crate)` counts: a ported item is a ported item whatever its
        // visibility, and `tsr-printer` is almost entirely crate-internal — counting
        // only `pub` put it at 3 items against 153 anchors.
        let visible = trimmed.strip_prefix("pub(crate) ").or_else(|| trimmed.strip_prefix("pub "));
        let is_item = visible.is_some_and(|rest| {
            ["fn ", "struct ", "enum ", "trait ", "const "]
                .iter()
                .any(|kind| rest.starts_with(kind))
        });
        if !is_item {
            continue;
        }
        coverage.items += 1;
        // Walk back over the doc comment and attributes immediately above.
        let mut cursor = index;
        let mut anchored = false;
        while cursor > 0 {
            cursor -= 1;
            let above = lines[cursor].trim_start();
            if above.starts_with("///") || above.starts_with("#[") || above.starts_with("//") {
                if PHRASINGS.iter().any(|phrase| above.contains(phrase)) {
                    anchored = true;
                }
            } else {
                break;
            }
        }
        if anchored {
            coverage.anchored += 1;
        }
    }
    coverage
}

/// Run the check.
pub fn run(root: &Path, upstream_override: Option<PathBuf>) -> Result<()> {
    let upstream_root = upstream_override.unwrap_or_else(|| root.join("vendor/typescript-go"));
    let upstream = Upstream::index(&upstream_root)?;
    println!(
        "upstream: {} ({} Go files, {} declarations)\n",
        upstream_root.display(),
        upstream.files.len(),
        upstream.symbols.len()
    );

    let mut per_crate: BTreeMap<String, (Coverage, usize)> = BTreeMap::new();
    let mut broken: Vec<(PathBuf, usize, String)> = Vec::new();
    let mut checked = 0usize;

    let crates = root.join("crates");
    for entry in fs::read_dir(&crates).with_context(|| format!("reading {}", crates.display()))? {
        let path = entry?.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let source_dir = path.join("src");
        if !source_dir.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        collect_rust(&source_dir, &mut files)?;
        for file in files {
            // Generated code carries anchors it did not choose; regenerating is
            // what keeps those honest, so they are counted but never failed on.
            let generated = file.components().any(|c| c.as_os_str() == "generated");
            let source = fs::read_to_string(&file)?;
            let entry = per_crate
                .entry(name.clone())
                .or_insert_with(|| (Coverage { items: 0, anchored: 0 }, 0));
            if !generated {
                let coverage = coverage_of(&source);
                entry.0.items += coverage.items;
                entry.0.anchored += coverage.anchored;
            }
            for anchor in anchors_in(&file, &source) {
                entry.1 += 1;
                for reference in &anchor.references {
                    checked += 1;
                    if let Some(problem) = upstream.resolve(reference)
                        && !generated
                    {
                        broken.push((anchor.file.clone(), anchor.line, problem));
                    }
                }
            }
        }
    }

    println!("{:<22} {:>9}  {:>22}", "crate", "anchors", "public items anchored");
    for (name, (coverage, anchors)) in &per_crate {
        if *anchors == 0 && coverage.items == 0 {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let rate = if coverage.items == 0 {
            0.0
        } else {
            coverage.anchored as f64 / coverage.items as f64 * 100.0
        };
        println!(
            "{name:<22} {anchors:>9}  {:>22}",
            format!("{}/{} ({rate:.0}%)", coverage.anchored, coverage.items)
        );
    }

    println!("\n{checked} upstream references checked, {} unresolved", broken.len());
    if broken.is_empty() {
        return Ok(());
    }
    println!();
    for (file, line, problem) in &broken {
        let shown = file.strip_prefix(root).unwrap_or(file);
        println!("  {}:{line}: {problem}", shown.display());
    }
    bail!("{} anchor(s) do not resolve against {}", broken.len(), upstream_root.display())
}

fn collect_rust(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_rust(&path, out)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_declarations_are_indexed_and_struct_fields_only_qualified() {
        let source = "\
package printer

type Printer struct {
	writer textWriter
	Name   string
	Pos, End int
}

func NewPrinter() *Printer { return nil }

func (p *Printer) emitList(node *ast.Node) {}

const (
	LFNone ListFormat = 0
	LFMultiLine
)

var defaultIndentSize = 4
";
        let names = declarations(source);
        assert!(names.contains(&"Printer".to_string()));
        assert!(names.contains(&"NewPrinter".to_string()));
        assert!(names.contains(&"emitList".to_string()), "methods must be indexed");
        assert!(names.contains(&"LFNone".to_string()), "const blocks declare one name per line");
        assert!(names.contains(&"LFMultiLine".to_string()));
        assert!(names.contains(&"defaultIndentSize".to_string()));
        // A **bare** field name would make almost any span resolve and the check
        // would pass vacuously, so fields are indexed only with their struct.
        assert!(!names.contains(&"writer".to_string()), "a bare field must not resolve");
        assert!(!names.contains(&"Name".to_string()), "a bare field must not resolve");
        assert!(names.contains(&"Printer.writer".to_string()), "qualified fields resolve");
        assert!(names.contains(&"Printer.Name".to_string()));
        // `Pos, End int` declares two fields on one line.
        assert!(names.contains(&"Printer.Pos".to_string()));
        assert!(names.contains(&"Printer.End".to_string()));
        // The struct ends at the closing brace; what follows is not a field.
        assert!(!names.contains(&"Printer.NewPrinter".to_string()));
    }

    #[test]
    fn a_backticked_span_is_classified_only_when_it_is_checkable() {
        assert_eq!(
            classify("internal/printer/printer.go:4745"),
            Some(Reference::Line("internal/printer/printer.go".into(), 4745))
        );
        assert_eq!(
            classify("internal/printer/printer.go"),
            Some(Reference::Path("internal/printer/printer.go".into()))
        );
        assert_eq!(
            classify("ensureType"),
            Some(Reference::Symbol("ensureType".into())),
            "camelCase is an upstream symbol"
        );
        assert_eq!(classify("readonly"), None, "an all-lowercase word is too often prose");
        assert_eq!(classify("ast.SourceFile"), Some(Reference::Symbol("ast.SourceFile".into())));
        assert_eq!(classify("*ast.Node"), Some(Reference::Symbol("ast.Node".into())));
        assert_eq!(
            classify("DeclarationTransformer"),
            Some(Reference::Symbol("DeclarationTransformer".into()))
        );
        // Not upstream, and must not be guessed at.
        assert_eq!(classify("crate::factory"), None);
        assert_eq!(classify("const"), None);
        assert_eq!(classify("Printer::emit_list"), None);
        assert_eq!(classify("a b"), None);
    }

    #[test]
    fn an_anchor_claims_what_its_backticks_say() {
        let source = "\
/// Ported from typescript-go's `DeclarationTransformer`
/// (`internal/transformers/declarations/transform.go:63`).
pub struct Transformer;
";
        let anchors = anchors_in(Path::new("x.rs"), source);
        assert_eq!(anchors.len(), 1);
        assert_eq!(
            anchors[0].references,
            vec![
                Reference::Line("internal/transformers/declarations/transform.go".into(), 63),
                Reference::Symbol("DeclarationTransformer".into()),
            ]
        );
    }

    #[test]
    fn a_cited_line_is_checked_without_an_anchor_phrase() {
        // Most line citations are written mid-sentence, with no "Ported from" in
        // front. Requiring the phrase left 27 of them unchecked in `tsr-binder`.
        let source = "\
        // Half is already ported — the message selection (`binder.go:214` block
        // scoped → TS2451), and reporting on every declaration (`binder.go:259`).
        let x = 1;
";
        let anchors = anchors_in(Path::new("x.rs"), source);
        let references: Vec<&Reference> = anchors.iter().flat_map(|a| &a.references).collect();
        assert!(references.contains(&&Reference::Line("binder.go".into(), 214)));
        assert!(references.contains(&&Reference::Line("binder.go".into(), 259)));
    }

    #[test]
    fn prose_without_a_phrase_or_a_path_is_not_an_anchor() {
        // The guard against the `"ports "` and `Upstream's` failures: an ordinary
        // comment full of backticks claims nothing about upstream.
        let source = "\
        // `self.container` is the nearest `HAS_LOCALS` container, and `T` is not.
        let x = 1;
";
        assert!(anchors_in(Path::new("x.rs"), source).is_empty());
    }

    #[test]
    fn coverage_counts_the_doc_comment_directly_above_an_item() {
        let source = "\
/// Ported from typescript-go's `ast.Foo`.
pub fn anchored() {}

/// Does something.
pub fn unanchored() {}

pub struct Bare;
";
        let coverage = coverage_of(source);
        assert_eq!(coverage.items, 3);
        assert_eq!(coverage.anchored, 1);
    }
}
