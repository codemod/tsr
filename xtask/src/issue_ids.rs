//! Verify that every `bd` issue id cited in the repository actually exists.
//!
//! # Why this exists
//!
//! `cargo xtask anchors` resolves every upstream `file:line` reference, and
//! `docs/conventions.md` records why: **a citation nobody can follow is worse
//! than no citation, because it reads as provenance.** `bd` issue ids are the
//! same class of reference — a pointer into a store outside the source tree,
//! written by hand, quoted forward into other documents — and until now they had
//! no gate at all.
//!
//! They needed one. In a single cycle, **four fabricated ids were caught**, every
//! one by a person remembering to `grep` the document *after* running
//! `bd create`:
//!
//! - `tsr-6yg`, quoted in two handoffs and a document; the real id was `tsr-lgf`.
//!   It was produced by `bd create … | tail -2`, which prints the priority and
//!   status lines and cuts the line carrying the new id. A teammate found nothing
//!   behind the citation and refiled the incident, which produced a duplicate —
//!   the better of the two errors.
//! - `tsr-r4v` and `tsr-8mb`, both written into `docs/` by me *before* the
//!   issues were created, in commits that quoted the rule against doing exactly
//!   that.
//! - `tsr-ozy`, caught by its author before commit; the real id was `tsr-n22`.
//!
//! Four catches by discipline is four more than the gate caught, and it is the
//! wrong number of times to rely on remembering. The agent that caught the last
//! one put it plainly: *`xtask anchors` already resolves upstream `file:line`;
//! `bd` ids in `docs/` are the same class of reference with no gate behind them.*
//!
//! # What it checks, and what it deliberately does not
//!
//! Every token written in the **`bd <id>` form** — the citation convention this
//! repository actually uses — is looked up. The scan covers `docs/`, `PLAN.md`,
//! `README.md` and `CLAUDE.md` — prose, where a dangling id misleads a reader.
//!
//! **A bare `tsr-xxx` mention is deliberately not checked**, and that is a real
//! gap accepted for a reason the first run supplied: scanning every token flagged
//! seven entries in `PLAN.md`'s *crate table* (`tsr-lsp`, `tsr-napi`,
//! `tsr-execute` …), which are planned crates and are indistinguishable from an
//! issue id by shape — `tsr-lsp` is exactly as long as `tsr-v1j`. It also flagged
//! the one place this document quotes a **deliberately fabricated** id as its
//! worked example, which must stay unresolvable. Requiring the `bd ` prefix
//! removed all eight false positives without a hand-maintained exclusion list,
//! and kept both genuine catches. Source comments are **not** scanned: they cite
//! ids too, but they are also where an id may legitimately name an issue that was
//! closed and archived, and a gate that fires on correct history teaches people
//! to disable it.
//!
//! **This gate proves an id exists. It does not prove the id is the one you
//! meant** — exactly the limit `anchors` has, recorded in `docs/conventions.md`
//! under *"an anchor that resolves and points at the wrong construct is worse
//! than one that fails"*. A wrong-but-existing id stays invisible here, and the
//! only thing that catches it is reading the line that names the thing you cite.
//!
//! # When `bd` is not available
//!
//! It **reports and skips, loudly, and never silently passes**. That is
//! `tsr-conformance`'s own rule — *a stage that does not exist reports 0%,
//! loudly* — applied to a gate: a check that quietly succeeds when it could not
//! run is worse than no check, because the green tick is read as evidence.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Files whose prose is scanned. Source comments are excluded on purpose; see
/// the module docs.
const ROOTS: [&str; 4] = ["docs", "PLAN.md", "README.md", "CLAUDE.md"];

/// Where an id was cited, so a failure names the line rather than the id alone.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Citation {
    file: String,
    line: usize,
}

/// Every `tsr-…` token in `text`, with the 1-based line it sits on.
///
/// Deliberately permissive about what follows the prefix and strict about what
/// precedes it: an id is `tsr-` plus lowercase alphanumerics, and it must not be
/// preceded by an identifier character, so `tsr-checker` in a crate path is
/// rejected by the suffix rule and `xtsr-abc` by the prefix rule.
///
/// **Crate names are the reason this needs care.** This workspace is full of
/// `tsr-parser`, `tsr-conformance`, `tsr-ast` — tokens that match the shape of an
/// issue id exactly. They are excluded by [`is_crate_name`] rather than by a
/// length rule, because `bd` ids are three characters today and nothing promises
/// they stay three.
fn scan(text: &str) -> Vec<(String, usize)> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        // `cursor` is an offset into `line`, always. The first version of this
        // function kept a separate `rest` slice and re-sliced `line` with an
        // offset taken from `rest` -- which does not advance once past the first
        // match, and hangs. One index, one meaning.
        let mut cursor = 0usize;
        while let Some(at) = line[cursor..].find("tsr-") {
            let start = cursor + at;
            let after_prefix = start + 4;
            let tail = &line[after_prefix..];
            let len = tail
                .find(|c: char| !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '.')
                .unwrap_or(tail.len());
            let id = format!("tsr-{}", tail[..len].trim_end_matches('.'));
            // **Only the `bd <id>` form counts as a citation**, and the first
            // run is what forced that. Scanning every `tsr-…` token flagged
            // seven entries in `PLAN.md`'s crate table -- `tsr-lsp`,
            // `tsr-napi`, `tsr-execute` -- which are *planned crates*, not
            // issues, and are indistinguishable from an id by shape alone
            // (`tsr-lsp` is exactly as long as `tsr-v1j`). It also flagged the
            // one place `conventions.md` quotes a **deliberately fabricated**
            // id as its worked example, which must stay unresolvable.
            //
            // Requiring the `bd ` prefix removed all eight without a
            // hand-maintained exclusion list, and kept both genuine catches.
            // The cost is stated in the module docs: a bare `tsr-xxx` mention
            // is not checked.
            let preceded_by_bd = line[..start].trim_end().ends_with("bd");
            if preceded_by_bd && id.len() > 4 && !is_crate_name(&id) {
                found.push((id, index + 1));
            }
            // Always advance past the prefix, so a zero-length suffix cannot
            // stall the loop.
            cursor = after_prefix + len;
        }
    }
    found
}

/// Whether a `tsr-…` token names a crate in this workspace rather than an issue.
///
/// Read from the filesystem rather than hard-coded, so a new crate does not
/// start failing this gate the day it lands.
fn is_crate_name(id: &str) -> bool {
    CRATES.with(|crates| crates.contains(id))
}

thread_local! {
    static CRATES: BTreeSet<String> = crate_names();
}

fn crate_names() -> BTreeSet<String> {
    let root = super::workspace_root().join("crates");
    let Ok(entries) = std::fs::read_dir(root) else { return BTreeSet::new() };
    entries
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with("tsr-"))
        .collect()
}

/// Every scanned file under `root`.
fn files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in ROOTS {
        let path = root.join(entry);
        if path.is_file() {
            out.push(path);
        } else if path.is_dir() {
            walk(&path, &mut out);
        }
    }
    out.sort();
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

/// Every issue id `bd` knows about, in **one** call.
///
/// `bd show <id>` per citation was the obvious shape and it is unusable: this
/// repository cites ~200 distinct ids, and spawning a process for each took
/// longer than the entire conformance corpus run. `bd list --all --json` answers
/// the same question once.
///
/// `--all` is load-bearing — without it `bd list` filters closed issues, and a
/// citation of a *closed* issue is a perfectly good citation. A gate that fired
/// on correctly-cited finished work would be trained away within a day.
///
/// `None` means `bd` could not be run at all, which is reported and skipped
/// rather than treated as "no ids exist".
fn known_ids() -> Option<BTreeSet<String>> {
    let out = Command::new("bd").args(["list", "--all", "--json"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&text).ok()?;
    let ids: BTreeSet<String> = parsed
        .as_array()?
        .iter()
        .filter_map(|issue| issue.get("id")?.as_str().map(str::to_string))
        .collect();
    // An empty list from a successful call is indistinguishable here from a
    // store that failed to open, and treating it as "nothing exists" would fail
    // every citation in the repository at once. Refuse to use it.
    if ids.is_empty() { None } else { Some(ids) }
}

/// Check every cited id, or say loudly that the check did not run.
pub fn run(root: &Path) -> Result<()> {
    let files = files(root);
    if files.is_empty() {
        bail!("no documentation files found under {}", root.display());
    }

    let mut citations: BTreeMap<String, Vec<Citation>> = BTreeMap::new();
    for path in &files {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let display = path.strip_prefix(root).unwrap_or(path).display().to_string();
        for (id, line) in scan(&text) {
            citations.entry(id).or_default().push(Citation { file: display.clone(), line });
        }
    }

    let total: usize = citations.values().map(Vec::len).sum();

    let Some(known) = known_ids() else {
        // Loudly, and not as a pass. See the module docs.
        println!(
            "SKIPPED: `bd list --all --json` did not answer, so {total} issue citation(s) \
             across {} id(s) were NOT checked. This is not a pass.",
            citations.len()
        );
        return Ok(());
    };

    let mut dangling: Vec<(&String, &Vec<Citation>)> = Vec::new();
    for (id, where_cited) in &citations {
        // A sub-issue id (`tsr-4sc.11`) resolves if `bd` knows it directly; some
        // stores list only the parent, so the parent is accepted as a fallback
        // rather than reporting a real sub-issue as dangling.
        let parent = id.split_once('.').map(|(head, _)| head.to_string());
        let ok = known.contains(id) || parent.is_some_and(|p| known.contains(&p));
        if !ok {
            dangling.push((id, where_cited));
        }
    }

    if dangling.is_empty() {
        println!(
            "{total} issue citation(s) across {} distinct id(s) checked, 0 dangling",
            citations.len()
        );
        return Ok(());
    }

    for (id, where_cited) in &dangling {
        eprintln!("dangling issue id {id}:");
        for citation in *where_cited {
            eprintln!("    {}:{}", citation.file, citation.line);
        }
    }
    bail!(
        "{} issue id(s) cited in documentation do not exist. A citation nobody can follow is \
         worse than no citation, because it reads as provenance.",
        dangling.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_finds_ids_in_the_bd_form_including_inside_backticks() {
        let text = "See `bd tsr-4sc` and bd tsr-zlo.\nAlso bd tsr-6ph on line two.";
        assert_eq!(
            scan(text),
            vec![
                ("tsr-4sc".to_string(), 1),
                ("tsr-zlo".to_string(), 1),
                ("tsr-6ph".to_string(), 2),
            ]
        );
    }

    /// Sub-issue ids carry a dot, and dropping it would look up a different
    /// issue that exists — the silent-wrong-answer case this gate cannot
    /// otherwise see.
    #[test]
    fn a_sub_issue_id_keeps_its_suffix() {
        assert_eq!(scan("blocked on bd tsr-4sc.11 today"), vec![("tsr-4sc.11".to_string(), 1)]);
    }

    /// A trailing sentence period is not part of the id.
    #[test]
    fn a_trailing_period_is_not_part_of_the_id() {
        assert_eq!(scan("closed by bd tsr-v1j."), vec![("tsr-v1j".to_string(), 1)]);
    }

    /// **The first run's false positives, pinned.** `PLAN.md`'s crate table
    /// lists planned crates whose names are shaped exactly like issue ids, and
    /// `conventions.md` quotes a deliberately fabricated id as a worked example
    /// that must stay unresolvable. Neither is written in the `bd` form.
    #[test]
    fn a_bare_mention_is_not_a_citation() {
        assert_eq!(scan("tsr-lsp       LSP server"), Vec::new());
        assert_eq!(scan("tsr-napi      Node bindings (napi-rs)"), Vec::new());
        assert_eq!(scan("`tsr-6yg`. A teammate found nothing behind the citation"), Vec::new());
        assert_eq!(scan("crates/tsr-checker/src is a path"), Vec::new());
    }

    /// The exclusion is by citation form, so a real crate name written as a
    /// citation would still be checked — and should be, because that is a
    /// mistake rather than a false positive.
    #[test]
    fn the_bd_form_is_what_makes_it_a_citation() {
        assert_eq!(scan("see bd tsr-checker"), Vec::new(), "a crate name is still excluded");
        assert_eq!(scan("see bd tsr-abc"), vec![("tsr-abc".to_string(), 1)]);
    }
}
