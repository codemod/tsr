//! Judging the binder against upstream's `.symbols` baselines.
//!
//! See [`crate::symbols_baseline`] for the format and why it is the right oracle.
//!
//! # What is compared, and what is not
//!
//! For each symbol upstream names, the baseline gives its declaration sites. This
//! suite asks: **did we create a symbol of that name, declared on the same
//! lines?** That tests symbol creation, declaration merging (one symbol with two
//! declarations rather than two symbols), and scope placement, all of which are
//! model-independent — the baseline says nothing about how either side stores a
//! symbol table, which matters because ours is deliberately not upstream's.
//!
//! **Multi-file cases are compared unit by unit.** Each baseline section names a
//! unit, and each unit is bound on its own — which is not an approximation but
//! the truth: there is no program and no cross-file linking yet, so a symbol
//! declared in `a.ts` is not visible from `b.ts`. The baseline agrees, because
//! the comparison already drops any symbol whose declarations live in another
//! file. Before this, 1,153 cases — 13% of the corpus with `.symbols` baselines
//! — were skipped outright.
//!
//! It does **not** yet test resolution: which *occurrence* binds to which symbol.
//! The baseline has that, but the occurrence's own position is implicit in the
//! layout (the annotation follows the line it describes), so recovering it means
//! reconstructing the interleaving. That is worth doing and is filed.
//!
//! **Positions are full starts, recovered rather than recorded.** Upstream's
//! `Decl(file, line, character)` is the declaration node's *full start* — where
//! its leading trivia begins — not where its first token does. For a member on
//! the line after `class C {`, upstream reports the position of the `{`. We do
//! not store full start on nodes, so [`symbols_baseline::FullStarts`] recovers it
//! by scanning the file forwards with the scanner, which computes exactly this
//! quantity for the parser. It is exact except in regions where a context-free
//! scan diverges from the parser's tokenisation (regex, `>>` splitting, JSX
//! text), which fall back to a backwards walk.
//!
//! **Name spelling is normalised away.** Upstream prints a name the way the
//! source wrote it: `C["foo"]` for a string-literal member, `"fs"` for an ambient
//! module, `C[1]` for a numeric one. Those are spellings of a fact the binder
//! stores as a value, so [`normalise_symbol_name`] reduces them to dotted form
//! rather than teaching the binder to remember quotes it has no other use for.
//! *Computed* names (`A[expr]`) are left alone — they are genuinely late-bound
//! and we create no symbol for them, and normalising them would turn a real gap
//! into a passing case.
//!
//! **A symbol is indexed under every dotted suffix of its qualified name.**
//! Upstream prints `checker.symbolToString(symbol, node.parent)` — the shortest
//! name accessible *from the reference site*. A use of `m` inside
//! `namespace M { export class C { m() {} } }` prints `C.m`; a use from outside
//! prints `M.C.m`. Same symbol, two spellings, and choosing between them is a
//! resolution we have no checker to redo. Accepting any suffix reproduces both.
//! It weakens the test — `C.m` would also match a `C.m` nested elsewhere — and
//! that is the price of not having a checker yet.
//!
//! **Anonymous containers do not qualify.** A member of `{ salt: 2 }` prints
//! bare, because no expression names the object literal; a member of an unnamed
//! class expression prints `(Anonymous class).foo`, because upstream gives that
//! one a display name. See [`anonymity_of`].
//!
//! Lines only, not columns: a column comparison would fail on the fallback
//! positions above and attribute it to the binder.

use std::collections::BTreeSet;

use tsr_ast::{NodeTable, SyntaxKind};
use tsr_binder::BindResult;

use crate::{
    corpus::CaseEntry,
    suite::{Outcome, Suite},
    symbols_baseline,
};

/// Does every symbol upstream found exist here, declared on the same lines?
pub struct BinderSymbols;

impl Suite for BinderSymbols {
    fn name(&self) -> &'static str {
        "binder_symbols"
    }

    fn describes(&self) -> &'static str {
        "every symbol in the .symbols baseline exists with the same declaration lines"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        let Some(baseline) = case.expected_symbols() else {
            return Outcome::Skipped { reason: "no .symbols baseline".into() };
        };
        if case.has_known_divergence() {
            return Outcome::Skipped {
                reason: "upstream records a known divergence from TypeScript (.diff baseline)"
                    .into(),
            };
        }

        let expected_files = symbols_baseline::parse(&baseline);
        let parsed = match case.load() {
            Ok(parsed) => parsed,
            Err(err) => return Outcome::Failed { reason: format!("{err:#}") },
        };

        // Each baseline section describes one unit, and each unit is bound on its
        // own — which is not a limitation here but the truth: there is no program
        // and no cross-file linking yet, so a symbol declared in `a.ts` is simply
        // not visible from `b.ts`. The baseline agrees, because the comparison
        // below already drops any symbol whose declarations live in another file.
        let mut missing = Vec::new();
        let mut compared = 0usize;
        let mut unparsable = 0usize;

        for expected_file in &expected_files {
            let Some(unit) =
                parsed.files.iter().find(|unit| same_unit(&unit.name, &expected_file.file))
            else {
                continue;
            };
            if !crate::scanner_suite::is_typescript_unit(&unit.name) {
                continue;
            }

            let arena = tsr_core::Arena::new();
            let script_kind = tsr_parser::ScriptKind::from_file_name(&unit.name);
            let result = tsr_parser::parse_with_script_kind(&arena, &unit.content, script_kind);
            // A file we cannot parse tells us nothing about the binder.
            if !result.diagnostics.is_empty() {
                unparsable += 1;
                continue;
            }
            let bound = tsr_binder::bind(result.source_file, &result.nodes);

            // One forward scan of the file, reused for every declaration position.
            let full_starts = symbols_baseline::FullStarts::scan(&unit.content);

            // What we produced, keyed by *qualified* name.
            let mut ours: std::collections::HashMap<String, BTreeSet<u32>> =
                std::collections::HashMap::new();
            for (id, symbol) in bound.symbols().iter() {
                let mut declared = BTreeSet::new();
                for declaration in &symbol.declarations {
                    let span = result.nodes.span(*declaration);
                    // Upstream reports the *full start*; see `symbols_baseline`.
                    let pos = full_starts.of(&unit.content, span.start);
                    let (line, _) = symbols_baseline::line_and_character(&unit.content, pos);
                    declared.insert(line);
                }
                // Indexed under every dotted suffix, not just the full chain.
                // Upstream prints the *shortest name accessible from the reference
                // site*: a use of `m` inside `namespace M` prints `C.m`, while a use
                // from outside prints `M.C.m` — same symbol, two spellings, and which
                // one appears depends on a resolution we have no checker to redo.
                // Accepting any suffix is the honest approximation; it weakens the
                // test slightly, since `C.m` would also match a `C.m` nested
                // somewhere else entirely.
                let full = qualified_name(&bound, &result.nodes, id);
                for (offset, _) in std::iter::once((0, '.'))
                    .chain(full.match_indices('.').map(|(i, _)| (i + 1, '.')))
                {
                    ours.entry(full[offset..].to_string()).or_default().extend(&declared);
                }
            }

            // What upstream expects, deduplicated: the baseline repeats a symbol once
            // per occurrence, and a symbol is one fact however often it is used.
            // `BTreeMap`, not `HashMap`: the loop below stops after three misses, so
            // hash order would decide *which* three a failing case reports and the
            // committed snapshot would churn on reruns with no code change. A snapshot
            // that moves on its own teaches reviewers to ignore its diff.
            let mut expected: std::collections::BTreeMap<String, BTreeSet<u32>> =
                std::collections::BTreeMap::new();
            for reference in &expected_file.refs {
                if reference.declarations.is_empty() {
                    continue;
                }
                // Only symbols declared *in this file*. A reference to `console`
                // resolves to `Decl(lib.dom.d.ts, --, --)`, and we neither load lib
                // files nor could produce those declarations; requiring them would
                // score the absence of a standard library as a binder failure. The
                // same filter is what makes per-unit binding sound: a symbol from a
                // sibling unit is another file's declaration and is skipped here.
                if reference.declarations.iter().any(|d| d.file != expected_file.file) {
                    continue;
                }
                let lines = expected.entry(normalise_symbol_name(&reference.symbol)).or_default();
                for declaration in &reference.declarations {
                    lines.insert(declaration.line);
                }
            }
            if expected.is_empty() {
                continue;
            }
            compared += 1;

            let multi = expected_files.len() > 1;
            for (name, lines) in &expected {
                let where_ =
                    if multi { format!("{}: ", expected_file.file) } else { String::new() };
                match ours.get(name.as_str()) {
                    None => missing.push(format!("{where_}{name}: no symbol")),
                    // Ours must *contain* the expected lines rather than equal them:
                    // upstream lists only the declarations reachable from a use site,
                    // while we hold every declaration of the symbol.
                    Some(found) if !lines.is_subset(found) => missing.push(format!(
                        "{where_}{name}: declared on {found:?}, expected to include {lines:?}"
                    )),
                    Some(_) => {}
                }
                if missing.len() >= 3 {
                    break;
                }
            }
            if missing.len() >= 3 {
                break;
            }
        }

        if compared == 0 {
            return Outcome::Skipped {
                reason: if unparsable > 0 {
                    "the parser reports errors for this file".into()
                } else {
                    "the baseline names no symbols in any unit we bind".into()
                },
            };
        }

        if missing.is_empty() {
            Outcome::Passed
        } else {
            Outcome::Failed {
                reason: format!("{} symbol(s): {}", missing.len(), missing.join("; ")),
            }
        }
    }
}

/// A symbol's name as the baseline writes it.
///
/// Upstream's baseline prints `checker.symbolToString(symbol, node.parent)` —
/// the name as reachable *from the reference site*, not a raw parent walk. The
/// difference shows on anonymous containers: a member of `{ salt: 2 }` prints as
/// `salt`, because there is no expression that names the object literal, while a
/// member of an unnamed class expression prints as `(Anonymous class).foo`,
/// because upstream gives that one a display name. A parent walk that stops at
/// the right places reproduces both without a checker.
fn qualified_name(bound: &BindResult<'_>, nodes: &NodeTable, id: tsr_binder::SymbolId) -> String {
    let symbols = bound.symbols();
    let symbol = symbols.get(id);
    let Some(parent) = symbol.parent else {
        return symbol.name.to_string();
    };
    match anonymity_of(symbols.get(parent).name) {
        // Unreachable by any name: the member is only ever written bare.
        Anonymity::Unnameable => symbol.name.to_string(),
        Anonymity::Displayed(fallback) => {
            let container = assigned_name(bound, nodes, parent)
                .map_or_else(|| fallback.to_string(), |named| qualified_name(bound, nodes, named));
            format!("{container}.{}", symbol.name)
        }
        Anonymity::Named => {
            format!("{}.{}", qualified_name(bound, nodes, parent), symbol.name)
        }
    }
}

/// The symbol of the variable an anonymous declaration is assigned to.
///
/// A direct port of the rule in upstream's `getNameOfSymbolAsWritten`
/// (`internal/checker/nodebuilderimpl.go:1005`): before falling back to
/// `(Anonymous class)`, it checks whether the declaration's parent is a
/// `VariableDeclaration` and, if so, displays the variable's name. That is why
/// `const C = class { #x }` gives `C.#x` in the baselines and not
/// `(Anonymous class).#x`.
fn assigned_name(
    bound: &BindResult<'_>,
    nodes: &NodeTable,
    symbol: tsr_binder::SymbolId,
) -> Option<tsr_binder::SymbolId> {
    let declaration = *bound.symbols().get(symbol).declarations.first()?;
    let parent = nodes.parent(declaration)?;
    (nodes.kind(parent) == SyntaxKind::VariableDeclaration).then(|| bound.symbol_of(parent))?
}

/// How a container participates in a qualified name.
enum Anonymity {
    /// An ordinary named container: qualify with its own qualified name.
    Named,
    /// Anonymous, and upstream prints it under this display name.
    Displayed(&'static str),
    /// Anonymous and unnameable: members are only ever written bare.
    Unnameable,
}

/// How the baseline spells a container, given its symbol name.
///
/// The internal `__`-prefixed names are upstream's markers for a symbol no
/// source can spell. An unnamed class expression still gets a display name in
/// the baseline; an object or type literal does not.
fn anonymity_of(name: &str) -> Anonymity {
    match name {
        "__class" => Anonymity::Displayed("(Anonymous class)"),
        "__object" | "__type" | "__jsxAttributes" => Anonymity::Unnameable,
        _ => Anonymity::Named,
    }
}

/// Whether a unit's `@filename` names the same file as a baseline section.
///
/// The two spell paths differently: a case may write `./a.ts` or a Windows
/// `C:\a\b\c.ts` where the baseline says `a.ts` and `C:/a/b/c.ts`, and a
/// rooted case directory (`/.src/node_modules/…`) is written relative in the
/// baseline. Comparing the raw strings left 18 cases untested for no better
/// reason than punctuation.
fn same_unit(unit: &str, baseline: &str) -> bool {
    fn normalise(path: &str) -> String {
        let slashes = path.replace('\\', "/");
        let trimmed = slashes.trim_start_matches("./").to_string();
        // Collapse `a//b`, which appears in a few hand-written cases.
        trimmed.replace("//", "/")
    }
    let (unit, baseline) = (normalise(unit), normalise(baseline));
    if unit == baseline {
        return true;
    }
    // A rooted unit path ending in the baseline's relative one is the same file.
    // Anchored at a separator so `ab.ts` does not match `b.ts`.
    unit.strip_suffix(&baseline).is_some_and(|prefix| prefix.ends_with('/'))
}

/// The baseline's spelling of a symbol name, reduced to plain dotted form.
///
/// Upstream prints a name the way the *source* wrote it: `class C { "foo"() {} }`
/// gives `C["foo"]`, `declare module "fs"` gives `"fs"`, and a numeric member
/// gives `C[1]`. Those are three spellings of one fact — a member of `C` called
/// `foo` — and which spelling was used is a property of the source, not of the
/// binder. A symbol's name here is the value, so the two are reconciled by
/// stripping the spelling off the baseline rather than by teaching the binder to
/// remember quotes it has no other use for.
///
/// **Computed names are deliberately left alone.** `A[A.p1]` and `[Symbol.iterator]`
/// are late-bound: the name is not known until the checker evaluates the
/// expression, and we create no symbol for them at all. Normalising those would
/// turn a real gap into a passing case.
fn normalise_symbol_name(name: &str) -> String {
    // A whole name in quotes is a string literal spelled as the source wrote it:
    // `declare module "fs"` is the symbol `"fs"`, and `{ 'a': 1 }` gives `'a'`.
    // Either quote, because the baseline reproduces the spelling.
    for quote in ['"', '\''] {
        if name.len() >= 2 && name.starts_with(quote) && name.ends_with(quote) {
            return name[1..name.len() - 1].to_string();
        }
    }
    let mut out = String::with_capacity(name.len());
    let mut rest = name;
    while let Some(open) = rest.find('[') {
        let Some(close_offset) = rest[open..].find(']') else { break };
        let close = open + close_offset;
        let inside = &rest[open + 1..close];
        let Some(literal) = static_bracket_name(inside) else {
            // Computed: keep it verbatim, including the brackets, and stop —
            // anything after it is qualified by a name we do not know.
            break;
        };
        out.push_str(&rest[..open]);
        if !out.is_empty() {
            out.push('.');
        }
        out.push_str(literal);
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

/// The value inside `[…]` when it is a literal rather than an expression.
fn static_bracket_name(inside: &str) -> Option<&str> {
    // Either quote: the baseline reproduces the source spelling, and `C['a']`
    // and `C["a"]` name the same member.
    for quote in ['"', '\''] {
        if inside.len() >= 2 && inside.starts_with(quote) && inside.ends_with(quote) {
            return Some(&inside[1..inside.len() - 1]);
        }
    }
    // A numeric literal in any spelling — `2.0`, `0b11`, `1e3`. Starting with a
    // digit is what separates them from an expression: `A[A.p1]` and `A[a]` are
    // computed and must keep their brackets.
    if inside.starts_with(|c: char| c.is_ascii_digit()) {
        return Some(inside);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_string_literal_member_name_reduces_to_dotted_form() {
        // `class C { "foo"() {} }` — we store the name `foo` with parent `C`,
        // upstream prints how it was spelled.
        assert_eq!(normalise_symbol_name("C[\"foo\"]"), "C.foo");
        assert_eq!(normalise_symbol_name("C[1]"), "C.1");
        assert_eq!(normalise_symbol_name("A.B[\"c\"].d"), "A.B.c.d");
    }

    #[test]
    fn an_ambient_module_name_loses_its_quotes() {
        assert_eq!(normalise_symbol_name("\"fs\""), "fs");
        assert_eq!(normalise_symbol_name("\"./relativeModule\""), "./relativeModule");
        // Either quote: `{ 'a': 1 }` prints `'a'` and `{ "a": 1 }` prints `"a"`.
        assert_eq!(normalise_symbol_name("'a'"), "a");
        assert_eq!(normalise_symbol_name("''"), "");
    }

    #[test]
    fn a_computed_name_is_left_alone() {
        // Late-bound: we create no symbol for these, and normalising them would
        // turn a real gap into a passing case.
        assert_eq!(normalise_symbol_name("A[A.p1]"), "A[A.p1]");
        assert_eq!(normalise_symbol_name("Result[Symbol.iterator]"), "Result[Symbol.iterator]");
        assert_eq!(normalise_symbol_name("[foo()]"), "[foo()]");
    }

    /// Bind a snippet and return the qualified name of the symbol called `name`.
    fn qualified(source: &str, name: &str) -> String {
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "the snippet should parse cleanly");
        let bound = tsr_binder::bind(parsed.source_file, &parsed.nodes);
        let (id, _) = bound
            .symbols()
            .iter()
            .find(|(_, symbol)| symbol.name == name)
            .unwrap_or_else(|| panic!("no symbol named {name}"));
        qualified_name(&bound, &parsed.nodes, id)
    }

    #[test]
    fn a_class_expression_is_displayed_under_the_variable_it_is_assigned_to() {
        // Upstream's getNameOfSymbolAsWritten checks for a VariableDeclaration
        // parent before falling back to `(Anonymous class)`, which is why the
        // baselines say `C.#x` here and not `(Anonymous class).#x`.
        assert_eq!(qualified("const C = class { #x = 1; };", "#x"), "C.#x");
    }

    #[test]
    fn a_class_expression_assigned_to_nothing_stays_anonymous() {
        assert_eq!(
            qualified("declare function f(c: unknown): void;\nf(class { #x = 1; });", "#x"),
            "(Anonymous class).#x"
        );
    }

    #[test]
    fn an_object_literal_member_is_never_qualified() {
        assert_eq!(qualified("const o = { salt: 2 };", "salt"), "salt");
    }

    #[test]
    fn a_unit_matches_its_baseline_section_across_path_spellings() {
        assert!(same_unit("./a.ts", "a.ts"));
        assert!(same_unit("C:\\a\\b\\c.ts", "C:/a/b/c.ts"));
        assert!(same_unit(
            "/.src/node_modules/@types/node/index.d.ts",
            "node_modules/@types/node/index.d.ts"
        ));
        assert!(same_unit(
            "node_modules/lit-element/development//lit-element.d.ts",
            "node_modules/lit-element/development/lit-element.d.ts"
        ));
        // Not the same file, and the separator anchor is what stops it.
        assert!(!same_unit("ab.ts", "b.ts"));
        assert!(!same_unit("a.ts", "b.ts"));
    }

    #[test]
    fn an_ordinary_name_is_unchanged() {
        assert_eq!(normalise_symbol_name("C"), "C");
        assert_eq!(normalise_symbol_name("M.X"), "M.X");
    }
}
