//! `printer_round_trip`: parse → print → reparse → compare trees.
//!
//! # A gate that needs no baselines
//!
//! The printer is correct when the *tree* survives being printed and reparsed. So
//! the denominator is the whole corpus rather than the cases upstream happened to
//! record emit output for, and the suite can only sit at 100% — the same shape as
//! `scanner_termination`, and for the same reason: the property is intrinsic, not
//! compared against anything external.
//!
//! Formatting is deliberately outside the property. Quote style, spacing and line
//! breaks are not in the tree, so the printer is free to choose them. Phase 5's
//! emit baselines will constrain them; this does not.
//!
//! # What "compare trees" means here, and what it misses
//!
//! Full structural equality over 192 node types would need a generated comparator.
//! This compares a **fingerprint** in two parts instead.
//!
//! The first is a pre-order walk emitting each node's kind, plus the payload that
//! distinguishes nodes of the same kind — identifier and literal text, and the
//! `const`/`let` flags that are not in the tree at all.
//!
//! The second exists because the walk alone is not enough, and the gap is
//! measurable rather than theoretical: **none of the 36 optional `…_token` fields
//! on the generated nodes is traversed by `walk_node`**. A `?` on an optional
//! property, a `!`, a `*` on a generator, a `?.` — every one is a node the parser
//! allocated and the walk never reaches. A printer that dropped `a?.b` to `a.b`
//! would produce the same walk fingerprint and pass.
//!
//! So the second part counts, over the whole `NodeTable`, exactly the token kinds
//! those fields can hold. It has no ordering information and needs none: it is
//! there to notice that one vanished.
//!
//! It is deliberately *not* a histogram of every kind in the table. The table also
//! holds JSDoc, which is trivia rather than tree structure — a comment-preserving
//! printer is not what this gate is for, and counting it made 455 cases fail for
//! dropping identifiers that existed only inside comments. Narrowing to the token
//! kinds keeps the check aimed at the gap it was added for.
//!
//! Together the two catch a dropped modifier, a lost `?`, a `var` printed for a
//! `const`, a mis-escaped string, and two tokens that merged.
//!
//! What still escapes both is a change preserving the kind sequence, every
//! payload, *and* the kind histogram — a reordering of same-kind siblings, say.
//! Nothing in a printer that walks children in order produces that. A generated
//! structural comparator would close it properly, and belongs with Phase 5's emit
//! work rather than here (`bd tsr-49v.4`).
//!
//! # Why a parse error is not a failure
//!
//! 4,999 of 5,031 judged corpus cases parse cleanly, and the rest are deliberately
//! malformed — the corpus is a *compiler* test suite, so it contains syntax errors
//! on purpose. Printing a tree built from error recovery and expecting it to
//! reparse identically is not a property the printer owes anyone: recovery invents
//! nodes that have no source text. Cases whose input does not parse cleanly are
//! therefore skipped, with the count reported.

use std::collections::BTreeMap;

use tsr_ast::{Node, NodeTable, SourceFile, SyntaxKind, Visit};
use tsr_parser::{ParsedFile, ScriptKind};

use crate::{
    CaseEntry,
    suite::{Outcome, Suite},
};

/// The `printer_round_trip` suite.
pub struct PrinterRoundTrip;

impl Suite for PrinterRoundTrip {
    fn name(&self) -> &'static str {
        "printer_round_trip"
    }

    fn describes(&self) -> &'static str {
        "every unit of the case parses, prints, and reparses to the same tree"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        let Ok(test) = case.load() else {
            return Outcome::Failed { reason: "case did not load".into() };
        };

        // Every unit is checked for clean input *before* any is printed. Doing it
        // lazily made the denominator depend on the printer: a case whose first
        // unit was unsupported and whose second does not parse counted as judged
        // until the printer learned the first unit, then silently became a skip.
        // A denominator that moves when the component under test improves is the
        // shape of a measurement bug, not of progress.
        let units: Vec<_> = test
            .files
            .iter()
            .map(|unit| (unit, ScriptKind::from_file_name(&unit.name)))
            .filter(|(_, kind)| *kind != ScriptKind::Json)
            .collect();

        let mut parsed_units = Vec::with_capacity(units.len());
        for (unit, kind) in units {
            let parsed = ParsedFile::parse_with_script_kind(unit.content.clone(), kind);
            if !parsed.diagnostics().is_empty() {
                return Outcome::Skipped {
                    reason: "the case does not parse cleanly, so there is no tree to preserve"
                        .into(),
                };
            }
            parsed_units.push((parsed, kind));
        }

        for (first, kind) in &parsed_units {
            let kind = *kind;
            let printed = first.with_ast(|file| tsr_printer::print(file, first.nodes()));
            if let Some(unhandled) = printed.unsupported.first() {
                return Outcome::Unsupported {
                    reason: format!("printer does not implement {}", unhandled.name()),
                };
            }

            let before = first.with_ast(|file| fingerprint(file, first.nodes()));
            let before_kinds = kind_histogram(first.nodes());
            let second = ParsedFile::parse_with_script_kind(printed.text.clone(), kind);
            if !second.diagnostics().is_empty() {
                let first_error = second.diagnostics()[0].text();
                return Outcome::Failed {
                    reason: format!("printed text does not parse: {first_error}"),
                };
            }
            let after = second.with_ast(|file| fingerprint(file, second.nodes()));

            if before != after {
                return Outcome::Failed { reason: first_difference(&before, &after) };
            }
            let after_kinds = kind_histogram(second.nodes());
            if before_kinds != after_kinds {
                return Outcome::Failed {
                    reason: histogram_difference(&before_kinds, &after_kinds),
                };
            }
        }
        Outcome::Passed
    }
}

/// One entry of a tree fingerprint.
type Entry = (SyntaxKind, Option<String>);

/// A pre-order walk of kinds and identifying payload.
fn fingerprint<'a>(file: &'a SourceFile<'a>, nodes: &NodeTable) -> Vec<Entry> {
    let mut walker = Fingerprint { nodes, out: Vec::new() };
    walker.visit_source_file(file);
    walker.out
}

struct Fingerprint<'t> {
    nodes: &'t NodeTable,
    out: Vec<Entry>,
}

impl<'a> Visit<'a> for Fingerprint<'_> {
    fn visit_node(&mut self, node: Node<'a>) {
        let kind = node.node_id().map_or(SyntaxKind::Unknown, |id| self.nodes.kind(id));
        self.out.push((kind, payload(node, self.nodes)));
        tsr_ast::visit::walk_node(self, node);
    }
}

/// What distinguishes two nodes of the same kind.
fn payload(node: Node<'_>, nodes: &NodeTable) -> Option<String> {
    match node {
        Node::Identifier(n) => Some(n.text.to_string()),
        Node::PrivateIdentifier(n) => Some(n.text.to_string()),
        Node::StringLiteral(n) => Some(n.text.to_string()),
        Node::NumericLiteral(n) => Some(n.text.to_string()),
        Node::BigIntLiteral(n) => Some(n.text.to_string()),
        Node::RegularExpressionLiteral(n) => Some(n.text.to_string()),
        Node::NoSubstitutionTemplateLiteral(n) => Some(n.text.to_string()),
        Node::TemplateHead(n) => Some(n.text.to_string()),
        Node::TemplateMiddle(n) => Some(n.text.to_string()),
        Node::TemplateTail(n) => Some(n.text.to_string()),
        Node::Token(n) => Some(n.kind.name().to_string()),
        Node::KeywordExpression(n) => Some(n.kind.name().to_string()),
        Node::KeywordTypeNode(n) => Some(n.kind.name().to_string()),
        // `const`/`let`/`using` are flags, not tokens. Without this a `var`
        // printed for a `const` would round-trip silently — the one difference
        // this fingerprint would otherwise be blind to.
        Node::VariableDeclarationList(n) => n.node_id.map(|id| {
            let flags = nodes.flags(id) & tsr_ast::NodeFlags::BLOCK_SCOPED;
            format!("{flags:?}")
        }),
        _ => None,
    }
}

/// How many nodes of each kind the parser allocated, tokens included.
///
/// The walk misses every `…_token` field; this does not, because a token is a
/// registered node like any other.
fn kind_histogram(nodes: &NodeTable) -> BTreeMap<SyntaxKind, usize> {
    let mut counts = BTreeMap::new();
    for index in 0..nodes.len() {
        let id = tsr_ast::NodeId::new(u32::try_from(index).unwrap_or(u32::MAX - 1));
        let kind = nodes.kind(id);
        if TRACKED_TOKENS.contains(&kind) {
            *counts.entry(kind).or_insert(0) += 1;
        }
    }
    counts
}

/// The token kinds that live in untraversed `…_token` fields.
///
/// Each one changes meaning rather than formatting: dropping `?.` turns `a?.b`
/// into `a.b`, and dropping `...` turns a rest parameter into a positional one.
/// Both reparse cleanly and neither shows up in the walk.
const TRACKED_TOKENS: &[SyntaxKind] = &[
    SyntaxKind::QuestionToken,
    SyntaxKind::ExclamationToken,
    SyntaxKind::AsteriskToken,
    SyntaxKind::DotDotDotToken,
    SyntaxKind::QuestionDotToken,
];

/// Name a kind whose count changed.
fn histogram_difference(
    before: &BTreeMap<SyntaxKind, usize>,
    after: &BTreeMap<SyntaxKind, usize>,
) -> String {
    for (kind, count) in before {
        let now = after.get(kind).copied().unwrap_or(0);
        if now != *count {
            return format!("{} count changed: {count} became {now}", kind.name());
        }
    }
    for (kind, count) in after {
        if !before.contains_key(kind) {
            return format!("{} appeared: 0 became {count}", kind.name());
        }
    }
    "node kind counts differ".to_string()
}

/// Name the first place two fingerprints diverge.
fn first_difference(before: &[Entry], after: &[Entry]) -> String {
    let at = before.iter().zip(after).position(|(a, b)| a != b);
    match at {
        Some(index) => {
            let show = |entry: &Entry| match &entry.1 {
                Some(payload) => format!("{}({payload})", entry.0.name()),
                None => entry.0.name().to_string(),
            };
            format!(
                "tree differs at node {index}: {} became {}",
                show(&before[index]),
                show(&after[index])
            )
        }
        None => format!("tree has {} nodes, printed has {}", before.len(), after.len()),
    }
}
