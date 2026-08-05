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
//! **Multi-file cases are bound as one program and compared unit by unit.** The
//! case's units go into a [`tsr_compiler::Program`], and each baseline section is
//! then compared against the file it names. Comparing per unit is not an
//! approximation but the truth: binding is per-file upstream too, and what
//! crosses files is *resolution*, which the checker does. So the comparison still
//! drops any symbol whose declarations live in another file. Before multi-file
//! cases were compared at all, 1,153 — 13% of the corpus with `.symbols`
//! baselines — were skipped outright.
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
//! one a display name; an unnamed *function* expression is displayed under the
//! variable it was assigned to, since that is what `getNameOfDeclaration` falls
//! back to for one. See [`anonymity_of`].
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
        // Varied before absent. A configuration-varied case has no *plain*
        // `.symbols`, so asking for that first reported all 1,397 of them as
        // "no baseline" — the right exclusion under a reason that says something
        // else. Checked when this was fixed for `checker_types`: no case has both
        // a plain and a varied baseline, so the pass *rate* was never affected;
        // only the skip breakdown was, and it overstated how much upstream had
        // recorded nothing for.
        if case.has_varied_symbols() {
            return Outcome::Skipped {
                reason: "configuration-varied baseline (bd tsr-bb4.1)".into(),
            };
        }
        let Some(baseline) = case.expected_symbols() else {
            return Outcome::Skipped { reason: "upstream recorded no .symbols baseline".into() };
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

        // Each baseline section describes one unit. The units are bound together
        // as one program, but compared one at a time, because that is what the
        // baseline is: per file. A symbol declared in `a.ts` is still not visible
        // from `b.ts` — resolving across files is the checker's, not the
        // binder's — and the comparison below drops any symbol whose declarations
        // live elsewhere.
        let mut missing = Vec::new();
        let mut compared = 0usize;
        let mut unparsable = 0usize;

        // Every unit of the case, bound together as one program. The comparison
        // below is still per file — the baseline is written per file — but the
        // files now exist in one object rather than one at a time, which is what
        // cross-file resolution will need.
        // One arena for the case, dropped with it. Everything the program holds
        // borrows from it, which is why it is a local of this function rather
        // than something the suite owns — see ADR-0034.
        let arena = tsr_core::Arena::new();
        let program = tsr_compiler::Program::in_arena(
            &arena,
            tsr_compiler::ProgramOptions {
                files: parsed
                    .files
                    .iter()
                    .filter(|unit| crate::scanner_suite::is_typescript_unit(&unit.name))
                    .map(|unit| (unit.name.clone(), unit.content.clone()))
                    .collect(),
                ..Default::default()
            },
        );

        for expected_file in &expected_files {
            let Some(unit) =
                parsed.files.iter().find(|unit| same_unit(&unit.name, &expected_file.file))
            else {
                continue;
            };
            if !crate::scanner_suite::is_typescript_unit(&unit.name) {
                continue;
            }
            let Some(file) = program.source_file(&unit.name, "/") else { continue };
            // A file we cannot parse tells us nothing about the binder.
            if !file.diagnostics().is_empty() {
                unparsable += 1;
                continue;
            }

            // One forward scan of the file, reused for every declaration position.
            let full_starts = symbols_baseline::FullStarts::scan(&unit.content);

            let bound = program.binder();
            let nodes = program.nodes();
            let ours = {
                // Which symbols share a declaration, so a `default` export can be
                // displayed under the name its declaration was written with; see
                // [`display_names`].
                let mut names_by_declaration: NamesByDeclaration<'_> =
                    NamesByDeclaration::default();
                for (_, symbol) in bound.symbols().iter() {
                    if symbol.name == INTERNAL_DEFAULT {
                        continue;
                    }
                    for declaration in &symbol.declarations {
                        names_by_declaration.entry(*declaration).or_default().push(symbol.name);
                    }
                }

                // What we produced, keyed by *qualified* name.
                let mut ours: std::collections::HashMap<String, BTreeSet<u32>> =
                    std::collections::HashMap::new();
                for (id, symbol) in bound.symbols().iter() {
                    // **Only this file's symbols.** Under program-wide identity
                    // (ADR-0034) the store holds every unit's, and a `Span` is
                    // still an offset into its own file's text — so a sibling
                    // unit's declaration read against `unit.content` would
                    // produce a line number from the wrong file rather than an
                    // error. The expected side already drops cross-file symbols
                    // (see below), so without this filter the comparison would
                    // silently widen on garbage positions.
                    if !symbol.declarations.iter().any(|d| file.contains(*d)) {
                        continue;
                    }
                    let mut declared = BTreeSet::new();
                    for declaration in &symbol.declarations {
                        if !file.contains(*declaration) {
                            continue;
                        }
                        let span = nodes.span(*declaration);
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
                    for full in
                        display_names(bound, nodes, id, &names_by_declaration, &unit.content)
                    {
                        for offset in dotted_suffixes(&full) {
                            ours.entry(full[offset..].to_string()).or_default().extend(&declared);
                        }
                    }
                }
                ours
            };

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

/// A symbol's names as the baseline may write it.
///
/// Upstream's baseline prints `checker.symbolToString(symbol, node.parent)` —
/// the name as reachable *from the reference site*, not a raw parent walk. The
/// difference shows on anonymous containers: a member of `{ salt: 2 }` prints as
/// `salt`, because there is no expression that names the object literal, while a
/// member of an unnamed class expression prints as `(Anonymous class).foo`,
/// because upstream gives that one a display name. A parent walk that stops at
/// the right places reproduces both without a checker.
fn display_names(
    bound: &BindResult<'_>,
    nodes: &NodeTable,
    id: tsr_binder::SymbolId,
    names_by_declaration: &NamesByDeclaration<'_>,
    source: &str,
) -> Vec<String> {
    let symbols = bound.symbols();
    let symbol = symbols.get(id);

    // A default export is named `default`, but upstream prints it under the name
    // its declaration was written with whenever the reference is in the same file
    // (`getNameOfSymbolAsWritten`, `nodebuilderimpl.go:978`): `export default
    // function foo` prints `foo`, while `export default class {}` has no name to
    // print and stays `default`. Both spellings appear in the baselines, so both
    // are offered — the same accommodation the dotted suffixes above make, and
    // for the same reason: choosing between them is a resolution we have no
    // checker to redo. The written name is recovered from the *local* symbol that
    // shares the declaration, since that is where the source's spelling landed.
    // An anonymous class or function has no name of its own, and upstream prints
    // the variable it was assigned to, or a placeholder
    // (`getNameOfSymbolAsWritten`: "(Anonymous class)", "(Anonymous function)").
    // The container path below asks the same question; this is the answer for the
    // symbol itself, which is what `>this : Symbol((Anonymous class), …)` needs.
    let mut own: Vec<&str> = vec![symbol.name];
    if let Anonymity::Displayed(placeholder) = anonymity_of(symbol.name) {
        let assigned = assigned_name(bound, nodes, id)
            .map(|named| display_names(bound, nodes, named, names_by_declaration, source));
        return match assigned {
            Some(names) if !names.is_empty() => names,
            _ => vec![placeholder.to_string()],
        };
    }
    if symbol.name == INTERNAL_DEFAULT
        && let Some(written) = symbol.declarations.iter().find_map(|d| names_by_declaration.get(d))
    {
        own.extend(written);
    }

    // The container's spellings, or a single empty one when nothing names it —
    // a member of an object literal is written bare.
    let containers: Vec<String> = match symbol.parent {
        None => vec![String::new()],
        Some(parent) => match anonymity_of(symbols.get(parent).name) {
            Anonymity::Unnameable => vec![String::new()],
            Anonymity::Displayed(fallback) => assigned_name(bound, nodes, parent).map_or_else(
                || vec![fallback.to_string()],
                |named| display_names(bound, nodes, named, names_by_declaration, source),
            ),
            Anonymity::Named => display_names(bound, nodes, parent, names_by_declaration, source),
        },
    };

    let mut names = Vec::new();
    for container in &containers {
        // A member written with a *computed* name is printed the way it was
        // written — `C[Symbol.iterator]`, `[foo()]`, `[-1]` — because
        // `getNameOfSymbolAsWritten` falls through to the declaration's name node
        // and `declarationNameToString` of a `ComputedPropertyName` is the source
        // text of it. That holds whether or not the name is late-bound, so the
        // written form is offered *alongside* the ordinary spelling rather than
        // instead of it: `{ ['a']: 1 }` declares `a` statically and the baseline
        // may print either.
        for declaration in &symbol.declarations {
            let Some(computed) = bound.computed_name(*declaration) else { continue };
            let span = nodes.span(computed);
            let (Ok(start), Ok(end)) = (usize::try_from(span.start), usize::try_from(span.end))
            else {
                continue;
            };
            if let Some(text) = source.get(start..end) {
                names.push(format!("{container}{text}"));
            }
        }
        if symbol.name == INTERNAL_COMPUTED {
            // There is no other spelling: the name is whatever the expression
            // evaluates to, and only the checker knows that.
            continue;
        }
        for name in &own {
            if container.is_empty() {
                names.push((*name).to_string());
            } else {
                names.push(format!("{container}.{name}"));
            }
        }
    }
    names
}

/// Byte offsets of every dotted suffix of a display name, longest first.
///
/// Only dots *outside* brackets split: `C[Symbol.iterator]` is one name, and
/// splitting at the dot inside it would index the nonsense `iterator]`.
fn dotted_suffixes(name: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    let mut depth = 0i32;
    for (index, byte) in name.bytes().enumerate() {
        match byte {
            b'[' => depth += 1,
            b']' => depth -= 1,
            b'.' if depth == 0 => offsets.push(index + 1),
            _ => {}
        }
    }
    offsets
}

/// Upstream's `ast.InternalSymbolNameComputed`: a member whose name is not known
/// until the checker evaluates the expression.
const INTERNAL_COMPUTED: &str = "__computed";

/// Every symbol's name, keyed by the declaration it was first declared on.
///
/// Upstream picks the first declaration that *has* a name
/// (`getNameOfSymbolAsWritten`), which is why every declaration is indexed and
/// not only the first. Used only to recover the written name of a `default`
/// export; see
/// [`display_names`].
type NamesByDeclaration<'a> = std::collections::HashMap<tsr_ast::NodeId, Vec<&'a str>>;

/// Upstream's `ast.InternalSymbolNameDefault`, the name every `export default`
/// in a file shares.
const INTERNAL_DEFAULT: &str = "default";

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
        // An anonymous function that carries expando properties is displayed
        // under the variable it was assigned to, for the same reason a class
        // expression is: `getNameOfDeclaration` falls back to `GetAssignedName`
        // for a function expression, an arrow, and a class expression alike. A
        // *named* function expression never reaches here — its symbol is named.
        "__function" => Anonymity::Displayed("(Anonymous function)"),
        // Upstream's `getDisplayName` prints a declaration whose name could not
        // be read as `(Missing)`.
        "__missing" => Anonymity::Displayed("(Missing)"),
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
/// `pub` rather than `pub(crate)` so that probes under `examples/` pair units
/// against baseline sections with **this** function rather than a hand-rolled
/// string compare. The 18 cases above are the argument: a probe that mis-pairs
/// units silently attributes lines to the wrong file, which is the same class of
/// drift `types_producer::gap_reason` is kept in step to avoid.
///
/// `#[must_use]` because `must_use_candidate` fires only on *public* functions,
/// so widening the visibility above is what surfaced it — a lint appearing on an
/// unchanged body is worth a word rather than a silent attribute.
#[must_use]
pub fn same_unit(unit: &str, baseline: &str) -> bool {
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
    let decoded = decode_unicode_escapes(name);
    let name: &str = &decoded;
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

/// A name with its `\uXXXX` and `\u{…}` escapes resolved to the characters they
/// spell.
///
/// The baseline reproduces the *source* spelling of an identifier, so
/// `var \u0061;` prints as `\u0061` while the symbol is called `a` — the scanner
/// resolves the escape, as it must for `\u0061` and `a` to be the same variable.
/// This is the same reduction the quote handling above performs: a spelling of a
/// value becomes the value. An escape that does not resolve is left exactly as
/// written, since it is then not an escape.
fn decode_unicode_escapes(name: &str) -> String {
    if !name.contains("\\u") {
        return name.to_string();
    }
    let mut out = String::with_capacity(name.len());
    let mut rest = name;
    while let Some(at) = rest.find("\\u") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        let (digits, remainder) = if let Some(braced) = after.strip_prefix('{') {
            match braced.find('}') {
                Some(close) => (&braced[..close], &braced[close + 1..]),
                None => ("", after),
            }
        } else if after.len() >= 4 {
            after.split_at(4)
        } else {
            ("", after)
        };
        if let Some(resolved) = u32::from_str_radix(digits, 16).ok().and_then(char::from_u32) {
            out.push(resolved);
            rest = remainder;
        } else {
            // Not an escape after all — `\u` followed by something else is just
            // those characters, which is what `invalidUnicodeEscapeSequance4`
            // tests.
            out.push_str("\\u");
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// The value inside `[…]` when it is a literal rather than an expression.
///
/// The test has to be that the text is *entirely* one literal, not that it looks
/// like one at the start. `["+" + bar]` begins and ends with a quote and
/// `[0 + 1]` begins with a digit, and reducing either to a member name turns a
/// computed name — which the binder deliberately leaves late-bound — into a
/// spelling that would silently match something else. Measured 2026-08-04:
/// requiring the whole text is worth 18 conformance cases.
fn static_bracket_name(inside: &str) -> Option<&str> {
    // Either quote: the baseline reproduces the source spelling, and `C['a']`
    // and `C["a"]` name the same member. The quote must not recur inside, or
    // `"a" + "b"` would read as the string `a" + "b`.
    for quote in ['"', '\''] {
        if inside.len() >= 2
            && inside.starts_with(quote)
            && inside.ends_with(quote)
            && !inside[1..inside.len() - 1].contains(quote)
        {
            return Some(&inside[1..inside.len() - 1]);
        }
    }
    // A numeric literal in any spelling — `2.0`, `0b11`, `1e3`. A digit first
    // and nothing but literal characters after: `A[A.p1]`, `A[a]` and `A[1 << 6]`
    // are computed and must keep their brackets.
    if inside.starts_with(|c: char| c.is_ascii_digit()) && is_numeric_literal(inside) {
        return Some(inside);
    }
    None
}

/// Whether the whole text is one numeric literal.
///
/// `2.0`, `0b11`, `1e3`, `11e-1`, `1_000`. A sign counts only as an exponent's,
/// which is what separates `11e-1` from the expression `11 - 1`.
fn is_numeric_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.iter().enumerate().all(|(index, byte)| {
        byte.is_ascii_alphanumeric()
            || *byte == b'.'
            || *byte == b'_'
            || (matches!(byte, b'+' | b'-') && index > 0 && matches!(bytes[index - 1], b'e' | b'E'))
    })
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
    fn an_expression_that_starts_like_a_literal_is_still_an_expression() {
        // The whole text has to be one literal. Reducing these would invent a
        // member name for something the binder leaves late-bound.
        assert_eq!(normalise_symbol_name("[0 + 1]"), "[0 + 1]");
        assert_eq!(normalise_symbol_name("C[1 << 6]"), "C[1 << 6]");
        assert_eq!(normalise_symbol_name("[\"+\" + bar]"), "[\"+\" + bar]");
        // Genuine literals still reduce.
        assert_eq!(normalise_symbol_name("C[0b11]"), "C.0b11");
        assert_eq!(normalise_symbol_name("C[1e3]"), "C.1e3");
        // A sign belongs to an exponent, and only there.
        assert_eq!(normalise_symbol_name("Nums[11e-1]"), "Nums.11e-1");
        assert_eq!(normalise_symbol_name("[11 - 1]"), "[11 - 1]");
    }

    #[test]
    fn a_computed_name_is_left_alone() {
        // Late-bound: we create no symbol for these, and normalising them would
        // turn a real gap into a passing case.
        assert_eq!(normalise_symbol_name("A[A.p1]"), "A[A.p1]");
        assert_eq!(normalise_symbol_name("Result[Symbol.iterator]"), "Result[Symbol.iterator]");
        assert_eq!(normalise_symbol_name("[foo()]"), "[foo()]");
    }

    /// Bind a snippet and return every name the symbol called `name` displays as.
    fn qualified_all(source: &str, name: &str) -> Vec<String> {
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "the snippet should parse cleanly");
        let bound = tsr_binder::bind(
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: source },
        );
        let mut names_by_declaration: NamesByDeclaration<'_> = NamesByDeclaration::default();
        for (_, symbol) in bound.symbols().iter() {
            if symbol.name == INTERNAL_DEFAULT {
                continue;
            }
            for declaration in &symbol.declarations {
                names_by_declaration.entry(*declaration).or_default().push(symbol.name);
            }
        }
        let (id, _) = bound
            .symbols()
            .iter()
            .find(|(_, symbol)| symbol.name == name)
            .unwrap_or_else(|| panic!("no symbol named {name}"));
        display_names(&bound, &parsed.nodes, id, &names_by_declaration, source)
    }

    /// The single name a symbol displays as, for the cases that have only one.
    fn qualified(source: &str, name: &str) -> String {
        let names = qualified_all(source, name);
        assert_eq!(names.len(), 1, "expected one spelling, got {names:?}");
        names.into_iter().next().expect("just asserted non-empty")
    }

    #[test]
    fn a_default_export_is_displayed_under_the_name_it_was_written_with() {
        // Upstream's getNameOfSymbolAsWritten prints the declaration's own name
        // for a reference in the same file, and `default` for one from outside.
        // The baselines contain both, so both are offered.
        // Qualified by the module symbol, which the suite's dotted-suffix
        // indexing sees through.
        let names = qualified_all("export default function foo() {}", "default");
        assert!(names.contains(&"test.default".to_string()), "{names:?}");
        assert!(names.contains(&"test.foo".to_string()), "{names:?}");
    }

    #[test]
    fn an_unnamed_default_export_has_only_the_default_spelling() {
        // No local symbol is created for it, so there is no written name to
        // recover — which is also what upstream prints.
        assert_eq!(qualified_all("export default class { m() {} }", "default"), ["test.default"]);
    }

    #[test]
    fn a_default_export_merges_the_declarations_that_share_it() {
        let arena = tsr_core::Arena::new();
        let source = "export default function foo(): void;\nexport default interface Foo {}\n";
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "the snippet should parse cleanly");
        let bound = tsr_binder::bind(
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "a.ts", text: source },
        );
        let (_, default) = bound
            .symbols()
            .iter()
            .find(|(_, symbol)| symbol.name == "default")
            .expect("the file exports a default");
        assert_eq!(default.declarations.len(), 2);
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
    fn an_escaped_identifier_reduces_to_the_characters_it_spells() {
        // `var \u0061;` declares `a`; the baseline prints how it was written.
        assert_eq!(normalise_symbol_name("\\u0061"), "a");
        assert_eq!(normalise_symbol_name("arg\\u0032"), "arg2");
        assert_eq!(normalise_symbol_name("\\u{0061}"), "a");
        assert_eq!(normalise_symbol_name("a\\u{0061}"), "aa");
        // Not an escape: left alone.
        assert_eq!(normalise_symbol_name("u0031a"), "u0031a");
    }

    #[test]
    fn an_ordinary_name_is_unchanged() {
        assert_eq!(normalise_symbol_name("C"), "C");
        assert_eq!(normalise_symbol_name("M.X"), "M.X");
    }
}
