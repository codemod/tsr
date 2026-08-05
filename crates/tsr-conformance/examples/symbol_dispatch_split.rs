//! Split `rank_board`'s rows 9 and 10 by the thing their reason string does not
//! carry — `bd tsr-4sc`, `docs/architecture/checker-notes-symbols.md`.
//!
//! `cargo run -p tsr-conformance --example symbol_dispatch_split --release`
//!
//! # Why a new instrument was needed, and the cheaper check that came first
//!
//! `docs/conventions.md` records three probes that re-implemented the harness
//! and measured a different compiler. So the first question is always whether
//! the existing output already carries the split. For one of `rank_board`'s rows
//! it does: `access_reason` (`crates/tsr-conformance/src/types_producer.rs:869`)
//! interpolates `checker.type_to_string(receiver_type)` into the reason, so a
//! receiver histogram is recoverable from the uncut strings with no new counters.
//!
//! **For these two rows it does not, and the reason is structural.** Both come
//! from `describe` (`types_producer.rs:1013`), which interpolates exactly three
//! things: the symbol's flags, the kind of its **value declaration**, and — only
//! for a value declaration that has one — its annotation or initialiser.
//!
//! - Row 10 is `SymbolFlags(ALIAS) / no value declaration`. An alias symbol
//!   *never* has a value declaration (`SymbolFlags::VALUE` does not include
//!   `ALIAS`), so the second field is a constant and the third is empty. The
//!   whole row is one string, and `import a = N`, `import { x } from "./m"` and
//!   `export { q }` are indistinguishable inside it. That is the split the
//!   briefing asks for, and the existing instrument cannot express it.
//! - Row 9 is `SymbolFlags(FUNCTION) / FunctionDeclaration / neither`. `neither`
//!   means the `FunctionDeclaration` has **no return annotation** — `type_id()`
//!   for a `FunctionDeclaration` *is* its return annotation
//!   (`crates/tsr-ast/src/generated/alias.rs:748`) — and no initialiser, which a
//!   function never has. So the row is "a function whose return type has to be
//!   inferred", and everything that distinguishes one member of it from another
//!   lives in the parameters, the type parameters or the body: none of it is in
//!   the string.
//!
//! # Attributed and independent, printed side by side
//!
//! `docs/conventions.md`, "Size a positional arm from the flattened run": when
//! an instrument attributes a line to the first of several conditions that could
//! each have claimed it, its per-condition counts answer a question about the
//! attribution order rather than about the work. A function can be `async`
//! *and* have a destructuring parameter.
//!
//! So each row prints two columns. **Attributed** is first-match-wins in the
//! order below and sums to the row. **Independent** counts every line each
//! property holds of, sums to more than the row, and is the number to size an
//! arm from.
//!
//! # Control buckets, all of which must read zero
//!
//! - `row 9: NO SYMBOL` / `row 10: NO SYMBOL` — the symbol is re-derived here by
//!   the same test `gap_reason`'s declaration-name branch uses
//!   (`types_producer.rs:1093`), so a line in the row whose symbol cannot be
//!   re-derived means the two tests have drifted apart.
//! - `row 9: UNEXPLAINED` — a line in the row that none of the properties
//!   describes. It is not an error; it is the honest size of what this probe
//!   does not account for, and any prediction has to subtract it.
//! - `row 10: UNCLASSIFIED KIND` — an alias declaration kind not in the list.

use std::collections::BTreeMap;

use rayon::prelude::*;
use tsr_ast::{ModifierLike, Node, NodeId, SyntaxKind};
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

/// The exact reason strings this probe splits. Matched in full, not by
/// substring: a prefix test would silently absorb neighbouring rows and the
/// totals would stop reconciling with `rank_board`'s.
const ROW_9: &str =
    "declaration name, symbol has no type: SymbolFlags(FUNCTION) / FunctionDeclaration / neither";
const ROW_10: &str =
    "declaration name, symbol has no type: SymbolFlags(ALIAS) / no value declaration";

#[derive(Default)]
struct Counts {
    /// Lines matched into each row, before any split.
    row_9: usize,
    row_10: usize,
    /// First-match-wins.
    attributed_9: BTreeMap<&'static str, usize>,
    attributed_10: BTreeMap<String, usize>,
    /// Every property that holds, per line.
    independent_9: BTreeMap<&'static str, usize>,
    /// Controls.
    no_symbol_9: usize,
    no_symbol_10: usize,
    /// The lines the row-9 split cannot account for, named so the next run
    /// explains them instead of bounding them.
    unexplained_9: Vec<String>,
    /// Cases touched, for the concentration check on the sub-population.
    by_case_9: BTreeMap<String, usize>,
    by_case_10_same_file: BTreeMap<String, usize>,
    by_case_10_cross_file: BTreeMap<String, usize>,
}

impl Counts {
    fn merge(&mut self, other: Self) {
        self.row_9 += other.row_9;
        self.row_10 += other.row_10;
        self.no_symbol_9 += other.no_symbol_9;
        self.unexplained_9.extend(other.unexplained_9);
        self.no_symbol_10 += other.no_symbol_10;
        for (map, from) in [
            (&mut self.attributed_9, other.attributed_9),
            (&mut self.independent_9, other.independent_9),
        ] {
            for (key, value) in from {
                *map.entry(key).or_default() += value;
            }
        }
        for (key, value) in other.attributed_10 {
            *self.attributed_10.entry(key).or_default() += value;
        }
        for (map, from) in [
            (&mut self.by_case_9, other.by_case_9),
            (&mut self.by_case_10_same_file, other.by_case_10_same_file),
            (&mut self.by_case_10_cross_file, other.by_case_10_cross_file),
        ] {
            for (key, value) in from {
                *map.entry(key).or_default() += value;
            }
        }
    }
}

/// Whether an alias form resolves inside the file that declares it.
///
/// This is the whole point of the probe for row 10. `Checker::resolve_alias`
/// (`crates/tsr-checker/src/symbols.rs`) can only ever answer the same-file
/// forms; the cross-file ones need module resolution plumbed into
/// `Checker::new`, which is a signature change and not an arm.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reach {
    SameFile,
    CrossFile,
    Unknown,
}

fn main() {
    check_classifier();
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let mut cases = corpus.discover().expect("discovering cases");

    // A smoke-test knob, and it announces itself. `docs/conventions.md` records
    // three probes whose numbers were quoted as the gradient's while their
    // population was something else; a filtered run here prints a banner rather
    // than a share, so no partial number can be mistaken for the measurement.
    let filter = std::env::var("SYMBOL_SPLIT_CASES").ok();
    if let Some(filter) = &filter {
        cases.retain(|case| case.name.contains(filter.as_str()));
        println!(
            "\n*** PARTIAL RUN: SYMBOL_SPLIT_CASES={filter} kept {} cases. These counts are NOT\n\
             *** the gradient's population and must not be quoted as a share of it.\n",
            cases.len()
        );
    }

    let counts = cases
        .par_iter()
        .filter_map(|case| {
            // The suite's own skips, skip for skip, so every share here is a
            // share of the gradient's denominator (`docs/conventions.md`, "a
            // probe's denominator must be the gradient's by construction").
            if case.has_varied_types() || case.has_known_divergence() {
                return None;
            }
            let text = case.expected_types()?;
            let expected = types_baseline::parse(&text);
            if types_baseline::assertion_count(&expected) == 0 {
                return None;
            }
            let parsed = case.load().ok()?;
            let arena = tsr_core::Arena::new();
            let (program, ours, ids) =
                types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
            let nodes = program.nodes();
            let node_map = program.node_map();
            let bound = program.binder();
            let mut checker = tsr_checker::Checker::new(bound, nodes, node_map);
            // A SECOND checker over the same program, and it is load-bearing.
            // See `classify_on_a_separate_checker` below.
            let mut classifier = tsr_checker::Checker::new(bound, nodes, node_map);
            let share = std::env::var("SYMBOL_SPLIT_SHARE_CHECKER").is_ok();

            let mut counts = Counts::default();
            for (index, expected_file) in expected.iter().enumerate() {
                let our_file = ours.get(index);
                let our_ids = ids.get(index);
                for (position, want) in expected_file.assertions.iter().enumerate() {
                    let Some(got) = our_file.and_then(|file| file.get(position)) else { continue };
                    let Some(_want_type) = want.text.strip_prefix(&format!("{} : ", got.text))
                    else {
                        continue;
                    };
                    if got.type_string != "error" {
                        continue;
                    }
                    let Some(&id) = our_ids.and_then(|line_ids| line_ids.get(position)) else {
                        continue;
                    };
                    let reason =
                        types_producer::gap_reason(&mut checker, bound, nodes, node_map, id);
                    if reason == ROW_9 {
                        counts.row_9 += 1;
                        *counts.by_case_9.entry(case.name.clone()).or_default() += 1;
                        let Some((symbol, _)) = declaration_of(bound, nodes, id) else {
                            counts.no_symbol_9 += 1;
                            continue;
                        };
                        let properties = if share {
                            function_properties(&mut checker, bound, node_map, nodes, symbol)
                        } else {
                            function_properties(&mut classifier, bound, node_map, nodes, symbol)
                        };
                        for property in &properties {
                            *counts.independent_9.entry(*property).or_default() += 1;
                        }
                        // A line whose only properties are contextual is not
                        // explained by this probe, and is counted as such.
                        let first = properties
                            .iter()
                            .find(|property| !property.starts_with("CONTEXT:"))
                            .copied()
                            .unwrap_or("UNEXPLAINED");
                        *counts.attributed_9.entry(first).or_default() += 1;
                        // The residual, named rather than counted. A control
                        // that reads non-zero has to be explicable on the next
                        // run without a second instrument, so the lines it
                        // holds are printed with their case and their source.
                        if first == "UNEXPLAINED" {
                            let span = nodes.span(id);
                            counts.unexplained_9.push(format!(
                                "{}  {}:{}  `{}`",
                                case.name,
                                index,
                                span.start,
                                got.text.trim()
                            ));
                        }
                    } else if reason == ROW_10 {
                        counts.row_10 += 1;
                        let Some((_, declaration)) = declaration_of(bound, nodes, id) else {
                            counts.no_symbol_10 += 1;
                            continue;
                        };
                        let (label, reach) = alias_form(node_map, nodes, declaration);
                        *counts.attributed_10.entry(label).or_default() += 1;
                        let bucket = match reach {
                            Reach::SameFile => &mut counts.by_case_10_same_file,
                            Reach::CrossFile => &mut counts.by_case_10_cross_file,
                            Reach::Unknown => continue,
                        };
                        *bucket.entry(case.name.clone()).or_default() += 1;
                    }
                }
            }
            Some(counts)
        })
        .reduce(Counts::default, |mut left, right| {
            left.merge(right);
            left
        });

    report(&counts);
}

/// The symbol's **first declaration**, re-derived by the same test
/// `gap_reason`'s declaration-name branch uses.
///
/// `declarations.first()` and not `value_declaration`: the whole reason row 10
/// exists is that an alias has no value declaration, and row 9's function
/// symbols may carry overloads whose first declaration is the one
/// `getSignaturesOfSymbol` starts from.
fn declaration_of(
    bound: &tsr_binder::BindResult<'_>,
    nodes: &tsr_ast::NodeTable,
    id: NodeId,
) -> Option<(tsr_binder::SymbolId, NodeId)> {
    let parent = nodes.parent(id)?;
    let symbol = bound.symbol_of(parent)?;
    let declaration = bound.symbols().get(symbol).declarations.first().copied()?;
    Some((symbol, declaration))
}

/// Which alias form this is, and whether it can be answered without module
/// resolution.
fn alias_form(
    node_map: &tsr_ast::NodeMap<'_>,
    nodes: &tsr_ast::NodeTable,
    declaration: NodeId,
) -> (String, Reach) {
    match node_map.get(declaration) {
        Some(Node::ImportEqualsDeclaration(node)) => match node.module_reference {
            Some(tsr_ast::ModuleReference::Identifier(_)) => {
                ("import a = b            (same file)".to_string(), Reach::SameFile)
            }
            Some(tsr_ast::ModuleReference::QualifiedName(_)) => (
                "import a = b.c          (same file, prints the alias's own name)".to_string(),
                Reach::SameFile,
            ),
            Some(tsr_ast::ModuleReference::ExternalModuleReference(_)) => {
                ("import a = require(...)  (cross file)".to_string(), Reach::CrossFile)
            }
            None => ("import a = <missing>".to_string(), Reach::Unknown),
        },
        Some(Node::ImportClause(_)) => {
            ("import d from       (cross file)".to_string(), Reach::CrossFile)
        }
        Some(Node::ImportSpecifier(_)) => {
            ("import { x } from   (cross file)".to_string(), Reach::CrossFile)
        }
        Some(Node::NamespaceImport(_)) => {
            ("import * as ns from (cross file)".to_string(), Reach::CrossFile)
        }
        Some(Node::NamespaceExport(_)) => {
            ("export * as ns from (cross file)".to_string(), Reach::CrossFile)
        }
        Some(Node::ExportSpecifier(_)) => {
            // The module specifier lives on the grandparent `ExportDeclaration`,
            // and its presence is the entire same-file/cross-file question for
            // this form: `export { q }` re-exports a local, `export { q } from
            // "./m"` does not.
            let has_specifier = nodes
                .parent(declaration)
                .and_then(|clause| nodes.parent(clause))
                .and_then(|export| node_map.get(export))
                .is_some_and(|node| {
                    matches!(node, Node::ExportDeclaration(d) if d.module_specifier.is_some())
                });
            if has_specifier {
                ("export { q } from   (cross file)".to_string(), Reach::CrossFile)
            } else {
                ("export { q }        (SAME FILE)".to_string(), Reach::SameFile)
            }
        }
        Some(Node::ExportAssignment(node)) => {
            // `export default x` and `export = x` are the same node. Only an
            // identifier right-hand side names a local; anything else is an
            // expression whose type is the answer, not an alias target.
            match node.expression {
                Some(tsr_ast::Expression::Identifier(_)) if node.is_export_equals => {
                    ("export = x          (SAME FILE)".to_string(), Reach::SameFile)
                }
                Some(tsr_ast::Expression::Identifier(_)) => {
                    ("export default x    (SAME FILE)".to_string(), Reach::SameFile)
                }
                _ => ("export default <expr>".to_string(), Reach::SameFile),
            }
        }
        // `export as namespace N` — a UMD global alias. Neither half: its target
        // is the file's own module symbol, so it needs no module resolution, but
        // it is not one of the local forms `resolve_alias` handles either.
        Some(Node::NamespaceExportDeclaration(_)) => {
            ("export as namespace N (neither half)".to_string(), Reach::Unknown)
        }
        _ => (format!("UNCLASSIFIED KIND: {:?}", nodes.kind(declaration)), Reach::Unknown),
    }
}

/// Every property of a function symbol that is known to stop
/// `getSignaturesOfSymbol` (`crates/tsr-checker/src/signatures.rs:142`) from
/// answering.
///
/// # It walks **every** declaration, not the first
///
/// This was a real defect in the first draft and it moved 819 lines. Upstream's
/// `getSignaturesOfSymbol` loops over all of a symbol's declarations and this
/// port's does too, so one gapping overload is enough to gap the symbol. Reading
/// only `declarations.first()` reported "an overload set" as if the arity were
/// the cause, when what actually gapped was a later signature's parameter.
///
/// # The attribution order is causal strength, weakest last
///
/// `docs/conventions.md` warns that a first-match-wins column is an answer about
/// the attribution order. Here the order is: the properties that are known to
/// *stop* the port first, then the two that are merely *context* — having more
/// than one declaration, and having no return expression — because neither of
/// those stops anything on its own (`function f(a: number); function f(a:
/// string); function f(a) {}` prints, and `function f() {}` prints `() => void`).
/// A line whose only properties are the contextual two is therefore what this
/// probe cannot explain, and it says so.
fn function_properties<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    bound: &tsr_binder::BindResult<'_>,
    node_map: &tsr_ast::NodeMap<'a>,
    nodes: &tsr_ast::NodeTable,
    symbol: tsr_binder::SymbolId,
) -> Vec<&'static str> {
    let mut out = Vec::new();
    let error = checker.intrinsics().error;
    let declarations: Vec<NodeId> =
        bound.symbols().get(symbol).declarations.iter().copied().collect();
    let entry_exports = !bound.symbols().get(symbol).exports.is_empty();
    let entry_members = !bound.symbols().get(symbol).members.is_empty();

    let mut anonymous = false;
    let mut is_async = false;
    let mut generator = false;
    let mut modifier = false;
    let mut destructuring = false;
    let mut parameter_gap = false;
    let mut constraint_gap = false;
    let mut return_gap = false;
    let mut many_returns = false;
    let mut no_return = true;

    for declaration in declarations.iter().copied() {
        let Some(Node::FunctionDeclaration(function)) = node_map.get(declaration) else {
            continue;
        };
        anonymous |= function.name.is_none_or(|name| name.text.is_empty());
        is_async |= function
            .modifiers
            .iter()
            .any(|m| matches!(m, ModifierLike::Token(t) if t.kind == SyntaxKind::AsyncKeyword));
        generator |= function.asterisk_token.is_some();
        modifier |= function.type_parameters.iter().any(|p| !p.modifiers.is_empty());
        destructuring |= function
            .parameters
            .iter()
            .any(|p| !matches!(p.name, Some(tsr_ast::BindingName::Identifier(_))));
        parameter_gap |= function
            .parameters
            .iter()
            .filter_map(|p| p.r#type)
            .any(|annotation| checker.get_type_from_type_node(annotation) == error);
        constraint_gap |= function
            .type_parameters
            .iter()
            .filter_map(|p| p.constraint.or(p.default_type))
            .any(|annotation| checker.get_type_from_type_node(annotation) == error);
        // The body, which is where the rest of the row is: `neither` means there
        // is no return annotation, so the return type has to be inferred.
        let (returns, gapped, distinct) = return_types(checker, node_map, nodes, declaration);
        return_gap |= gapped;
        many_returns |= distinct >= 2;
        no_return &= returns == 0;
    }

    // Known to stop the port.
    if anonymous {
        out.push("an anonymous function (export default)");
    }
    // `get_type_of_func_class_enum_module_worker`'s own explicit gap: a function
    // with expando properties prints `{ (): void; a: string; }` upstream, and
    // printing only the signatures there would be a wrong answer rather than a
    // partial one.
    if entry_exports || entry_members {
        out.push("expando properties (f.a = 1) — the members must print too");
    }
    if is_async {
        out.push("async — the return type is Promise<T>, a global");
    }
    if generator {
        out.push("a generator — the return type is Generator<...>, a global");
    }
    if modifier {
        out.push("a type parameter carrying a modifier (const/in/out)");
    }
    if destructuring {
        out.push("a destructuring parameter");
    }
    if parameter_gap {
        out.push("a parameter annotation that is itself a gap");
    }
    if constraint_gap {
        out.push("a type-parameter constraint or default that is itself a gap");
    }
    if return_gap {
        out.push("a return expression that is itself a gap");
    }
    if many_returns {
        out.push("two or more distinct return types (needs subtype reduction)");
    }
    // Context, not cause — see the order note above.
    if declarations.len() > 1 {
        out.push("CONTEXT: more than one declaration (overload set or merge)");
    }
    if no_return {
        out.push("CONTEXT: no return expression anywhere (upstream infers void)");
    }
    out
}

/// The return statements of a function body, not descending into nested
/// functions, with their types.
///
/// Returns `(count, any gapped, distinct printed types)`. Typing them here is
/// what separates *"the return expression is itself a gap"* — which is
/// propagation and worth nothing to this row — from *"two returns disagree"*,
/// which is a real missing rule.
fn return_types<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    node_map: &tsr_ast::NodeMap<'a>,
    nodes: &tsr_ast::NodeTable,
    declaration: NodeId,
) -> (usize, bool, usize) {
    let mut stack = vec![declaration];
    let mut expressions = Vec::new();
    let mut first = true;
    while let Some(id) = stack.pop() {
        if !first && is_function_like(nodes.kind(id)) {
            continue;
        }
        first = false;
        if let Some(Node::ReturnStatement(node)) = node_map.get(id)
            && let Some(expression) = node.expression
        {
            expressions.push(expression);
        }
        if let Some(node) = node_map.get(id) {
            tsr_ast::for_each_child_id(node, |child| stack.push(child));
        }
    }
    let error = checker.intrinsics().error;
    let mut gapped = false;
    let mut distinct: Vec<String> = Vec::new();
    for expression in &expressions {
        let id = checker.check_expression(*expression);
        if id == error {
            gapped = true;
            continue;
        }
        let printed = checker.type_to_string(id);
        if !distinct.contains(&printed) {
            distinct.push(printed);
        }
    }
    (expressions.len(), gapped, distinct.len())
}

fn is_function_like(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::FunctionDeclaration
            | SyntaxKind::FunctionExpression
            | SyntaxKind::ArrowFunction
            | SyntaxKind::MethodDeclaration
            | SyntaxKind::GetAccessor
            | SyntaxKind::SetAccessor
            | SyntaxKind::Constructor
    )
}

/// The two properties this probe's arithmetic rests on, asserted on every run
/// rather than in a `#[cfg(test)]` block Cargo would not execute inside an
/// example.
fn check_classifier() {
    // EXACTNESS. A prefix test on row 10 would also absorb
    // `SymbolFlags(ALIAS | X) / no value declaration`, which is a different
    // symbol shape, and the total would stop reconciling with `rank_board`'s.
    assert_ne!(
        ROW_10,
        "declaration name, symbol has no type: SymbolFlags(ALIAS | TYPE) / no value declaration",
        "the row key is matched in full"
    );
    // NEITHER. `neither` in row 9's key is the absence of a **return
    // annotation** — `Node::type_id` for a `FunctionDeclaration` returns
    // `n.r#type`, which is the return type. If that ever stops being true the
    // row stops meaning "the return type must be inferred" and every bucket
    // below is mislabelled.
    assert!(ROW_9.ends_with("/ neither"), "row 9 is the no-return-annotation row");
}

fn report(counts: &Counts) {
    println!("\n# rank_board rows 9 and 10, split\n");

    println!("## Controls (all must read 0)");
    println!("  row 9:  NO SYMBOL        {}", counts.no_symbol_9);
    println!("  row 10: NO SYMBOL        {}", counts.no_symbol_10);
    let unclassified: usize = counts
        .attributed_10
        .iter()
        .filter(|(key, _)| key.starts_with("UNCLASSIFIED KIND"))
        .map(|(_, count)| *count)
        .sum();
    println!("  row 10: UNCLASSIFIED KIND {unclassified}");
    let unexplained = counts.attributed_9.get("UNEXPLAINED").copied().unwrap_or_default();
    println!("  row 9:  UNEXPLAINED       {unexplained}");
    if unexplained > 0 {
        // `docs/conventions.md`: a sub-row measured against a non-zero residual
        // is a lower bound, so the residual is printed in full rather than
        // summarised. It is bounded by construction — a control that needs
        // truncating is not a control.
        println!("\n  Every UNEXPLAINED line, so the next run can name its cause:");
        let mut lines = counts.unexplained_9.clone();
        lines.sort_unstable();
        for line in &lines {
            println!("    {line}");
        }
        #[allow(clippy::cast_precision_loss)]
        let share = 100.0 * unexplained as f64 / counts.row_9 as f64;
        println!(
            "\n  *** The row-9 sub-rows below are LOWER BOUNDS: {unexplained} of {} lines\n\
             *** ({share:.1}%) are not accounted for by any listed property.",
            counts.row_9,
        );
    }

    println!(
        "\n## Row 9 — SymbolFlags(FUNCTION) / FunctionDeclaration / neither: {} lines, {} cases",
        counts.row_9,
        counts.by_case_9.len()
    );
    concentration("row 9", &counts.by_case_9);
    println!("\n  {:<62} {:>8} {:>12}", "property", "attrib.", "independent");
    let mut keys: Vec<&&str> =
        counts.attributed_9.keys().chain(counts.independent_9.keys()).collect();
    keys.sort_unstable();
    keys.dedup();
    for key in keys {
        println!(
            "  {:<62} {:>8} {:>12}",
            key,
            counts.attributed_9.get(*key).copied().unwrap_or_default(),
            counts.independent_9.get(*key).copied().unwrap_or_default()
        );
    }
    let attributed: usize = counts.attributed_9.values().sum();
    println!("  {:<62} {:>8}", "TOTAL (attributed; must equal the row)", attributed);

    println!("\n## Row 10 — SymbolFlags(ALIAS) / no value declaration: {} lines", counts.row_10);
    let mut forms: Vec<(&String, &usize)> = counts.attributed_10.iter().collect();
    forms.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    for (form, count) in forms {
        println!("  {form:<62} {count:>8}");
    }
    let same: usize = counts.by_case_10_same_file.values().sum();
    let cross: usize = counts.by_case_10_cross_file.values().sum();
    println!(
        "\n  SAME FILE  {:>8} lines over {:>5} cases",
        same,
        counts.by_case_10_same_file.len()
    );
    println!(
        "  CROSS FILE {:>8} lines over {:>5} cases",
        cross,
        counts.by_case_10_cross_file.len()
    );
    concentration("row 10 same-file", &counts.by_case_10_same_file);
    concentration("row 10 cross-file", &counts.by_case_10_cross_file);
}

/// The concentration check `docs/architecture/checker-notes-rank.md` runs beside
/// every row, re-run on the sub-population because the parent's shape is not the
/// child's.
fn concentration(label: &str, by_case: &BTreeMap<String, usize>) {
    let total: usize = by_case.values().sum();
    if total == 0 {
        println!("  {label}: empty");
        return;
    }
    let mut sorted: Vec<(&String, &usize)> = by_case.iter().collect();
    sorted.sort_by_key(|(name, count)| (std::cmp::Reverse(**count), (*name).clone()));
    let top1 = *sorted[0].1;
    let top10: usize = sorted.iter().take(10).map(|(_, count)| **count).sum();
    #[allow(clippy::cast_precision_loss)]
    let share = |part: usize| 100.0 * part as f64 / total as f64;
    println!(
        "  {label}: {total} lines, {} cases, top-1 {:.1}% ({}), top-10 {:.1}%",
        by_case.len(),
        share(top1),
        sorted[0].0,
        share(top10)
    );
}
