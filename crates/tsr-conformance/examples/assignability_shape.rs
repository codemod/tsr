//! What shape are the corpus's `TS2322`s, and how many are inside the bound a
//! narrow relater can safely report on?
//!
//! Diagnostic tool, not a suite. It exists to settle the two falsifiers
//! [`docs/adr/0040`] records as blocking, **before** any emitter is written:
//!
//! 1. **Sizing.** Of the cases whose *only* diagnostic code is `TS2322`, how many
//!    are primitive-to-primitive — the `SELECTABLE` domain (`calls.rs:75`) where a
//!    `false` from [`tsr_checker::Checker::is_type_assignable_to`] is trustworthy —
//!    rather than object-to-object, where our structural comparison is narrow and
//!    a `false` would be a *confident wrong diagnostic*?
//! 2. **Message form.** Upstream's `relater.go:4796` switch picks `TS2322` only at
//!    the bottom; the arms above it are separate codes. Does anything but the
//!    generic form ever occur on a primitive-to-primitive comparison?
//!
//! # What it reads, and what it does *not*
//!
//! **Only upstream's `.errors.txt` baselines.** It runs no checker. The type names
//! it classifies are the names *upstream printed*, so the answer is an **upper
//! bound** on what a bounded emitter in this port could convert: our port may
//! compute a different type, or none, at the same position. A small number here is
//! therefore decisive against building; a large one still needs an our-side
//! measurement before it justifies one.
//!
//! That bound is stated rather than hidden because `docs/conventions.md` records
//! two defensible proxies bracketing one such answer by 31×.
//!
//! # Denominators
//!
//! Printed and reconciled against the `diagnostics` suite's own population, per
//! the rule in `docs/conventions.md` ("a probe's denominator must be the
//! gradient's by construction"). The suite's exclusions — configuration-varied
//! baselines, known divergences, cases with no baseline at all, cases expecting no
//! diagnostics — are applied here identically, and the pre-exclusion
//! diagnostic-bearing count is printed beside them so that neither number can be
//! quoted for the other.
//!
//! # The parse
//!
//! An `.errors.txt` header block states one diagnostic per line with a position,
//! and an `errorChain` elaboration follows as *indented continuation lines with no
//! position*. Those continuations are counted, because a chained `TS2322` is
//! indistinguishable from a plain one by code alone — and the code is all
//! [`tsr_conformance::errors_baseline`] keeps.

use std::collections::BTreeMap;

use tsr_conformance::{Corpus, errors_baseline, repo_root};

/// The duplicate-identifier family `bd tsr-8yu` measures the binder's reach with.
const BINDER_CODES: &[u32] = &[2300, 2451, 2393, 2440, 2441, 2396, 2528, 2567];

/// The assignability family: `TS2322` and its four nearest relatives.
const FAMILY: &[u32] = &[2322, 2345, 2739, 2740, 2741];

#[allow(clippy::cast_precision_loss, reason = "a case count is far inside f64's exact range")]
fn pct(n: usize, d: usize) -> f64 {
    if d == 0 { 0.0 } else { n as f64 / d as f64 * 100.0 }
}

/// The printed names of the types `SELECTABLE` admits, in their non-literal form.
///
/// Mirrors `crates/tsr-checker/src/calls.rs:75`: `STRING | STRING_LITERAL |
/// NUMBER | NUMBER_LITERAL | BIG_INT | BIG_INT_LITERAL | BOOLEAN |
/// BOOLEAN_LITERAL | VOID | UNDEFINED | NULL | NEVER`. `true` and `false` are the
/// two `BOOLEAN_LITERAL`s; the remaining literal forms are recognised by shape in
/// [`is_selectable_name`].
///
/// Deliberately absent, and each for the reason `calls.rs` gives: `any`,
/// `unknown`, `symbol`, `unique symbol`, every enum name, and every template
/// literal type.
const SELECTABLE_NAMES: &[&str] = &[
    "string",
    "number",
    "bigint",
    "boolean",
    "void",
    "undefined",
    "null",
    "never",
    "true",
    "false",
];

/// Does this printed type name denote a type inside `SELECTABLE`?
///
/// Conservative by construction: anything unrecognised is *outside*. A name that
/// is wrongly admitted inflates the sizing answer, which is the direction this
/// probe must not err in.
fn is_selectable_name(name: &str) -> bool {
    if SELECTABLE_NAMES.contains(&name) {
        return true;
    }
    // `STRING_LITERAL`: TypeScript prints these with double quotes.
    if name.len() >= 2
        && name.starts_with('"')
        && name.ends_with('"')
        && !name[1..name.len() - 1].contains('"')
    {
        return true;
    }
    // `BIG_INT_LITERAL`, e.g. `123n`, `-1n`.
    if let Some(digits) = name.strip_suffix('n') {
        let digits = digits.strip_prefix('-').unwrap_or(digits);
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            return true;
        }
    }
    // `NUMBER_LITERAL`, e.g. `0`, `-1`, `1.5`. Not `1e3` — upstream normalises the
    // printed form, but admitting an unrecognised shape is the wrong direction.
    let digits = name.strip_prefix('-').unwrap_or(name);
    !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && digits.bytes().any(|b| b.is_ascii_digit())
}

/// One `TS2322` as the baseline states it.
struct Ts2322 {
    line: u32,
    column: u32,
    file: String,
    source: Option<String>,
    target: Option<String>,
    /// The diagnostic carries an `errorChain` elaboration (indented continuation
    /// lines under the header).
    chained: bool,
}

impl Ts2322 {
    /// Both sides inside `SELECTABLE`, and no elaboration — the shape a bounded
    /// emitter could produce.
    fn is_primitive(&self) -> bool {
        match (&self.source, &self.target) {
            (Some(source), Some(target)) => {
                !self.chained && is_selectable_name(source) && is_selectable_name(target)
            }
            _ => false,
        }
    }
}

/// `Type 'A' is not assignable to type 'B'.` → `("A", "B")`.
///
/// Returns `None` for any other text under code 2322, which is how a message form
/// this probe does not know about becomes visible rather than silently
/// mis-classified.
fn parse_generic_message(text: &str) -> Option<(String, String)> {
    let rest = text.strip_prefix("Type '")?;
    let (source, rest) = rest.split_once("' is not assignable to type '")?;
    let target = rest.strip_suffix("'.")?;
    Some((source.to_string(), target.to_string()))
}

/// One parsed header line: file, line, column, code, message text.
type Header = (String, u32, u32, u32, String);

/// Header-block lines, each paired with whether an `errorChain` elaboration
/// follows it — the indented, position-less continuation lines.
fn parse_headers(text: &str) -> Vec<(Header, bool)> {
    let mut out: Vec<(Header, bool)> = Vec::new();
    for line in text.lines() {
        if line.starts_with("==== ") {
            break;
        }
        if let Some(parsed) = parse_header_line(line) {
            out.push((parsed, false));
        } else if !line.trim().is_empty() {
            // A non-empty, non-header line inside the header block is an
            // elaboration belonging to the previous diagnostic.
            if let Some(last) = out.last_mut() {
                last.1 = true;
            }
        }
    }
    out
}

fn parse_header_line(line: &str) -> Option<Header> {
    let (location, rest) = line.split_once("): ")?;
    let (file, position) = location.rsplit_once('(')?;
    let (line_number, column) = position.split_once(',')?;
    let rest = rest.strip_prefix("error TS").or_else(|| rest.strip_prefix("warning TS"))?;
    let (code, text) = rest.split_once(": ")?;
    Some((
        file.to_string(),
        line_number.trim().parse().ok()?,
        column.trim().parse().ok()?,
        code.trim().parse().ok()?,
        text.to_string(),
    ))
}

/// Byte offset of a 1-based (line, UTF-16 column), the inverse of
/// [`tsr_conformance::symbols_baseline::line_and_character`].
fn offset_of(content: &str, line: u32, column: u32) -> Option<usize> {
    let mut base = 0usize;
    for (index, source_line) in content.split_inclusive('\n').enumerate() {
        if u32::try_from(index).ok()? + 1 == line {
            let mut units = 1u32;
            for (byte, ch) in source_line.char_indices() {
                if units == column {
                    return Some(base + byte);
                }
                units += u32::try_from(ch.len_utf16()).ok()?;
            }
            return Some(base + source_line.len());
        }
        base += source_line.len();
    }
    None
}

/// The truncated source text at an offset — the human-readable half of falsifier 3.
fn snippet_at(content: &str, offset: usize) -> String {
    content[offset..].lines().next().unwrap_or("").chars().take(28).collect()
}

/// `parent-kind / node-kind` for the node the diagnostic is anchored to.
///
/// **Why the node kind rather than the source text.** An identifier followed by
/// `:` is a `VariableDeclaration` name *or* a `PropertyAssignment` name, and the
/// two are different check-traversal sites entirely. A text classifier merges
/// them; the first version of this probe did, and reported 38 "annotated
/// declaration names" that were not all declarations.
///
/// The node chosen is the **narrowest** whose span starts exactly at the
/// diagnostic's position — which is what `GetErrorRangeForNode`
/// (`vendor/typescript-go/internal/scanner/scanner.go:2588`) produces, since for a
/// `KindVariableDeclaration` it returns `GetNameOfDeclaration(node)`.
fn anchor_kinds(nodes: &tsr_ast::NodeTable, offset: u32) -> Option<(String, String)> {
    let mut best: Option<tsr_ast::NodeId> = None;
    for index in 0..u32::try_from(nodes.len()).ok()? {
        let id = tsr_ast::NodeId::new(index);
        let span = nodes.span(id);
        if span.start != offset {
            continue;
        }
        if best.is_none_or(|current| span.end < nodes.span(current).end) {
            best = Some(id);
        }
    }
    let id = best?;
    let parent = nodes.parent(id).map_or("<root>".to_string(), |p| format!("{:?}", nodes.kind(p)));
    Some((parent, format!("{:?}", nodes.kind(id))))
}

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    let cases = corpus.discover().expect("discover");

    // Denominators, kept separate so neither can be quoted for the other.
    let mut discovered = 0usize;
    let mut diagnostic_bearing = 0usize; // pre-exclusion: the 7,025 population
    let mut judged = 0usize; // the `diagnostics` suite's population
    let mut judged_with_2322 = 0usize;
    let mut judged_only_2322 = 0usize;
    let mut bearing_only_2322 = 0usize;

    let mut all_2322_diagnostics = 0usize;
    let mut all_2322_primitive = 0usize;
    let mut all_2322_chained = 0usize;
    let mut all_2322_unparsed: BTreeMap<String, usize> = BTreeMap::new();

    // The direct bucket for falsifier 1: TS2322-only cases in which *every*
    // TS2322 is primitive-to-primitive. Nothing less converts a case, because the
    // suite compares the exact multiset.
    let mut fully_primitive_cases = 0usize;
    let mut partly_primitive_cases = 0usize;
    let mut no_primitive_cases = 0usize;
    let mut fully_primitive_diagnostics = 0usize;

    let mut positions: BTreeMap<String, usize> = BTreeMap::new();
    let mut source_literal_generalised = 0usize;
    let mut examples: Vec<String> = Vec::new();
    // Cases whose every TS2322 sits on a `VariableDeclaration` name — the one call
    // site `docs/adr/0040` names, `checkVariableLikeDeclaration`
    // (`checker.go:5899`). This is the level-4 bucket: what *slice one* finishes,
    // as distinct from what the whole bound could ever reach.
    let mut variable_declaration_only_cases = 0usize;

    let mut reachable_syntactic = 0usize;
    let mut reachable_plus_2322 = 0usize;
    let mut reachable_plus_family = 0usize;

    for case in &cases {
        discovered += 1;

        let Ok(baseline) = case.expected_errors() else { continue };
        let Some(baseline) = baseline else { continue };
        let parsed = errors_baseline::parse(&baseline);
        if parsed.is_empty() {
            continue;
        }
        diagnostic_bearing += 1;
        let only_2322_here = parsed.iter().all(|d| d.code == 2322);
        if only_2322_here {
            bearing_only_2322 += 1;
        }

        // The suite's exclusions, in the suite's order.
        if case.has_varied_errors() || case.has_known_divergence() || !case.has_any_baseline() {
            continue;
        }
        judged += 1;
        let syntactic = |code: u32| code < 2000 || BINDER_CODES.contains(&code);
        if parsed.iter().all(|d| syntactic(d.code)) {
            reachable_syntactic += 1;
        }
        if parsed.iter().all(|d| syntactic(d.code) || d.code == 2322) {
            reachable_plus_2322 += 1;
        }
        if parsed.iter().all(|d| syntactic(d.code) || FAMILY.contains(&d.code)) {
            reachable_plus_family += 1;
        }
        if !parsed.iter().any(|d| d.code == 2322) {
            continue;
        }
        judged_with_2322 += 1;

        // Collect the TS2322s with their text.
        let mut here: Vec<Ts2322> = Vec::new();
        for ((file, line, column, code, text), chained) in parse_headers(&baseline) {
            if code != 2322 {
                continue;
            }
            let (source, target) = if let Some((source, target)) = parse_generic_message(&text) {
                (Some(source), Some(target))
            } else {
                *all_2322_unparsed.entry(text.clone()).or_default() += 1;
                (None, None)
            };
            here.push(Ts2322 { line, column, file, source, target, chained });
        }

        all_2322_diagnostics += here.len();
        all_2322_chained += here.iter().filter(|d| d.chained).count();
        all_2322_primitive += here.iter().filter(|d| d.is_primitive()).count();

        if !only_2322_here {
            continue;
        }
        judged_only_2322 += 1;

        let primitive = here.iter().filter(|d| d.is_primitive()).count();
        if primitive == here.len() && !here.is_empty() {
            fully_primitive_cases += 1;
            fully_primitive_diagnostics += here.len();

            // Falsifier 3, and the generalisation question, only for the
            // population a bounded emitter would actually touch.
            let loaded = case.load().ok();
            let mut all_variable_declarations = true;
            for diagnostic in &here {
                if diagnostic.source.as_deref().is_some_and(|s| !SELECTABLE_NAMES.contains(&s)) {
                    source_literal_generalised += 1;
                }
                let unit = loaded.as_ref().and_then(|test| {
                    test.files.iter().find(|f| {
                        f.name.trim_start_matches('/') == diagnostic.file.trim_start_matches('/')
                            || f.name.ends_with(diagnostic.file.trim_start_matches('/'))
                    })
                });
                let located = unit.and_then(|unit| {
                    let offset = offset_of(&unit.content, diagnostic.line, diagnostic.column)?;
                    let kind = tsr_parser::ScriptKind::from_file_name(&unit.name);
                    if kind == tsr_parser::ScriptKind::Json {
                        return None;
                    }
                    // Parents are set by the binder, and the parent kind is what
                    // separates a declaration name from an object-literal key.
                    let parsed =
                        tsr_parser::ParsedFile::parse_with_script_kind(unit.content.clone(), kind);
                    parsed.with_ast(|file| {
                        let _bound = tsr_binder::bind(
                            file,
                            parsed.nodes(),
                            tsr_binder::FileInfo { name: &unit.name, text: parsed.source() },
                        );
                    });
                    let kinds = anchor_kinds(parsed.nodes(), u32::try_from(offset).ok()?)?;
                    Some((kinds, snippet_at(&unit.content, offset)))
                });
                if let Some(((parent, node), snippet)) = located {
                    if parent != "VariableDeclaration" {
                        all_variable_declarations = false;
                    }
                    *positions.entry(format!("{parent} / {node}")).or_default() += 1;
                    if examples.len() < 30 {
                        examples.push(format!(
                            "  {:<44}  {:<30}  '{}' -> '{}'",
                            format!("{parent}/{node}"),
                            snippet.replace('\t', " "),
                            diagnostic.source.as_deref().unwrap_or("?"),
                            diagnostic.target.as_deref().unwrap_or("?"),
                        ));
                    }
                } else {
                    all_variable_declarations = false;
                    *positions.entry("unlocatable".to_string()).or_default() += 1;
                }
            }
            if all_variable_declarations {
                variable_declaration_only_cases += 1;
            }
        } else if primitive > 0 {
            partly_primitive_cases += 1;
        } else {
            no_primitive_cases += 1;
        }
    }

    println!("== denominators ==");
    println!("  cases discovered                                {discovered:>6}");
    println!(
        "  with >=1 diagnostic in the baseline header      {diagnostic_bearing:>6}   (the 7,025 population)"
    );
    println!(
        "    of those, TS2322-only                         {bearing_only_2322:>6}   (the 516)"
    );
    println!(
        "  judged by the `diagnostics` suite               {judged:>6}   (the 5,488 population)"
    );
    println!("    carrying >=1 TS2322                           {judged_with_2322:>6}");
    println!("    TS2322-only                                   {judged_only_2322:>6}");

    println!();
    println!("== the reachability table, over the suite's own denominator ({judged}) ==");
    let share = |n: usize| pct(n, judged);
    println!(
        "  scanner + parser + binder only          {reachable_syntactic:>6}   {:.2}%",
        share(reachable_syntactic)
    );
    println!(
        "  + TS2322                                {reachable_plus_2322:>6}   {:.2}%   (+{})",
        share(reachable_plus_2322),
        reachable_plus_2322 - reachable_syntactic
    );
    println!(
        "  + assignability family                  {reachable_plus_family:>6}   {:.2}%   (+{})",
        share(reachable_plus_family),
        reachable_plus_family - reachable_syntactic
    );

    println!();
    println!("== every TS2322 in the judged population ==");
    println!("  diagnostics                                     {all_2322_diagnostics:>6}");
    println!("  with an errorChain elaboration                  {all_2322_chained:>6}");
    println!("  primitive -> primitive, unchained (SELECTABLE)   {all_2322_primitive:>6}");
    println!(
        "  header text that is not the generic message      {:>6}",
        all_2322_unparsed.values().sum::<usize>()
    );
    for (text, count) in all_2322_unparsed.iter().take(15) {
        println!("      {count:>4}  {text}");
    }

    println!();
    println!("== falsifier 1: TS2322-only cases, by shape ==");
    println!(
        "  every TS2322 primitive -> primitive              {fully_primitive_cases:>6}   <- THE DIRECT BUCKET"
    );
    println!("    the diagnostics in them                       {fully_primitive_diagnostics:>6}");
    println!("  some primitive, some not                        {partly_primitive_cases:>6}");
    println!("  none primitive                                  {no_primitive_cases:>6}");
    println!(
        "  ...of the fully-primitive, every TS2322 on a\n  VariableDeclaration name (slice one)            {variable_declaration_only_cases:>6}   <- WHAT SLICE ONE FINISHES"
    );

    println!();
    println!("== falsifier 3: what the position points at, in the convertible cases ==");
    for (shape, count) in &positions {
        println!("  {count:>6}  {shape}");
    }
    println!(
        "  source printed as a literal (generalisation not applied)  {source_literal_generalised:>6}"
    );

    println!();
    println!("== examples ==");
    for example in &examples {
        println!("{example}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_types_selectable_admits_are_admitted() {
        // Every arm of `SELECTABLE` (`crates/tsr-checker/src/calls.rs:75`) in its
        // printed form.
        for name in [
            "string",
            "number",
            "bigint",
            "boolean",
            "void",
            "undefined",
            "null",
            "never",
            "true",
            "false",
            "\"a\"",
            "0",
            "-1",
            "1.5",
            "10n",
        ] {
            assert!(is_selectable_name(name), "{name} should be inside SELECTABLE");
        }
    }

    #[test]
    fn an_unrecognised_name_is_outside_the_bound() {
        // The direction that matters: a wrongly-admitted name *inflates* the
        // sizing answer, which is the one error this probe must not make. `any`
        // and `unknown` are not in SELECTABLE at all; the rest are object types
        // whose `false` from a narrow relater is untrustworthy.
        for name in [
            "any",
            "unknown",
            "symbol",
            "{ a: number; }",
            "number[]",
            "string | number",
            "Foo",
            "() => void",
            "E.A",
            "`a${string}`",
        ] {
            assert!(!is_selectable_name(name), "{name} should be outside SELECTABLE");
        }
    }

    #[test]
    fn the_generic_message_yields_both_type_names() {
        assert_eq!(
            parse_generic_message("Type '\"a\"' is not assignable to type 'number'."),
            Some(("\"a\"".to_string(), "number".to_string()))
        );
        // Any other text under code 2322 must be surfaced, not mis-classified.
        assert_eq!(parse_generic_message("Type 'A' is not comparable to type 'B'."), None);
    }

    #[test]
    fn a_continuation_line_marks_the_previous_diagnostic_as_chained() {
        // An `errorChain` elaboration is printed with no position, so it can only
        // be attributed to the header line above it.
        let text = "a.ts(1,1): error TS2322: Type 'A' is not assignable to type 'B'.\n  \
                    Types of property 'x' are incompatible.\n\
                    ==== a.ts ====\n";
        let parsed = parse_headers(text);
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].1, "the elaboration should mark the diagnostic chained");
    }

    #[test]
    fn an_offset_round_trips_through_line_and_column() {
        let content = "let a = 1;\nlet bb = 2;\n";
        assert_eq!(offset_of(content, 2, 5), Some(15));
        assert_eq!(snippet_at(content, 15), "bb = 2;");
    }
}
