//! Rendering our types the way upstream's `.types` baseline writer does.
//!
//! Ported from `internal/testutil/tsbaseline/type_symbol_baseline.go`. The spec
//! and the reasoning are in `docs/architecture/checker-oracle.md`; this module is
//! the implementation.
//!
//! # The walker is measured separately from the checker, on purpose
//!
//! A wrong walker and a wrong checker are indistinguishable in the line gradient
//! — both simply fail to match, and the number cannot say which is at fault. So
//! [`Assertion`] carries the expression text apart from the type, and
//! [`walker_agreement`] compares only the text, by testing whether upstream's
//! line *starts with* `{our text} : `. That is sound even though the line cannot
//! be split — the expression may itself contain `" : "` — and it needs no type at
//! all.
//!
//! **Get that number high before believing anything about types.**

use tsr_ast::{Node, NodeId, NodeMap, NodeTable, SyntaxKind, Tree, predicates, push_children};

use crate::types_baseline::{FileTypes, TypeAssertion};

/// One `>expression : type` line, with the halves still apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assertion {
    /// The node's source text, as upstream's `sourceText` — the raw slice from
    /// the post-trivia position to the node's end, with line breaks removed.
    pub text: String,
    /// The rendered type.
    pub type_string: String,
    /// The node the line came from. Carried for `examples/types_shapes.rs`,
    /// which ranks the checker's remaining work by *what* we failed on — the
    /// answer's shape alone cannot do that, because `f()` → `string` is an
    /// intrinsic answer that needs call resolution.
    pub kind: SyntaxKind,
    /// Why the answer was `error`, when [`assertions_for_case`] was asked to
    /// explain and it was. `None` otherwise — including for every run of the
    /// suite, which does not pay for it.
    pub reason: Option<String>,
}

impl Assertion {
    /// The whole line, as a `.types` baseline writes it after the `>`.
    #[must_use]
    pub fn line(&self) -> String {
        format!("{} : {}", self.text, self.type_string)
    }
}

/// Every node upstream's baseline writer would visit, in its order.
///
/// Ported from `forEachASTNode` (`type_symbol_baseline.go:318`): a pre-order DFS
/// with children pushed reversed, so they come back out in source order.
///
/// **Upstream walks with `ForEachChild`; this walks with `push_children`**, which
/// is generated from `ast.json` and additionally yields token-valued fields —
/// 0.41% more nodes, measured in
/// [ADR-0033](../../../docs/adr/0033-the-parser-fills-the-node-map.md). The
/// selection predicates reject those (a `case` keyword is not an expression, an
/// identifier, or a declaration name), so the *kept* set should be identical.
/// That is an assumption, and [`walker_agreement`] is what measures it rather
/// than trusting it.
fn walk_in_order(root: Node<'_>) -> Vec<Node<'_>> {
    let mut result = Vec::new();
    let mut work = vec![root];
    let mut children = Vec::new();
    while let Some(node) = work.pop() {
        result.push(node);
        children.clear();
        push_children(node, &mut children);
        children.reverse();
        work.extend(children.iter().copied());
    }
    result
}

/// Whether the baseline writer emits a line for this node.
///
/// Two stages, exactly as upstream splits them: `visitNode` (`:304`) keeps
/// expressions, identifiers and declaration names, and `writeTypeOrSymbol`
/// (`:345`) then drops the ones that are part of a type or an omitted expression.
///
/// The third of upstream's drops — an identifier whose parent's
/// `GetMeaningFromDeclaration` carries no *value* meaning — is **not ported
/// here**; see [`selects`]'s note.
fn selects(id: NodeId, tree: Tree<'_, '_>) -> bool {
    let kind = tree.kind(id);
    let kept = predicates::is_expression_node(id, tree)
        || kind == SyntaxKind::Identifier
        || predicates::is_declaration_name(id, tree);
    if !kept {
        return false;
    }
    if predicates::is_part_of_type_node(id, tree) || kind == SyntaxKind::OmittedExpression {
        return false;
    }
    // Upstream's third drop (`:361`): an identifier whose parent declares no
    // *value* gets no line — which is what keeps `interface I`, an import's
    // names, and a type parameter out of a `.types` baseline while `const x`
    // stays in.
    //
    // The exception is deliberate and upstream's: a **type alias's own name** is
    // kept even though `type T` means only a type, *"because that may evaluate to
    // some interesting type"*.
    if kind == SyntaxKind::Identifier
        && let Some(parent) = tree.parent(id)
    {
        let meaning = predicates::meaning_from_declaration(parent, tree);
        let names_a_type_alias = tree.kind(parent) == SyntaxKind::TypeAliasDeclaration
            && tree.node(parent).and_then(|n| n.name_id()) == Some(id);
        if !meaning.contains(predicates::SemanticMeaning::VALUE) && !names_a_type_alias {
            return false;
        }
    }
    true
}

/// The source text of a node, as upstream's `sourceText`.
///
/// `GetSourceTextOfNodeFromSourceFile(file, node, includeTrivia: false)` is the
/// slice from the node's post-trivia start to its end; `iterateBaseline` then
/// strips line delimiters from it (`:231`).
fn source_text(source: &str, span: tsr_core::Span) -> String {
    let start = skip_trivia(source, span.start as usize);
    let end = (span.end as usize).min(source.len());
    let raw = source.get(start..end).unwrap_or_default();
    raw.replace(['\r', '\n'], "")
}

/// Advance past whitespace and comments, as `scanner.SkipTrivia` does.
fn skip_trivia(source: &str, mut pos: usize) -> usize {
    let bytes = source.as_bytes();
    while pos < bytes.len() {
        match bytes[pos] {
            b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c => pos += 1,
            b'/' if bytes.get(pos + 1) == Some(&b'/') => {
                pos += 2;
                while pos < bytes.len() && bytes[pos] != b'\n' {
                    pos += 1;
                }
            }
            b'/' if bytes.get(pos + 1) == Some(&b'*') => {
                pos += 2;
                while pos < bytes.len()
                    && !(bytes[pos] == b'*' && bytes.get(pos + 1) == Some(&b'/'))
                {
                    pos += 1;
                }
                pos = (pos + 2).min(bytes.len());
            }
            _ => break,
        }
    }
    pos
}

/// Produce the assertion lines for one file.
///
/// `type_of` renders a node's type; it is a parameter so the walker can be
/// measured with a stub, which is what [`walker_agreement`] does.
pub fn assertions_for_file(
    file: &Node<'_>,
    source: &str,
    nodes: &NodeTable,
    map: &NodeMap<'_>,
    mut type_of: impl FnMut(NodeId) -> String,
) -> Vec<Assertion> {
    let tree = Tree { nodes, map };
    let mut out = Vec::new();
    for node in walk_in_order(*file) {
        let Some(id) = node.node_id() else { continue };
        if !selects(id, tree) {
            continue;
        }
        let text = source_text(source, nodes.span(id));
        let type_string = type_of(id);
        out.push(Assertion { text, type_string, kind: tree.kind(id), reason: None });
    }
    out
}

/// The type a node has, as upstream's `GetTypeAtLocation` would answer it.
///
/// Three cases of `getTypeOfNode` (`checker.go:31927`), in upstream's order,
/// which is load-bearing:
///
/// - a **type declaration's own name** takes `getDeclaredTypeOfSymbol`, so
///   `class A {}` records `>A : A` — the *instance* type, not `typeof A`;
/// - any other **declaration name** takes `getTypeOfSymbol`;
/// - an **expression** takes `checkExpression`.
///
/// The first two are the pair that is easy to collapse and must not be: a class
/// symbol declares `A` and has `typeof A`, and testing the general
/// declaration-name branch first would answer every class name with the wrong
/// one of them.
///
/// Anything else is `errorType` — a gap rather than an answer, exactly as it is
/// inside the checker.
pub fn type_at_location(
    checker: &mut tsr_checker::Checker<'_, '_>,
    binder: &tsr_binder::BindResult<'_>,
    nodes: &NodeTable,
    map: &NodeMap<'_>,
    id: NodeId,
) -> String {
    let error = checker.intrinsics().error;
    let Some(node) = map.get(id) else { return checker.type_to_string(error) };

    // `IsTypeDeclarationName` (`ast/utilities.go:3598`): an identifier naming a
    // class, interface, type alias, enum or type parameter. Upstream tests this
    // *before* the general declaration-name branch below.
    if let Some(parent) = nodes.parent(id)
        && nodes.kind(id) == SyntaxKind::Identifier
        && is_type_declaration(nodes.kind(parent))
        && map.get(parent).and_then(|p| p.name_id()) == Some(id)
        && let Some(symbol) = binder.symbol_of(parent)
    {
        let declared = checker.get_declared_type_of_symbol(symbol);
        return checker.type_to_string(declared);
    }

    // A declaration name resolves through its parent's symbol.
    if let Some(parent) = nodes.parent(id)
        && map.get(parent).and_then(|p| p.name_id()) == Some(id)
        && let Some(symbol) = binder.symbol_of(parent)
    {
        let id = checker.get_type_of_symbol(symbol);
        return checker.type_to_string(id);
    }

    if let Ok(expression) = tsr_ast::Expression::try_from(node) {
        let id = checker.check_expression(expression);
        return checker.type_to_string(id);
    }
    checker.type_to_string(error)
}

/// Render every baseline section of a case, in the baseline's order.
///
/// One entry per `expected` section, so position *i* on one side is position *i*
/// on the other, and a section we have no unit for comes back empty rather than
/// absent — the alignment is what [`crate::types_suite::compare`] compares.
///
/// Shared by the suite and by `examples/types_shapes.rs` so the histogram is
/// taken over exactly what the gate judges. Returns [`Assertion`]s with the two
/// halves still apart, because the histogram needs the type alone and the suite
/// needs the whole line; [`to_file_types`] makes the latter.
#[must_use]
pub fn assertions_for_case(
    case: &crate::TestCase,
    expected: &[FileTypes],
    explain: bool,
) -> Vec<Vec<Assertion>> {
    let mut ours = Vec::new();
    for expected_file in expected {
        let Some(unit) = case
            .files
            .iter()
            .find(|u| crate::binder_suite::same_unit(&u.name, &expected_file.file))
        else {
            ours.push(Vec::new());
            continue;
        };
        if tsr_parser::ScriptKind::from_file_name(&unit.name) == tsr_parser::ScriptKind::Json {
            ours.push(Vec::new());
            continue;
        }
        let arena = tsr_core::Arena::new();
        let options = tsr_parser::ParseOptions {
            jsdoc: false,
            ..tsr_parser::ParseOptions::for_file(&unit.name)
        };
        let file = tsr_parser::parse_with_options(&arena, &unit.content, options);
        let bound = tsr_binder::bind(
            file.source_file,
            &file.nodes,
            tsr_binder::FileInfo { name: &unit.name, text: &unit.content },
        );
        let mut checker = tsr_checker::Checker::new(&bound, &file.nodes, &file.node_map);
        let mut gaps = Vec::new();
        let mut rendered = assertions_for_file(
            &Node::SourceFile(file.source_file),
            &unit.content,
            &file.nodes,
            &file.node_map,
            |id| {
                let answer =
                    type_at_location(&mut checker, &bound, &file.nodes, &file.node_map, id);
                if explain && answer == "error" {
                    gaps.push(id);
                }
                answer
            },
        );
        if explain {
            let mut gaps = gaps.into_iter();
            for assertion in &mut rendered {
                if assertion.type_string == "error" {
                    let id = gaps.next().expect("one recorded gap per `error` line");
                    assertion.reason =
                        Some(gap_reason(&mut checker, &bound, &file.nodes, &file.node_map, id));
                }
            }
        }
        ours.push(rendered);
    }
    ours
}

/// Why a *type node* could not be resolved, one level finer than its kind.
///
/// The distinction this exists for is the one that ranks lib files
/// (`bd tsr-9or.1`): an annotation that reads `TypeReference` may be a name with
/// no declaration anywhere — `Array`, `Promise` — or a name that resolves to
/// something whose declared type is unported, or a generic whose arity this port
/// will not guess at. Those are three different pieces of work behind one node
/// kind, and the roll-up cannot tell them apart without this.
fn type_node_reason(
    binder: &tsr_binder::BindResult<'_>,
    nodes: &NodeTable,
    map: &NodeMap<'_>,
    id: NodeId,
) -> String {
    let kind = nodes.kind(id);
    if kind != SyntaxKind::TypeReference {
        return format!("{kind:?}");
    }
    let Some(Node::TypeReferenceNode(reference)) = map.get(id) else {
        return format!("{kind:?}");
    };
    let arguments = if reference.type_arguments.is_empty() { "" } else { " with arguments" };
    let Some(tsr_ast::EntityName::Identifier(name)) = reference.type_name else {
        return "TypeReference qualified name".to_string();
    };
    match name.node_id.and_then(|node| binder.resolve(nodes, node, name.text)) {
        // The name is carried in the reason so the report can histogram it; see
        // `examples/types_shapes.rs`, which splits it back off. It is the direct
        // test of "is this a lib type": `Array` and `Promise` are, `Foo` is not.
        None => format!("TypeReference unresolved{arguments}: {}", name.text),
        Some(symbol) => {
            let flags = binder.symbols().get(symbol).flags;
            format!("TypeReference resolved{arguments}: {flags:?}")
        }
    }
}

/// `ast.IsTypeDeclaration` (`ast/utilities.go:3585`), for the kinds a `.types`
/// baseline can reach. The import forms are omitted deliberately: they depend on
/// `IsTypeOnly` and on the import machinery, and answering them by kind alone
/// would be a guess.
fn is_type_declaration(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::TypeParameter
            | SyntaxKind::ClassDeclaration
            | SyntaxKind::InterfaceDeclaration
            | SyntaxKind::TypeAliasDeclaration
            | SyntaxKind::EnumDeclaration
    )
}

/// Why [`type_at_location`] answered `error`, for a line that it did.
///
/// The gap total alone does not rank work — it says the checker is unfinished,
/// which was known. What ranks work is *where the answer stopped*: an
/// unresolvable name is a lib-file or cross-file problem (`bd tsr-9or.1`), a
/// resolved symbol with no type is `getTypeOfSymbol` (`bd tsr-4sc.7`), and an
/// expression form the worker does not match is that form's own port.
///
/// This re-derives the cause rather than being threaded through the checker, so
/// that [`type_at_location`] stays the single description of what we answer. It
/// is only ever asked about a line that already came back `error`.
#[must_use]
pub fn gap_reason(
    checker: &mut tsr_checker::Checker<'_, '_>,
    binder: &tsr_binder::BindResult<'_>,
    nodes: &NodeTable,
    map: &NodeMap<'_>,
    id: NodeId,
) -> String {
    let error = checker.intrinsics().error;
    let Some(node) = map.get(id) else { return "no node".to_string() };

    // The symbol's flags *and* the kind of its value declaration, because those
    // are the two things upstream's `getTypeOfSymbol` dispatches on: the flags
    // choose the worker and the declaration kind chooses the branch inside it
    // (`checker.go:16509`, `:16578`). Either alone leaves the next step ambiguous.
    let describe = |checker: &mut tsr_checker::Checker<'_, '_>, symbol| {
        let symbols = binder.symbols();
        let flags = symbols.get(symbol).flags;
        let declaration = symbols
            .get(symbol)
            .value_declaration
            .map_or_else(|| "no value declaration".to_string(), |d| format!("{:?}", nodes.kind(d)));
        if checker.get_type_of_symbol(symbol) == error {
            // For a variable-like declaration, which half stopped: upstream takes
            // the annotation when there is one and the initialiser otherwise
            // (`checker.go:16652`), so naming the node that was not understood
            // separates "type nodes are unported" from "expressions are".
            let half = symbols.get(symbol).value_declaration.and_then(|d| map.get(d)).map_or_else(
                String::new,
                |node| match (node.type_id(), node.initializer_id()) {
                    (Some(annotation), _) => format!(
                        " / annotation {}",
                        type_node_reason(binder, nodes, map, annotation)
                    ),
                    (None, Some(initializer)) => {
                        format!(" / initialiser {:?}", nodes.kind(initializer))
                    }
                    (None, None) => " / neither".to_string(),
                },
            );
            format!("symbol has no type: {flags:?} / {declaration}{half}")
        } else {
            "symbol has a type (the line differs for another reason)".to_string()
        }
    };

    if let Some(parent) = nodes.parent(id)
        && nodes.kind(id) == SyntaxKind::Identifier
        && is_type_declaration(nodes.kind(parent))
        && let Some(symbol) = binder.symbol_of(parent)
        && map.get(parent).and_then(|p| p.name_id()) == Some(id)
    {
        let flags = binder.symbols().get(symbol).flags;
        return format!("type declaration name, nothing declared: {flags:?}");
    }

    // Mirrors [`type_at_location`] exactly, **including its fall-through**: the
    // declaration-name branch applies only when the parent actually bound a
    // symbol, and otherwise the node is tried as an expression. An earlier draft
    // of this returned "no symbol bound" as soon as the node was its parent's
    // `name`, and reported 22,768 lines that way — every one of them the `b` of
    // an `a.b`, which `type_at_location` in fact answers through the identifier
    // path. An instrumentation that does not follow the code it explains invents
    // its own findings.
    if let Some(parent) = nodes.parent(id)
        && map.get(parent).and_then(|p| p.name_id()) == Some(id)
        && let Some(symbol) = binder.symbol_of(parent)
    {
        return format!("declaration name, {}", describe(checker, symbol));
    }

    if let Ok(expression) = tsr_ast::Expression::try_from(node) {
        if let tsr_ast::Expression::Identifier(identifier) = expression {
            // A member name — the `b` of `a.b` — reaches here too, because its
            // parent binds no symbol and nothing else claims it. It is called out
            // rather than counted as an ordinary reference, because resolving it
            // as a free name is not merely a gap: a local called `b` in scope
            // would give it that local's type. `bd tsr-tl8`.
            let member = nodes
                .parent(id)
                .filter(|&parent| map.get(parent).and_then(|p| p.name_id()) == Some(id))
                .map(|parent| format!("the name of a {:?}", nodes.kind(parent)));
            let what = member.unwrap_or_else(|| "reference".to_string());
            return match identifier.node_id.and_then(|n| binder.resolve(nodes, n, identifier.text))
            {
                Some(symbol) => format!("{what}, {}", describe(checker, symbol)),
                None => format!("{what}, the name does not resolve"),
            };
        }
        // "answered error", not "form not ported": a ported form propagates its
        // operand's `error` — a parenthesised expression is the visible case —
        // so this names the node that produced the gap and not its cause.
        //
        // A binary expression names its operator too. `checkBinaryExpression` is
        // a dozen unrelated rules behind one node kind — `+` is not `===` is not
        // `&&` — so "39,035 BinaryExpression lines" cannot be ported in an order
        // without this.
        if let tsr_ast::Expression::BinaryExpression(binary) = expression {
            let operator = binary
                .operator_token
                .and_then(|token| token.node_id)
                .map_or_else(|| "?".to_string(), |token| format!("{:?}", nodes.kind(token)));
            return format!("expression answered error: BinaryExpression {operator}");
        }
        return format!("expression answered error: {:?}", nodes.kind(id));
    }
    format!("neither a declaration name nor an expression: {:?}", nodes.kind(id))
}

/// How far our walker agrees with upstream's, **ignoring types entirely**.
///
/// For each position, tests whether upstream's assertion line starts with
/// `{our text} : `. Sound despite the line being unsplittable, because a prefix
/// test needs no split.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WalkerAgreement {
    /// Positions where our expression text matched.
    pub matched: usize,
    /// Assertions upstream wrote.
    pub expected: usize,
    /// Assertions we produced.
    pub produced: usize,
}

/// Compare our assertion *text* against a baseline's, position by position.
#[must_use]
pub fn walker_agreement(expected: &[FileTypes], ours: &[Vec<Assertion>]) -> WalkerAgreement {
    let mut result = WalkerAgreement::default();
    for (index, expected_file) in expected.iter().enumerate() {
        result.expected += expected_file.assertions.len();
        let Some(our_file) = ours.get(index) else { continue };
        result.produced += our_file.len();
        for (position, expected_assertion) in expected_file.assertions.iter().enumerate() {
            let Some(ours) = our_file.get(position) else { continue };
            if expected_assertion.text.starts_with(&format!("{} : ", ours.text)) {
                result.matched += 1;
            }
        }
    }
    result
}

/// Render our assertions in the shape [`crate::types_suite::compare`] takes.
#[must_use]
pub fn to_file_types(name: &str, assertions: &[Assertion]) -> FileTypes {
    FileTypes {
        file: name.to_string(),
        assertions: assertions.iter().map(|a| TypeAssertion { text: a.line() }).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An assertion with the fields the agreement test does not look at filled
    /// in, so those tests keep saying what they are about.
    fn assertion(text: &str, type_string: &str) -> Assertion {
        Assertion {
            text: text.to_string(),
            type_string: type_string.to_string(),
            kind: SyntaxKind::Identifier,
            reason: None,
        }
    }

    /// Assertion texts for a source, with types stubbed out.
    fn texts(source: &str) -> Vec<String> {
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "fixture must parse: {source:?}");
        assertions_for_file(
            &Node::SourceFile(parsed.source_file),
            source,
            &parsed.nodes,
            &parsed.node_map,
            |_| "T".to_string(),
        )
        .into_iter()
        .map(|a| a.text)
        .collect()
    }

    #[test]
    fn assertions_come_out_in_source_order() {
        // Upstream's walk is pre-order with children reversed onto the stack, so
        // they come back out in source order. A walk that forgot the reverse
        // would emit the same *set* and fail every baseline from the first line.
        assert_eq!(texts("const x = 1;"), ["x", "1"]);
        assert_eq!(texts("a.b.c;"), ["a.b.c", "a.b", "a", "b", "c"]);
    }

    #[test]
    fn a_type_annotation_gets_no_assertion() {
        // `IsPartOfTypeNode` — "don't try to get the type of something that's
        // already a type". Without it, `string` would gain a line that upstream
        // never writes, shifting everything after it.
        assert_eq!(texts("const x: string = 1;"), ["x", "1"]);

        // A qualified type name is **asymmetric**, and this is not a guess: the
        // corpus baseline `compiler/aliasBug.types` records
        //
        // ```text
        // var p2: foo.Provide;
        // >p2 : provide.Provide
        // >foo : any
        // ```
        //
        // — the *left* identifier gets a line and the *right* one does not.
        // `IsPartOfTypeNode` drops a qualified name's right-hand side by asking
        // about the enclosing name, which is a type node; the left-hand side is
        // asked about directly and its parent is only a `QualifiedName`, which is
        // not. So `A.B.C` emits `A` and `B` but never `C`. This test originally
        // asserted just `["y"]` and the implementation was right.
        assert_eq!(texts("let y: A.B.C;"), ["y", "A", "B"]);
    }

    #[test]
    fn a_declaration_name_gets_an_assertion_but_a_property_access_name_is_an_expression() {
        // Both reach the walker, by different predicates: `x` is a declaration
        // name, while `b` in `a.b` is an expression. The distinction matters
        // because `PropertyAccessExpression` *has* a `name` field — treating that
        // as a declaration name is the trap `Node::is_declaration_node` exists to
        // avoid.
        assert_eq!(texts("function f(p) {}"), ["f", "p"]);
        assert!(texts("a.b;").contains(&"b".to_string()));

        // **`is_declaration_name` only earns its place on non-identifier names.**
        // `f` and `p` above would be selected anyway by the plain
        // `kind == Identifier` clause, so deleting the predicate changes nothing
        // for them. A string-literal member name is the case that needs it: it is
        // not an identifier, and it is not in an expression context either —
        // `"k"` is the *name* of the property assignment, not its initialiser.
        assert!(texts(r#"const o = { "k": 1 };"#).contains(&r#""k""#.to_string()));
    }

    #[test]
    fn a_name_that_declares_no_value_gets_no_assertion() {
        // Upstream's third drop. An interface names only a type, so its name gets
        // no line; a `const` names a value, so it does. Worth 26 points of walker
        // agreement — 65.08% to 91.05% — because type declarations are dense in
        // the corpus.
        // The interface *name* goes; its *members* stay, because a property
        // signature declares a value. Confirmed against
        // `compiler/sourceMapValidationDestructuringVariableStatementNestedObjectBindingPattern.types`,
        // which records `interface Robot { name: string }` as `>name : string`
        // with no line for `Robot`. Asserted here as `["m"]` — the first draft
        // said `[]` and the implementation was right.
        assert_eq!(texts("interface I { m: string }"), ["m"]);
        assert_eq!(texts("const x = 1;"), ["x", "1"]);
        assert_eq!(texts("function f<T>(p: T) {}"), ["f", "p"], "the type parameter is dropped");
    }

    #[test]
    fn a_heritage_clause_is_a_type_when_it_implements_and_an_expression_when_it_extends() {
        // The two halves of a class header are not symmetric. `extends B`
        // evaluates `B` as a value, so it gets a line; `implements I` names a
        // type, so it does not. And `interface I extends J` is a type as well,
        // because the clause's owner is an interface rather than a class — the
        // `!IsClassLike(parent.Parent)` half of the upstream condition.
        // Counted, not `contains`: `class B {}` emits `B` as its own declaration
        // name, so a containment check passes whether or not the `extends B`
        // reference was kept — which made an earlier version of this test blind
        // to the case it exists for.
        let count = |source: &str, name: &str| {
            texts(source).into_iter().filter(|text| text == name).count()
        };
        assert_eq!(count("class B {} class C extends B {}", "B"), 2, "declaration and base");
        assert_eq!(count("interface I {} class C implements I {}", "I"), 0, "both are types");
        assert_eq!(count("interface J {} interface I extends J {}", "J"), 0, "both are types");
    }

    #[test]
    fn a_namespace_name_appears_only_when_the_namespace_has_a_value_side() {
        // `GetModuleInstanceState`. A namespace holding nothing but types emits
        // no value, so upstream drops its name; one holding a `const` keeps it.
        // Worth 2.2 points of walker agreement and, more tellingly, took excess
        // lines from 813 to 65 — empty and type-only namespaces are common in
        // the corpus.
        assert_eq!(texts("namespace N {}"), Vec::<String>::new());
        assert_eq!(texts("namespace N { interface I {} }"), Vec::<String>::new());
        assert!(texts("namespace M { export const x = 1; }").contains(&"M".to_string()));
        // Nesting takes the highest state of the contents, so an inner value
        // instantiates the outer namespace too.
        let nested = texts("namespace A { export namespace B { export const x = 1; } }");
        assert!(nested.contains(&"A".to_string()) && nested.contains(&"B".to_string()));
        // An ambient module is instantiated without looking inside it, and its
        // name **does** get a line even though it is a string literal — the
        // value-meaning drop applies only to identifiers.
        // `compiler/missingFunctionImplementation2.types` records
        // `declare module "./x" {` as `>"./x" : typeof import("./x")`.
        assert_eq!(texts(r#"declare module "m" {}"#), [r#""m""#]);
    }

    #[test]
    fn an_import_counts_as_a_value_only_when_it_is_re_exported() {
        // The other arm of the worker: a plain import inside a namespace leaves
        // it non-instantiated, an `export import` does not.
        // Only the namespace *name* is at stake here — the import's own name and
        // the entity it references still get lines either way, which is why this
        // asserts on `N` rather than on the whole list.
        assert!(!texts("namespace N { import x = A.B; }").contains(&"N".to_string()));
        assert!(texts("namespace N { export import x = A.B; }").contains(&"N".to_string()));
    }

    #[test]
    fn a_type_aliass_own_name_is_kept_although_it_declares_no_value() {
        // Upstream's exception, and its reason: "for a complex type alias
        // `type T = ...`, showing T : T isn't very helpful" — but the name is
        // kept because the type "may evaluate to some interesting type". Without
        // the exception the alias name would be dropped with every other
        // type-only name.
        assert_eq!(texts("type T = string;"), ["T"]);
    }

    #[test]
    fn source_text_skips_leading_trivia_and_strips_line_breaks() {
        // Upstream's `sourceText` runs from the *post-trivia* position to the
        // node's end, and `iterateBaseline` then removes line delimiters.
        assert_eq!(source_text("  /* c */ x", tsr_core::Span::new(0, 11)), "x");
        assert_eq!(source_text("// c\nx", tsr_core::Span::new(0, 6)), "x");
        assert_eq!(source_text("a +\nb", tsr_core::Span::new(0, 5)), "a +b");
    }

    #[test]
    fn walker_agreement_ignores_the_type_half() {
        // The whole point: this must be blind to types, so a wrong checker
        // cannot be mistaken for a wrong walker.
        let expected = vec![FileTypes {
            file: "a.ts".into(),
            assertions: vec![
                TypeAssertion { text: "x : string".into() },
                TypeAssertion { text: "1 : 1".into() },
            ],
        }];
        let ours = vec![vec![assertion("x", "totally wrong"), assertion("1", "also wrong")]];
        let agreement = walker_agreement(&expected, &ours);
        assert_eq!(agreement.matched, 2, "both texts match though both types are wrong");
        assert_eq!(agreement.expected, 2);
    }

    #[test]
    fn walker_agreement_is_not_fooled_by_a_prefix_of_a_longer_expression() {
        // The prefix test compares against `{text} : `, not `{text}`. Without the
        // separator, our `a` would "match" upstream's `a.b : X`, and a walker
        // that truncated every expression would read 100%.
        let expected = vec![FileTypes {
            file: "a.ts".into(),
            assertions: vec![TypeAssertion { text: "a.b : X".into() }],
        }];
        let ours = vec![vec![assertion("a", "X")]];
        assert_eq!(walker_agreement(&expected, &ours).matched, 0);
    }
}
