//! Verify that every internal `§N` citation resolves to a recorded section —
//! `bd tsr-5`.
//!
//! # Why this exists
//!
//! `cargo xtask anchors` resolves upstream `file:line` references and
//! `cargo xtask issue-ids` resolves `bd` ids. The third citation class this
//! repository writes by hand — the project's own `§N` section numbers — had no
//! gate, and §267 (`checker-notes-diag2.md`) is what it cost: a code comment
//! declined a construct citing "§38's second fired leg", the citation pointed at
//! no witness, and the decline was wrong against four committed upstream
//! baselines.
//!
//! # What counts as a section
//!
//! A section is **defined** by a line in one of the record files —
//! `docs/architecture/checker-notes-*.md` and `STATUS.md` — that is either:
//!
//! - a Markdown heading: every `§N` token in it, any `§N–§M` range in it, and a
//!   bare leading number (`## 5.`, `### 6.1`), which is how several notes files
//!   number their sections and how other files then cite them; or
//! - a paragraph that opens in bold with a section number
//!   (`**§90 + §90.1 score — LANDED.**`), which is how scores and sub-items are
//!   written throughout the notes. Every `§N` inside that leading bold span
//!   counts.
//!
//! `STATUS.md` is a record file because it is where refusals live with their
//! numbers (`### §647 — …` is a heading there and nowhere else). The rest of
//! `docs/` is not: `conventions.md`, the ADRs and the `checker-9x-*` notes cite
//! sections but do not number their own.
//!
//! # What counts as a citation, and how it resolves
//!
//! Every `§N` / `§N.M…` in `crates/` (`.rs`, `.md`) and `docs/` (`.md`).
//!
//! - **Qualified by a record file** — written immediately after
//!   `checker-notes-x.md` (`checker-notes-diag2.md §299, §300`) — it must
//!   resolve **in that file**. This is the strongest check the gate makes.
//! - **Qualified by another document** (`STATUS-cli.md §7.10`, `PLAN.md §3.5`) or
//!   by the TypeScript spec (`1.0 spec §3.4.1`, where the spec may sit at the end
//!   of the previous line): not this gate's business, counted and skipped.
//! - **Unqualified**: it must resolve in **some** record file.
//!
//! `§N.M` that is not itself defined resolves to its parent `§N` when that is
//! defined, because sub-items are routinely discussed inside their parent's
//! section without a heading of their own (`§839.1` is a paragraph of `§839`).
//!
//! # What it deliberately does not prove
//!
//! **Section numbers are not unique.** The diagnostics and checker workstreams
//! numbered in parallel, so `§93` is a heading in both `checker-notes-diag2.md`
//! and `checker-notes-narrow.md`, and an unqualified `§93` resolves if either
//! exists. The gate proves a citation lands *somewhere*; like `issue-ids`, it
//! cannot prove it lands where its author meant. §267's original "§38" would
//! **still pass** — `§38` is a heading in two files — which is the honest limit:
//! the only cure for a wrong-but-existing citation is reading the section it
//! names. Qualifying a citation with its file is what upgrades it to a real
//! check.
//!
//! # Sections that exist only outside the notes
//!
//! Some numbered steps were recorded only in a commit message and a `STATUS.md`
//! §7 row, never as a section. Rewriting every citation of them would destroy
//! the provenance they carry, so they are listed in [`RECORDED`] with where the
//! record actually lives. The gate fails if an entry there stops being needed,
//! so the list cannot silently outgrow its reason.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Section numbers cited in the tree that have no section in any record file,
/// with where their record lives instead. Each was checked by hand against
/// `git log` when it was added.
const RECORDED: &[(&str, &str)] = &[
    (
        "203",
        "checker-1's §203: commits e0fbc77f (array elision is `undefined`) and 2dd0f947 (flow \
         join is a subtype-reduction site); STATUS.md §5 `array elision` row and the 2026-08-11 \
         §7 row. No notes section was written.",
    ),
    (
        "284",
        "two parallel §284s, neither with a notes section: diagnostics' 6392c902 (`xtask measure` \
         prints its diff) and checker 4d5e2e46 (the custom-iterator element).",
    ),
    (
        "300",
        "two parallel §300s, neither with a notes section: diagnostics' 4178b944 (`diagpair`) \
         and checker d4db4617 (imports from a shorthand ambient module are `any`).",
    ),
];

/// Files that *define* sections. See the module docs.
fn is_record_file(name: &str) -> bool {
    name == "STATUS.md" || (name.starts_with("checker-notes-") && is_markdown(name))
}

fn is_markdown(name: &str) -> bool {
    Path::new(name).extension().is_some_and(|e| e.eq_ignore_ascii_case("md"))
}

/// A section number as written: `93`, `6.1`, `3.4.1`.
fn number_at(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut end = 0;
    loop {
        let start = end;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end == start {
            // A dot not followed by a digit is punctuation, not a component.
            return (start > 0).then(|| &text[..start - 1]);
        }
        if end + 1 < bytes.len() && bytes[end] == b'.' && bytes[end + 1].is_ascii_digit() {
            end += 1;
        } else {
            return Some(&text[..end]);
        }
    }
}

/// Every `§N` in `line`: byte offset of the `§` and the number. A `§` glued to a
/// word (`fixpoint-patch-§12.md`) is part of a file name, not a citation.
fn citations(line: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    for (at, _) in line.match_indices('§') {
        let glued = line[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_');
        if glued {
            continue;
        }
        if let Some(number) = number_at(&line[at + '§'.len_utf8()..]) {
            out.push((at, number));
        }
    }
    out
}

/// The sections one line of a record file defines.
fn definitions(line: &str) -> Vec<String> {
    let trimmed = line.trim_start();
    let mut out = Vec::new();
    if let Some(heading) = trimmed.strip_prefix('#') {
        let heading = heading.trim_start_matches('#');
        if !heading.starts_with([' ', '\t']) {
            return out;
        }
        let heading = heading.trim_start();
        // `## 5.` / `### 6.1 Title`
        if let Some(number) = number_at(heading) {
            let rest = &heading[number.len()..];
            if rest.is_empty() || rest.starts_with(['.', ' ', ':', ')', '—', '–', '-', '/']) {
                out.push(number.to_string());
            }
        }
        let cited = citations(heading);
        out.extend(cited.iter().map(|(_, n)| (*n).to_string()));
        // `§591–§595` defines the numbers between, too.
        for pair in cited.windows(2) {
            let (a_at, a) = pair[0];
            let (b_at, b) = pair[1];
            let between = heading[a_at + '§'.len_utf8() + a.len()..b_at].trim();
            if matches!(between, "–" | "-")
                && let (Ok(a), Ok(b)) = (a.parse::<u32>(), b.parse::<u32>())
                && a < b
                && b - a <= 50
            {
                out.extend((a + 1..b).map(|n| n.to_string()));
            }
        }
        return out;
    }
    let body = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("> ")).unwrap_or(trimmed);
    if let Some(bold) = body.strip_prefix("**")
        && bold.starts_with('§')
    {
        let span = bold.find("**").map_or(bold, |end| &bold[..end]);
        out.extend(citations(span).into_iter().map(|(_, n)| n.to_string()));
    }
    out
}

/// What a citation is qualified by, judged from the text before it on its line.
enum Qualifier<'a> {
    None,
    RecordFile(&'a str),
    Elsewhere,
}

/// `checker-notes-x.md` immediately before the citation — allowing a closing
/// backtick, a Markdown link target, `'s`, and earlier members of the same list
/// (`§299, §300`, `§610–§612`).
fn qualifier<'a>(before: &'a str, previous_line: &str) -> Qualifier<'a> {
    let mut rest = before.trim_end();
    loop {
        // Peel one earlier list member: `§299,` / `§610–` / `§77/`.
        let peeled = rest.trim_end_matches([',', '/', '–', '-', '&']).trim_end();
        let peeled = peeled.strip_suffix(" and").unwrap_or(peeled).trim_end();
        let Some(at) = peeled.rfind('§') else { break };
        let tail = &peeled[at + '§'.len_utf8()..];
        let Some(number) = number_at(tail) else { break };
        let suffix = &tail[number.len()..];
        if !suffix.chars().all(|c| c.is_ascii_lowercase()) || suffix.len() > 1 {
            break;
        }
        rest = peeled[..at].trim_end();
    }
    let rest = rest.strip_suffix("'s").unwrap_or(rest);
    // A Markdown link names its file in the target: `[text](path.md#frag)`.
    let rest = match (rest.strip_suffix(')'), rest.rfind("](")) {
        (Some(inner), Some(at)) => {
            let target = &inner[at + 2..];
            target.split_once('#').map_or(target, |(file, _)| file)
        }
        _ => rest,
    };
    let rest = rest.trim_end_matches(['`', ']', '*']);
    if is_markdown(rest) {
        let start = rest
            .rfind(|c: char| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '§')))
            .map_or(0, |i| i + rest[i..].chars().next().map_or(1, char::len_utf8));
        let name = &rest[start..];
        return if is_record_file(name) && name != "STATUS.md" {
            Qualifier::RecordFile(name)
        } else {
            Qualifier::Elsewhere
        };
    }
    // The TypeScript spec, on this line or wrapped from the end of the last.
    let near = |text: &str| {
        let lower = text.to_ascii_lowercase();
        lower.trim_end().ends_with("spec") || lower.ends_with("spec)") || {
            let tail: String =
                lower.chars().rev().take(48).collect::<Vec<_>>().into_iter().rev().collect();
            tail.contains(" spec ") || tail.contains("-spec ") || tail.contains("spec (")
        }
    };
    // A continuation line holds nothing before the citation but comment markers.
    let continuation =
        before.trim_start().trim_start_matches(['/', '!', '*', '>', '-']).trim().is_empty();
    if near(before) || (continuation && near(previous_line)) {
        return Qualifier::Elsewhere;
    }
    Qualifier::None
}

/// Resolve `number` against `defined`, falling back through its parents.
fn resolves(number: &str, defined: &BTreeSet<String>) -> bool {
    let mut current = number;
    loop {
        if defined.contains(current) {
            return true;
        }
        match current.rsplit_once('.') {
            Some((parent, _)) => current = parent,
            None => return false,
        }
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, extensions: &[&str]) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out, extensions);
        } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| extensions.contains(&e))
        {
            out.push(path);
        }
    }
}

/// Check every citation, failing on danglers.
pub fn run(root: &Path) -> Result<()> {
    // Definitions, per record file.
    let mut defined_in: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut record_files = vec![root.join("STATUS.md")];
    walk(&root.join("docs/architecture"), &mut record_files, &["md"]);
    for path in &record_files {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        if !is_record_file(&name) {
            continue;
        }
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let set = defined_in.entry(name).or_default();
        for line in text.lines() {
            set.extend(definitions(line));
        }
    }
    let defined: BTreeSet<String> = defined_in.values().flatten().cloned().collect();
    if defined.is_empty() {
        bail!("no sections defined under {} — the record files moved?", root.display());
    }

    let mut files = Vec::new();
    walk(&root.join("crates"), &mut files, &["rs", "md"]);
    walk(&root.join("docs"), &mut files, &["md"]);
    files.sort();

    let recorded: BTreeMap<&str, &str> = RECORDED.iter().copied().collect();
    let mut used_records: BTreeSet<&str> = BTreeSet::new();
    let (mut total, mut skipped, mut file_checked) = (0usize, 0usize, 0usize);
    let mut dangling: Vec<String> = Vec::new();
    for path in &files {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let display = path.strip_prefix(root).unwrap_or(path).display().to_string();
        let mut previous = "";
        for (index, line) in text.lines().enumerate() {
            for (at, number) in citations(line) {
                total += 1;
                let ok = match qualifier(&line[..at], previous) {
                    Qualifier::Elsewhere => {
                        skipped += 1;
                        continue;
                    }
                    Qualifier::RecordFile(file) => {
                        file_checked += 1;
                        defined_in.get(file).is_some_and(|set| resolves(number, set))
                    }
                    Qualifier::None => resolves(number, &defined),
                };
                if ok {
                    continue;
                }
                let top = number.split('.').next().unwrap_or(number);
                if let Some((&key, _)) =
                    recorded.get_key_value(number).or(recorded.get_key_value(top))
                {
                    used_records.insert(key);
                    continue;
                }
                dangling.push(format!("{display}:{}: §{number}    {}", index + 1, line.trim()));
            }
            previous = line;
        }
    }

    let stale: Vec<&str> =
        recorded.keys().copied().filter(|key| !used_records.contains(key)).collect();
    println!(
        "{total} §-citation(s) in crates/ and docs/: {file_checked} checked against their named \
         notes file, {skipped} skipped as citing another document or the spec, {} resolved \
         through RECORDED, {} dangling; {} section number(s) defined across {} record file(s)",
        used_records.len(),
        dangling.len(),
        defined.len(),
        defined_in.len()
    );
    for line in &dangling {
        eprintln!("dangling {line}");
    }
    for key in &stale {
        eprintln!("RECORDED entry §{key} is no longer needed — remove it");
    }
    if !dangling.is_empty() || !stale.is_empty() {
        bail!(
            "{} dangling §-citation(s), {} stale RECORDED entr(ies). A citation nobody can follow \
             is worse than no citation, because it reads as provenance. Fix the number, qualify \
             it with the document it means, or — if the record lives only in a commit — add it \
             to RECORDED in xtask/src/sections.rs with where it lives.",
            dangling.len(),
            stale.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_stop_at_sentence_punctuation() {
        assert_eq!(
            citations("see §93. Then §6.1, and §3.4.1."),
            vec![(4, "93"), (15, "6.1"), (26, "3.4.1")]
        );
        assert_eq!(citations("fixpoint-patch-§12.md"), Vec::new());
        assert_eq!(citations("§ alone"), Vec::new());
    }

    #[test]
    fn headings_and_bold_leads_define_sections() {
        assert_eq!(definitions("## §609 CORRECTED BY §610 — x"), vec!["609", "610"]);
        assert_eq!(definitions("### 6.1 Title"), vec!["6.1"]);
        assert_eq!(definitions("## 8. Spin-off items"), vec!["8"]);
        assert_eq!(
            definitions("### 4.-2c THE FORM SEAM (§591–§593)"),
            vec!["4", "591", "593", "592"]
        );
        assert_eq!(definitions("**§90 + §90.1 score — LANDED.** right §89"), vec!["90", "90.1"]);
        assert_eq!(definitions("right **§89** here"), Vec::<String>::new());
        assert_eq!(definitions("#hashtag §5"), Vec::<String>::new());
    }

    #[test]
    fn qualifiers_are_read_from_the_text_just_before() {
        assert!(matches!(
            qualifier("`docs/architecture/checker-notes-diag2.md` §299, ", ""),
            Qualifier::RecordFile("checker-notes-diag2.md")
        ));
        assert!(matches!(
            qualifier("[x](checker-notes-narrow.md)'s ", ""),
            Qualifier::RecordFile("checker-notes-narrow.md")
        ));
        assert!(matches!(qualifier("(`STATUS-cli.md` ", ""), Qualifier::Elsewhere));
        assert!(matches!(qualifier("STATUS.md ", ""), Qualifier::Elsewhere));
        assert!(matches!(qualifier("TypeScript 1.0 spec (April 2014) ", ""), Qualifier::Elsewhere));
        assert!(matches!(
            qualifier("// ", "// callee is untyped. TS 1.0 spec"),
            Qualifier::Elsewhere
        ));
        // A file named earlier in the sentence does not qualify a later citation.
        assert!(matches!(
            qualifier("(`checker-notes-ctx.md`'s argument); the ", ""),
            Qualifier::None
        ));
    }

    #[test]
    fn a_sub_item_resolves_to_its_parent() {
        let defined: BTreeSet<String> = ["839".to_string()].into();
        assert!(resolves("839.1", &defined));
        assert!(!resolves("840", &defined));
    }
}
