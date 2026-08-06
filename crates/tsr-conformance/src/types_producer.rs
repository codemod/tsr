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

use std::sync::OnceLock;

use tsr_ast::{Node, NodeId, NodeMap, NodeTable, SyntaxKind, Tree, predicates, push_children};
use tsr_binder::SymbolFlags;

use crate::types_baseline::{FileTypes, TypeAssertion};

/// The current directory every case is compiled in.
///
/// The same one [`crate::binder_suite`] uses, so a unit named `a.ts` and a unit
/// named `/a.ts` land on the same path in both suites.
const CURRENT_DIRECTORY: &str = "/";

/// Where the bundled `lib.*.d.ts` are mounted on the case's file system.
///
/// A directory no case can name: upstream's real path is machine-dependent and
/// none of the baselines mention one, so any *observable* difference between
/// this and a real path would be a bug on its own.
const LIB_DIRECTORY: &str = "/.ts-lib";

/// The bundled lib files, read from the submodule once per process.
///
/// Once, because the alternative is 12,444 reads of 3.9 MB. Their *parse* is
/// still per case — see `docs/architecture/program.md`, "What a program of libs
/// costs", where that was measured at 15–17 ms and judged worth paying rather
/// than blocking on `bd tsr-6av`.
///
/// Empty when the submodule is absent, which degrades a case to a program with
/// no libs — the same program this suite built before the rewire, and not an
/// error.
fn bundled_libs() -> &'static [(String, String)] {
    static LIBS: OnceLock<Vec<(String, String)>> = OnceLock::new();
    LIBS.get_or_init(|| {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .map(|root| root.join("vendor/typescript-go/internal/bundled/libs"));
        let Some(dir) = dir else { return Vec::new() };
        let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
        let mut libs = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("lib.") || !name.ends_with(".d.ts") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(entry.path()) {
                libs.push((format!("{LIB_DIRECTORY}/{name}"), text));
            }
        }
        // Sorted, because `read_dir` yields in filesystem order and the binder
        // takes **first-in-wins** when two files declare the same global
        // (`binder.rs:522`). `interface Array<T>` is declared in 8 of these
        // files and `String` in 11, so *which* declaration survives — and
        // therefore which members a lib type has — depended on directory order.
        // A conformance number that varies with the filesystem is not a
        // measurement, and two machines could disagree about the same commit.
        //
        // Declaration merging (`bd tsr-9or.1`) makes the choice moot for merged
        // symbols, but the sort is what makes the number reproducible *now*,
        // and it keeps the pre- and post-merge measurements comparable.
        libs.sort_by(|(a, _), (b, _)| a.cmp(b));
        libs
    })
    .as_slice()
}

/// A case's units and the bundled libs, as a file system.
struct CaseHost {
    fs: tsr_vfs::InMemoryFileSystem,
}

impl tsr_module::types::ResolutionHost for CaseHost {
    fn fs(&self) -> &dyn tsr_vfs::FileSystem {
        &self.fs
    }

    fn current_directory(&self) -> &str {
        CURRENT_DIRECTORY
    }
}

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
/// The type a node has, as upstream's `GetTypeAtLocation` would answer it,
/// **rendered**.
///
/// The whole body is [`type_id_at_location`]; this is `render` applied to its
/// answer. The two were one function until `examples/subtypes.rs` needed the
/// `TypeId` rather than the string, and the split is mechanical: every exit of
/// the original was already `render(checker, id, <a TypeId>)`, so the rendering
/// step lifts out without touching a single decision. That is why this is a
/// refactor and not a change — there is no arm where the two could now
/// disagree.
pub fn type_at_location(
    checker: &mut tsr_checker::Checker<'_, '_>,
    binder: &tsr_binder::BindResult<'_>,
    nodes: &NodeTable,
    map: &NodeMap<'_>,
    id: NodeId,
) -> String {
    let computed = type_id_at_location(checker, binder, nodes, map, id);
    render(checker, id, computed)
}

/// The type a node has, as upstream's `GetTypeAtLocation` would answer it, as a
/// [`tsr_checker::TypeId`] rather than a string.
///
/// This is the whole of [`type_at_location`]'s body; that function is this plus
/// `render`. Split out for `examples/subtypes.rs`, which has to inspect a
/// union's *constituents* and cannot do that through a rendered string.
pub fn type_id_at_location(
    checker: &mut tsr_checker::Checker<'_, '_>,
    binder: &tsr_binder::BindResult<'_>,
    nodes: &NodeTable,
    map: &NodeMap<'_>,
    id: NodeId,
) -> tsr_checker::TypeId {
    let error = checker.intrinsics().error;
    let Some(node) = map.get(id) else { return error };

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
        return declared;
    }

    // `IsRightSideOfPropertyAccess` (`ast/utilities.go:3604`). The `b` of `a.b`
    // is typed as the *property*, which upstream reaches by having
    // `checkPropertyAccessExpression` record the resolved symbol on the name
    // node; here the access is checked and its answer used, which is the same
    // answer by a shorter route.
    //
    // Until this existed the name fell through to the identifier path and was
    // resolved as a **free name in the enclosing scope** — `bd tsr-tl8`, and the
    // one place this port could answer wrongly where a gap belonged.
    if let Some(parent) = nodes.parent(id)
        && nodes.kind(parent) == SyntaxKind::PropertyAccessExpression
        && map.get(parent).and_then(|p| p.name_id()) == Some(id)
        && let Some(Node::PropertyAccessExpression(access)) = map.get(parent)
    {
        let computed = checker.check_property_access_expression(access);
        return computed;
    }

    // A declaration name resolves through its parent's symbol.
    if let Some(parent) = nodes.parent(id)
        && map.get(parent).and_then(|p| p.name_id()) == Some(id)
        && let Some(symbol) = binder.symbol_of(parent)
    {
        let computed = checker.get_type_of_symbol(symbol);
        return computed;
    }

    // **A base class expression prints the base's instance type, not `typeof`**,
    // and this is a property of the *baseline writer* rather than of the checker.
    //
    // `class B extends A` puts `A` in an expression position, so
    // `IsInExpressionContext` is true and `getTypeOfNode` answers `typeof A` —
    // which is what upstream's checker answers too, correctly. Upstream
    // compensates in the writer, at `type_symbol_baseline.go:371`, and labels it
    // a workaround in those words:
    //
    // ```go
    // // Workaround to ensure we output 'C' instead of 'typeof C' for base class expressions
    // if ast.IsExpressionWithTypeArgumentsInClassExtendsClause(node.Parent) {
    //     t = fileChecker.GetTypeAtLocation(node.Parent)
    // }
    // if t == nil || checker.IsTypeAny(t) { t = fileChecker.GetTypeAtLocation(node) }
    // ```
    //
    // **`getTypeOfNode`'s own heritage branch (`checker.go:31959`) does not fix
    // this and porting it would be dead code.** `IsExpressionNode` of an
    // `ExpressionWithTypeArguments` is `!IsHeritageClause(node.Parent)`, so the
    // walker never selects the `EWTA` itself and that branch is unreachable from
    // a baseline — it exists for the language service. This function fuses
    // upstream's walker and `getTypeOfNode`, so the compensation belongs *here*,
    // keyed on the identifier's parent.
    //
    // `extends` only: `TryGetClassExtendingExpressionWithTypeArguments`
    // (`ast/utilities.go:1430`) rejects the `implements` case, and the two halves
    // of a class header are already asymmetric in the walker for the same reason.
    //
    // Worth 1,086 corpus lines, found by bucketing wrong answers by the parent
    // kind of the node that produced them.
    if let Some(parent) = nodes.parent(id)
        && nodes.kind(parent) == SyntaxKind::ExpressionWithTypeArguments
        && let Some(clause) = nodes.parent(parent)
        && let Some(Node::HeritageClause(heritage)) = map.get(clause)
        && heritage.token.kind == SyntaxKind::ExtendsKeyword
        && matches!(
            nodes.parent(clause).map(|owner| nodes.kind(owner)),
            Some(SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression)
        )
        && nodes.kind(id) == SyntaxKind::Identifier
        && let Some(Node::Identifier(name)) = map.get(id)
        && let Some(symbol) =
            binder.resolve_name(nodes, map, id, name.text, tsr_binder::SymbolFlags::TYPE)
    {
        let declared = checker.get_declared_type_of_symbol(symbol);
        // Upstream's `t == nil || IsTypeAny(t)` fallback: when the base's
        // declared type is not available, the expression's own answer is used
        // rather than a gap being invented here.
        if declared != error {
            return declared;
        }
    }

    // **The left of a qualified name in type position prints `any`.**
    //
    // `A` in `function test(a: A.Outer)` records `>A : any` upstream, and the
    // route it takes there is worth stating because the answer looks like a
    // claim and is not. `getTypeOfNode` falls **all the way through** for that
    // node — the *left* of a qualified name is not itself part of a type node,
    // is not an expression node, and is no declaration form — so upstream
    // returns `errorType`. The writer then renders it, and because `errorType`
    // carries `TypeFlagsAny` the node builder prints `any`.
    //
    // Ours reached the expression fall-through below and answered `typeof A`,
    // which is a **wrong answer where upstream has a computed one**. That is the
    // second-largest parent kind among wrong identifier lines: 4,455 of them,
    // 97.3% of the `QualifiedName` population, measured independently by two
    // agents before either knew the other was looking.
    //
    // This is the same shape as the `extends`-clause compensation above — a
    // rendering rule that lives in upstream's **baseline writer**, not in its
    // checker — which is why it belongs here rather than in `tsr-checker`. The
    // checker still has no answer for this node and should not pretend to; the
    // producer knows what upstream's writer prints for it.
    // **Except inside a `typeof`, where the left keeps its value type.** Added
    // after the first version of this rule cost 120 lines in the `typeof`
    // bucket: `typeof M.C` records `>M : typeof M`, not `>M : any`.
    //
    // Upstream draws the line in two places and both say the same thing.
    // `isPartOfTypeNodeInParent` opens with `if parent.Kind == KindTypeQuery {
    // return false }`, and `IsExpressionNode`'s `QualifiedName` arm walks up
    // through nested qualified names and then asks `IsTypeQueryNode`. For
    // `typeof M.C` that is **true**, so `M` is an expression node and
    // `getTypeOfNode` answers `getRegularTypeOfExpression` — `typeof M`. For
    // `A.Outer` it is false, everything falls through to `errorType`, and the
    // writer prints `any`.
    //
    // `conformance/recursiveTypesWithTypeof.types` pins it: `var g: typeof g.x`
    // records `>g : { x: typeof g; }` for the *left* of the qualified name —
    // typed as an expression, not as `any`.
    //
    // The walk is up through **nested** qualified names, because `typeof A.B.C`
    // nests them and only the outermost parent is the `TypeQuery`.
    if let Some(parent) = nodes.parent(id)
        && nodes.kind(parent) == SyntaxKind::QualifiedName
        && let Some(Node::QualifiedName(qualified)) = map.get(parent)
        && qualified.right.and_then(|right| right.node_id) != Some(id)
    {
        let mut outermost = parent;
        while let Some(above) = nodes.parent(outermost) {
            if nodes.kind(above) != SyntaxKind::QualifiedName {
                break;
            }
            outermost = above;
        }
        let enclosing = nodes.parent(outermost).map(|above| nodes.kind(above));

        // **A third exemption: the entity name of an `import x = M.a`.**
        //
        // Upstream does *not* reach this through `IsExpressionNode` — that
        // function's `KindQualifiedName` arm walks to the outermost qualified
        // name and asks `IsTypeQueryNode || IsJSDocLinkLike ||
        // IsJSDocNameReference || IsJsxTagName`, all false for an import-equals.
        // It falls through `getTypeOfNode` past the type-node, expression,
        // class, type-declaration, binding and declaration branches to
        // `isInRightSideOfImportOrExportAssignment`, which takes
        // `getDeclaredTypeOfSymbol` and **falls back to `getTypeOfSymbol` when
        // that is the error type**. For a namespace the declared type *is* the
        // error type, so the answer is the value type: `typeof a`.
        //
        // The fallback order is the whole rule and must not be collapsed to
        // "answer the value type". Measured at `f99072c` by
        // `examples/qualified_name_left.rs`, this enclosing kind carries 133
        // wrong lines *and 34 right ones* — the 34 being where upstream also
        // says `any`. A blanket value-type rule would fix 133 and break 34, a
        // net of +99 presented as +133.
        if enclosing == Some(SyntaxKind::ImportEqualsDeclaration)
            && let Some(Node::Identifier(name)) = map.get(id)
            && let Some(symbol) =
                binder.resolve_name(nodes, map, id, name.text, SymbolFlags::NAMESPACE)
        {
            let declared = checker.get_declared_type_of_symbol(symbol);
            if declared != checker.intrinsics().error {
                return declared;
            }
            let value = checker.get_type_of_symbol(symbol);
            return value;
        }

        if enclosing != Some(SyntaxKind::TypeQuery) {
            return checker.intrinsics().any;
        }
    }

    // **The property name of a binding element prints `any`** — the `a` of
    // `const { a: b } = x`. This is a *strict subset* of the second guard at
    // `type_symbol_baseline.go:380`, `!ast.IsBindingElement(node.Parent)`, and
    // the subsetting is the whole decision.
    //
    // Upstream's guard covers **every** child of a binding element: the bound
    // name `b`, the property name `a`, the initialiser, the dots. Measured, the
    // arm's positions do not behave alike at all — `examples/writer_guards.rs`,
    // `WRITER_GUARDS_FLATTEN=1`, corpus at `0e8e902`:
    //
    // ```text
    // position                              claims   -> any  -> error  -> other
    // binding element: the bound name         2640      663        0      1977
    // binding element: the property name       428      428        0         0
    // binding element: elsewhere                76       16        0        60
    // ```
    //
    // The bound name is 25% conversion with a 1,977-line residue: upstream holds
    // a *real* type there most of the time and our `error` is our own gap, so
    // converting it would print `any` where upstream printed `string` — the false
    // credit ADR-0038 and ADR-0039 both refuse. The property name is 428 of 428
    // with an **empty** residue, the same signature the label arm had.
    //
    // And here the signature is not merely statistical. Trace `getTypeOfNode`
    // (`checker.go:31927`) for the `a` of `const { a: b } = x`:
    //
    // - `IsPartOfTypeNode` — no.
    // - `IsExpressionNode` — the `KindIdentifier` arm falls through to
    //   `IsInExpressionContext` (`ast/utilities.go:1983`), whose `KindBindingElement`
    //   case is `parent.Initializer() == node`. The property name is not the
    //   initialiser, so **false**.
    // - `IsTypeDeclaration` / `IsTypeDeclarationName` / `IsBindingElement(node)` —
    //   no; the node is the identifier, not the element.
    // - `IsDeclaration` — an identifier is not one.
    // - `IsDeclarationNameOrImportPropertyName` (`ast/utilities.go:1311`) — the
    //   `ImportSpecifier`/`ExportSpecifier` special case does not apply, so this
    //   is `IsDeclarationName`, which is `parent.Name() == node`. A binding
    //   element's `Name()` is the **bound** name `b`, not `a`. **False.**
    // - `IsBindingPattern`, the import/export assignment branch, `IsMetaProperty`,
    //   `IsImportAttributes` — no.
    //
    // It falls off the end: `return c.errorType`. Upstream holds `errorType` at
    // this position **by construction**, for every program, and the guard is what
    // renders it `any`. That is what makes converting it a port of upstream's
    // writer rather than a string match on a path that differs.
    //
    // Distributed rather than concentrated: 428 lines over 92 cases, top ten
    // 40.9%, the largest single case 29 lines.
    //
    // The `IsTypeAny` precondition is carried across for the same port-specific
    // reason as the label arm below: given `const a = 1; const { a: b } = x;`,
    // *this* producer reaches the expression fall-through and answers `1`, which
    // upstream never does. Those lines were not in the 428 and converting them
    // would ship an unmeasured change under a measured one.
    if let Some(parent) = nodes.parent(id)
        && let Some(Node::BindingElement(element)) = map.get(parent)
        && element.property_name.and_then(|name| name.node_id()) == Some(id)
    {
        let name = tsr_ast::Expression::try_from(node)
            .map_or(error, |expression| checker.check_expression(expression));
        if name == error {
            return checker.intrinsics().any;
        }
    }

    // **A label name prints `any`, and the writer is what decides it** — the
    // third of the eight guards at `type_symbol_baseline.go:380`, and the second
    // ported after `isIntrinsicJsxTag`.
    //
    // A label is not a value: it lives in its own namespace, `resolveName` never
    // sees it, and upstream's `checkIdentifier` answers **`errorType`** — the
    // same answer this port already gives. The `any` is the writer falling
    // through to the node builder because `!ast.IsLabelName(node)` failed.
    //
    // This arm was measured before it was ported, and it is the *only* one of the
    // seven unported guards whose measurement recommends porting it: **209 corpus
    // lines, 209 of which upstream answers `any`, with an empty residue.** A 100%
    // conversion rate and a zero `-> other` column is the signature of a position
    // where upstream *always* holds `errorType`, which is what makes converting
    // it a faithful port rather than a string match on a different path. The
    // dominant guard, `hadErrorBaseline`, has the opposite shape — 39,412
    // conversions of which ~93% are lines where upstream computed a genuine
    // `anyType` this port cannot — and is deliberately **not** ported. See
    // docs/architecture/checker-notes-guard.md for both measurements.
    //
    // The `IsTypeAny` precondition is upstream's and is load-bearing here for a
    // port-specific reason, not a theoretical one: given `const outer = 1;`,
    // *this* checker resolves the label `outer` to the variable and answers `1`.
    // Upstream does not share that namespace, so the line is ours to be wrong
    // about either way — but converting it would be an unmeasured change riding
    // along with a measured one, and the 209 lines never included it.
    if is_label_name(id, nodes, map) {
        let label = tsr_ast::Expression::try_from(node)
            .map_or(error, |expression| checker.check_expression(expression));
        if label == error {
            return checker.intrinsics().any;
        }
    }

    // **An intrinsic JSX tag name prints `any`, and the checker is not what
    // decides that** — the writer is, exactly as for the two compensations
    // above.
    //
    // `>div : any` appears 1,025 times in the corpus and never once as anything
    // else, *including* in cases carrying a complete `JSX` namespace where the
    // paired `.symbols` line resolves the same node to
    // `Symbol(JSX.IntrinsicElements.div, Decl(react.d.ts, ...))`. It is tempting
    // to read that as upstream computing `anyType`. It does not. Upstream's
    // checker answers **`errorType`** here: `getIntrinsicTagSymbol`
    // (`jsx.go:1216`) caches its resolved symbol on the *opening element's*
    // links rather than the tag name's, and
    // `checkJsxOpeningLikeElementOrOpeningFragment` (`jsx.go:130`) takes
    // `getStringLiteralType(tagName.Text())` for an intrinsic and deliberately
    // never calls `checkExpression(tagName)`. So the tag name reaches
    // `getTypeOfNode` unvisited, `resolveName("div", Value)` misses, and
    // `checkIdentifier` returns the error type.
    //
    // The writer then renders that error type **twice over, two different
    // ways**. `type_symbol_baseline.go:378` prints `t.AsIntrinsicType()
    // .IntrinsicName()` — the literal string `"error"` — unless one of eight
    // guards excludes the node, in which case it falls through to the node
    // builder, which renders any `TypeFlagsAny` type as the `any` keyword. One
    // of those eight is `isIntrinsicJsxTag`, ported below.
    //
    // `conformance/inlineJsxFactoryOverridesCompilerOption.types` shows both
    // spellings of one type on adjacent lines, which is the whole proof:
    //
    // ```text
    // ><h></h> : error      <- errorType, fast path, prints the intrinsic name
    // >h : any              <- the SAME errorType, guard fires, node builder
    // ```
    //
    // This corrects ADR-0038, which read the `any` spelling as evidence that
    // upstream renders `errorType` as `any` everywhere and concluded the
    // `error`-printing population was unreachable. It is reachable; it is
    // unported. See ADR-0039.
    //
    // **Only this one guard is ported.** The other seven — `hadErrorBaseline`,
    // binding element, label name, global scope augmentation, meta property, and
    // the import/export statement names — move populations nobody has measured,
    // and the property-access/qualified-name one is already covered above by a
    // rule reached along a different route. Porting them blind would present an
    // unmeasured net as a gain.
    //
    // The `IsTypeAny` precondition is upstream's and is load-bearing rather than
    // defensive: a **lowercase tag name that does resolve** — `const foo = () =>
    // {}` used as `<foo/>` — has a real type, never reaches the fast path at
    // all, and must keep printing it. The corpus discriminates this at 32 lines
    // printing `() => any` and 24 printing `typeof foo`.
    if nodes.kind(id) == SyntaxKind::Identifier
        && let Some(parent) = nodes.parent(id)
        && jsx_tag_name_of(parent, map) == Some(id)
        && let Some(Node::Identifier(name)) = map.get(id)
        && is_intrinsic_jsx_name(name.text)
    {
        let tag = tsr_ast::Expression::try_from(node)
            .map_or(error, |expression| checker.check_expression(expression));
        if tag == error || tag == checker.intrinsics().any {
            return checker.intrinsics().any;
        }
    }

    if let Ok(expression) = tsr_ast::Expression::try_from(node) {
        let computed = checker.check_expression(expression);
        return computed;
    }
    error
}

/// Render a type as the answer for the line at `reference`.
///
/// The whole of this file's part in `bd tsr-6j2`. `Checker::type_to_string_at`
/// is a **second** entry point beside `type_to_string`, which is untouched and
/// keeps its 110 call sites; only this rendering path passes a node, and only
/// this path can therefore get a context-sensitive name. `None` means the
/// checker cannot name the type at this position — a module object with no
/// unambiguous alias in scope — and that is rendered as a gap, exactly as an
/// uncomputed type is, rather than as the module symbol's file path.
fn render(
    checker: &mut tsr_checker::Checker<'_, '_>,
    reference: NodeId,
    id: tsr_checker::types::TypeId,
) -> String {
    let error = checker.intrinsics().error;
    checker.type_to_string_at(id, reference).unwrap_or_else(|| checker.type_to_string(error))
}

/// Whether this identifier is a label name.
///
/// Ported from `ast.IsLabelName` (`internal/ast/utilities.go:2263`), which is the
/// disjunction of two predicates:
///
/// ```go
/// return IsLabelOfLabeledStatement(node) || IsJumpStatementTarget(node)
/// ```
///
/// Both halves matter. `outer: while (true) { break outer; }` produces **two**
/// assertion lines for `outer`, and a guard keyed only on the declaration would
/// convert one of them.
fn is_label_name(id: NodeId, nodes: &NodeTable, map: &NodeMap<'_>) -> bool {
    if nodes.kind(id) != SyntaxKind::Identifier {
        return false;
    }
    let Some(parent) = nodes.parent(id) else { return false };
    let label = match map.get(parent) {
        Some(Node::LabeledStatement(statement)) => statement.label,
        Some(Node::BreakStatement(statement)) => statement.label,
        Some(Node::ContinueStatement(statement)) => statement.label,
        _ => return false,
    };
    label.and_then(|label| label.node_id) == Some(id)
}

/// The tag name of a JSX opening, closing or self-closing element.
///
/// The element kinds are upstream's, from the first clause of `isIntrinsicJsxTag`
/// (`internal/testutil/tsbaseline/type_symbol_baseline.go:481`). A JSX fragment
/// has no tag name and a `JsxNamespacedName` tag is not an identifier, so both
/// answer `None` and fall through to the ordinary expression path.
fn jsx_tag_name_of(element: NodeId, map: &NodeMap<'_>) -> Option<NodeId> {
    match map.get(element)? {
        Node::JsxOpeningElement(n) => n.tag_name.and_then(|tag| tag.node_id()),
        Node::JsxClosingElement(n) => n.tag_name.and_then(|tag| tag.node_id()),
        Node::JsxSelfClosingElement(n) => n.tag_name.and_then(|tag| tag.node_id()),
        _ => None,
    }
}

/// Whether a JSX tag name is an *intrinsic* element rather than a value reference.
///
/// Ported from `IsIntrinsicJsxName` (`internal/scanner/utilities.go:98`):
///
/// ```go
/// return len(name) != 0 && (name[0] >= 'a' && name[0] <= 'z' || strings.ContainsRune(name, '-'))
/// ```
///
/// The hyphen clause is not decoration — it is what makes `<public-foo>` a custom
/// element rather than a name lookup, and `conformance/jsxParsingError4` carries
/// exactly that shape.
fn is_intrinsic_jsx_name(name: &str) -> bool {
    name.starts_with(|first: char| first.is_ascii_lowercase()) || name.contains('-')
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
    // **One program per case, one checker over it.** Until 2026-08-05 this loop
    // gave each unit its own arena, bound it alone, and gave it its own
    // `Checker`, so a case's units could not see each other and no lib file was
    // in the picture at all. That was not a shortcut: with per-file identity a
    // symbol from another file could not be handed to the checker without
    // reading the wrong file's declarations. ADR-0034 widened the identity
    // space; this is the commit that lets the measurement see it, and it is
    // deliberately the *only* thing in its commit so its delta is the
    // widening's effect and nothing else.
    let arena = tsr_core::Arena::new();
    let program = program_for_case(&arena, case);
    render_case(&program, case, expected, explain, None)
}

/// [`assertions_for_case`], in a caller-owned program, with the node behind
/// each line.
///
/// A probe that classifies lines by *where the node sits* needs the `NodeId` and
/// the program's tables, which the plain entry point cannot hand back because it
/// owns the arena the program borrows. So the caller owns the arena and gets the
/// program back beside the lines; `program.nodes()` and `program.node_map()` are
/// then the tables the ids index.
///
/// This exists so a probe's denominator is the gradient's **by construction**.
/// `examples/writer_guards.rs` re-implemented the per-unit, lib-less shape
/// instead of calling in here, and every number it produced was measured against
/// a corpus with no `lib.*.d.ts` in it while the gradient it was compared to had
/// them all. `examples/overload_funnel.rs` had the identical defect and was
/// rewired in `731b1ee`; see `docs/architecture/checker-notes-guard.md`.
#[must_use]
pub fn assertions_for_case_with_ids<'a>(
    arena: &'a tsr_core::Arena,
    case: &crate::TestCase,
    expected: &[FileTypes],
) -> (tsr_compiler::Program<'a>, Vec<Vec<Assertion>>, Vec<Vec<NodeId>>) {
    let program = program_for_case(arena, case);
    let mut ids = Vec::new();
    let rendered = render_case(&program, case, expected, false, Some(&mut ids));
    debug_assert_eq!(ids.len(), rendered.len(), "one id section per rendered section");
    (program, rendered, ids)
}

/// The body both entry points share, so they cannot drift apart.
fn render_case(
    program: &tsr_compiler::Program<'_>,
    case: &crate::TestCase,
    expected: &[FileTypes],
    explain: bool,
    mut ids: Option<&mut Vec<Vec<NodeId>>>,
) -> Vec<Vec<Assertion>> {
    let nodes = program.nodes();
    let node_map = program.node_map();
    let bound = program.binder();
    // One checker for the whole program, not one per unit — which is upstream's
    // shape (`Program` has one `Checker`) and also means a lib type resolved for
    // the first unit is memoised for the rest.
    //
    // **With the program as its module host**, which is the line that makes the
    // cross-file seam visible to the gradient at all
    // ([ADR-0041](../../../docs/adr/0041-the-checker-asks-its-program-for-a-module.md)).
    // Everything else in that workstream — the loader's resolution cache, the
    // `ModuleHost` impl, the `Checker` field — is dead until a call site
    // supplies a host, and this is the only call site the `checker_types`
    // gradient runs through.
    //
    // The two other `Checker::new` sites in this file (the unit-level probes
    // further down) are deliberately left host-less: they parse one file and
    // have no program, and they are the control that a call site without a host
    // is unchanged.
    let mut checker = tsr_checker::Checker::with_module_host(bound, nodes, node_map, Some(program));

    let mut ours = Vec::new();
    for expected_file in expected {
        // A section with no unit, a JSON unit, or a unit the loader did not put
        // in the program — an unsupported extension, or a name it could not read
        // — renders empty, exactly as a unit with no expected section does.
        // Absent rather than wrong.
        let file = case
            .files
            .iter()
            .find(|u| crate::binder_suite::same_unit(&u.name, &expected_file.file))
            .filter(|u| {
                tsr_parser::ScriptKind::from_file_name(&u.name) != tsr_parser::ScriptKind::Json
            })
            .and_then(|u| program.source_file(&u.name));
        let Some(file) = file else {
            ours.push(Vec::new());
            if let Some(ids) = ids.as_deref_mut() {
                ids.push(Vec::new());
            }
            continue;
        };
        // The program's copy of the text, not the case's: they are equal, and
        // asking the file is what keeps them equal if the loader ever stops
        // handing the host's bytes through unchanged.
        let text = file.text();
        let mut gaps = Vec::new();
        let mut visited = Vec::new();
        let mut rendered = assertions_for_file(
            &Node::SourceFile(file.source_file()),
            text,
            nodes,
            node_map,
            |id| {
                visited.push(id);
                let answer = type_at_location(&mut checker, bound, nodes, node_map, id);
                if explain && answer == "error" {
                    gaps.push(id);
                }
                answer
            },
        );
        debug_assert_eq!(visited.len(), rendered.len(), "one recorded id per rendered line");
        if let Some(ids) = ids.as_deref_mut() {
            ids.push(visited);
        }
        if explain {
            let mut gaps = gaps.into_iter();
            for assertion in &mut rendered {
                if assertion.type_string == "error" {
                    let id = gaps.next().expect("one recorded gap per `error` line");
                    assertion.reason = Some(gap_reason(&mut checker, bound, nodes, node_map, id));
                }
            }
        }
        ours.push(rendered);
    }
    ours
}

/// The case's units and the bundled libs, as one program.
///
/// Every unit is a **root file**, rather than only the ones nothing imports:
/// this suite renders whatever section the baseline has, and a unit dropped for
/// being unreachable would render empty and be counted as a miss. Upstream's
/// `programFileNames` makes the same choice for a case with no tsconfig.
fn program_for_case<'a>(
    arena: &'a tsr_core::Arena,
    case: &crate::TestCase,
) -> tsr_compiler::Program<'a> {
    let mut files: Vec<(String, String)> = bundled_libs().to_vec();
    let mut roots = Vec::new();
    for unit in &case.files {
        let name = tsr_path::get_normalized_absolute_path(&unit.name, CURRENT_DIRECTORY);
        files.push((name.clone(), unit.content.clone()));
        roots.push(name);
    }

    let options = crate::trace_case::apply_test_directives(
        tsr_core::CompilerOptions::default(),
        case,
        CURRENT_DIRECTORY,
    );
    let host = CaseHost { fs: tsr_vfs::InMemoryFileSystem::new(files, [], true) };
    tsr_compiler::Program::from_root_files(
        arena,
        &host,
        tsr_compiler::LoadOptions {
            compiler_options: options,
            root_file_names: roots,
            default_library_path: LIB_DIRECTORY.to_string(),
        },
    )
}

/// Why a property access answered `errorType`: the receiver, or the property.
///
/// The distinction decides what to build next and nothing else can supply it —
/// "23,376 lines on a `PropertyAccessExpression`" is one number for two entirely
/// different pieces of work.
fn access_reason(
    checker: &mut tsr_checker::Checker<'_, '_>,
    access: &tsr_ast::PropertyAccessExpression<'_>,
    nodes: &NodeTable,
) -> String {
    let error = checker.intrinsics().error;
    let Some(receiver) = access.expression else { return "no receiver".to_string() };
    let receiver_type = checker.check_expression(receiver);
    if receiver_type == error {
        let kind = receiver
            .node_id()
            .map_or_else(|| "?".to_string(), |id| format!("{:?}", nodes.kind(id)));
        return format!("the receiver is a gap: {kind}");
    }
    let Some(tsr_ast::MemberName::Identifier(name)) = access.name else {
        return "the name is not an identifier".to_string();
    };
    if checker.get_property_of_type(receiver_type, name.text).is_some() {
        return "the property has no type".to_string();
    }
    format!("the receiver has no such property: {}", checker.type_to_string(receiver_type))
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
    // Two composite kinds recurse into what they are made of, because the bare
    // kind name cannot be ranked. `annotation ArrayType` was 2,020 lines that
    // could have been either "the global `Array` did not resolve" or "the
    // element type gapped" — two different pieces of work, one label. It turned
    // out to be the former (the globals are never merged across files;
    // `crates/tsr-compiler/src/lib.rs:24`), but the instrument could not say so
    // and a teammate had to read the checker to find out.
    //
    // A type literal recurses to its *first gapping member* for a sharper
    // reason: `get_type_from_type_literal` (`declared.rs:154`) returns `error`
    // for the whole literal if any one member gaps, so the interesting fact is
    // which member, not that it was a literal.
    if kind == SyntaxKind::ArrayType {
        if let Some(Node::ArrayTypeNode(array)) = map.get(id)
            && let Some(element) = array.element_type
            && let Some(element_id) = element.node_id()
        {
            return format!("ArrayType of {}", type_node_reason(binder, nodes, map, element_id));
        }
        return format!("{kind:?}");
    }
    if kind == SyntaxKind::TypeLiteral {
        if let Some(Node::TypeLiteralNode(literal)) = map.get(id) {
            let member = literal
                .members
                .iter()
                .filter_map(tsr_ast::TypeElement::node_id)
                .map(|member| type_node_reason(binder, nodes, map, member))
                .next();
            if let Some(member) = member {
                return format!("TypeLiteral of {member}");
            }
        }
        return format!("{kind:?}");
    }
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
    // `SymbolFlags::TYPE`, because this mirrors `declared.rs`'s
    // `get_type_from_type_reference`, which passes exactly that
    // (upstream's `SymbolFlagsType` at a type reference). Passing the old
    // unfiltered meaning here would make the instrument answer a question the
    // checker no longer asks — the drift that produced the 22,768-line phantom
    // finding, and the reason this function mirrors the code it explains line
    // for line rather than approximating it.
    match name
        .node_id
        .and_then(|node| binder.resolve_name(nodes, map, node, name.text, SymbolFlags::TYPE))
    {
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
        // The flags alone were actively misleading for a type alias: the label
        // read "nothing declared", but the arm *does* exist
        // (`get_declared_type_of_type_alias`, `declared.rs:562`) and something
        // *is* declared — what gapped is the alias's right-hand side. 1,134
        // lines sat unrankable under a label that pointed at the wrong half of
        // the declaration. Report the RHS instead, when there is one.
        let alias = map
            .get(parent)
            .and_then(|node| match node {
                Node::TypeAliasDeclaration(alias) => alias.r#type,
                _ => None,
            })
            .and_then(|rhs| rhs.node_id())
            .map(|rhs| type_node_reason(binder, nodes, map, rhs));
        return match alias {
            Some(alias) => format!("type declaration name, the alias RHS gaps: {alias}"),
            None => format!("type declaration name, nothing declared: {flags:?}"),
        };
    }

    // The `b` of `a.b` is typed as the access itself, so it is explained as one.
    // Keeping this in step with [`type_at_location`] is not optional: an earlier
    // version of this function reported 22,768 lines under a cause it had
    // invented by not following the code it explains.
    if let Some(parent) = nodes.parent(id)
        && nodes.kind(parent) == SyntaxKind::PropertyAccessExpression
        && map.get(parent).and_then(|p| p.name_id()) == Some(id)
        && let Some(Node::PropertyAccessExpression(access)) = map.get(parent)
    {
        return format!("member name, {}", access_reason(checker, access, nodes));
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
            // A member name no longer reaches here — it is typed as its property
            // access above (`bd tsr-tl8`) — but the label is kept for the other
            // name-shaped positions that still do, such as a JSX namespaced name.
            let member = nodes
                .parent(id)
                .filter(|&parent| map.get(parent).and_then(|p| p.name_id()) == Some(id))
                .map(|parent| format!("the name of a {:?}", nodes.kind(parent)));
            let what = member.unwrap_or_else(|| "reference".to_string());
            // `SymbolFlags::VALUE`, mirroring `expressions.rs`'s identifier arm.
            // The meaning is not cosmetic here: a narrower or wider one than the
            // checker's would reclassify gap lines in the histogram, attributing
            // them to a cause the checker did not take.
            return match identifier.node_id.and_then(|n| {
                binder.resolve_name(nodes, map, n, identifier.text, SymbolFlags::VALUE)
            }) {
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
        if let tsr_ast::Expression::PropertyAccessExpression(access) = expression {
            return format!("property access, {}", access_reason(checker, access, nodes));
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

    /// Every `{text} : {type}` pair for a source, through the **real** checker.
    ///
    /// [`texts`] stubs the type out, which is right for the walker tests and
    /// useless for anything about `type_at_location`. This runs the whole path.
    fn typed(source: &str) -> Vec<(String, String)> {
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "fixture must parse: {source:?}");
        let bound = tsr_binder::bind(
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "t.ts", text: source },
        );
        let mut checker = tsr_checker::Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        assertions_for_file(
            &Node::SourceFile(parsed.source_file),
            source,
            &parsed.nodes,
            &parsed.node_map,
            |id| type_at_location(&mut checker, &bound, &parsed.nodes, &parsed.node_map, id),
        )
        .into_iter()
        .map(|a| (a.text, a.type_string))
        .collect()
    }

    /// [`typed`], for a source that has to be parsed as `.tsx`.
    fn typed_tsx(source: &str) -> Vec<(String, String)> {
        let arena = tsr_core::Arena::new();
        let parsed =
            tsr_parser::parse_with_script_kind(&arena, source, tsr_parser::ScriptKind::Tsx);
        assert!(parsed.diagnostics.is_empty(), "fixture must parse: {source:?}");
        let bound = tsr_binder::bind(
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "t.tsx", text: source },
        );
        let mut checker = tsr_checker::Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        assertions_for_file(
            &Node::SourceFile(parsed.source_file),
            source,
            &parsed.nodes,
            &parsed.node_map,
            |id| type_at_location(&mut checker, &bound, &parsed.nodes, &parsed.node_map, id),
        )
        .into_iter()
        .map(|a| (a.text, a.type_string))
        .collect()
    }

    #[test]
    fn an_intrinsic_jsx_tag_name_prints_any() {
        // The whole population this rule is for. Our checker answers the error
        // type for `div` — correctly, and the same as upstream's — so without
        // the writer guard both tag names print `error` and every one of the
        // corpus's 1,779 intrinsic tag-name lines is a mismatch.
        //
        // Both the opening and the closing tag name get a line, which is why
        // `div` appears twice; a guard keyed only on `JsxOpeningElement` would
        // fix half the population and look like a whole fix.
        let out = typed_tsx("const e = <div></div>;");
        let tags: Vec<_> =
            out.iter().filter(|(text, _)| text == "div").map(|(_, ty)| ty.as_str()).collect();
        assert_eq!(tags, ["any", "any"], "in {out:?}");

        // A hyphenated custom element is the second clause of
        // `IsIntrinsicJsxName` and is intrinsic despite no lowercase-only rule
        // reaching it. `conformance/jsxParsingError4` carries this shape.
        let out = typed_tsx("const e = <x-foo></x-foo>;");
        let tags: Vec<_> =
            out.iter().filter(|(text, _)| text == "x-foo").map(|(_, ty)| ty.as_str()).collect();
        assert_eq!(tags, ["any", "any"], "in {out:?}");
    }

    #[test]
    fn a_lowercase_tag_name_that_resolves_keeps_its_own_type() {
        // Upstream's `IsTypeAny` precondition, and the case that separates a
        // port of the guard from "a lowercase tag prints `any`". `foo` is an
        // intrinsic *name* by `IsIntrinsicJsxName` — it is lowercase — but it
        // resolves to a value, so the writer never reaches the fast path and
        // prints the real type. The corpus has 32 lines printing `() => any`
        // and 24 printing `typeof foo` that depend on this.
        let out = typed_tsx("const foo = () => 1;\nconst e = <foo/>;");
        let tag = out
            .iter()
            .filter(|(text, _)| text == "foo")
            .map(|(_, ty)| ty.as_str())
            .next_back()
            .expect("a tag name line");
        assert_ne!(tag, "any", "in {out:?}");
        assert_eq!(tag, "() => number", "in {out:?}");
    }

    #[test]
    fn a_label_name_prints_any() {
        // Both halves of `ast.IsLabelName` — the label *of* a labeled statement
        // and the *target* of a jump — in one fixture, because they are separate
        // predicates upstream and a guard keyed on only the first fixes half the
        // population while looking like a whole fix.
        //
        // Our checker answers the error type for both, correctly and for
        // upstream's reason: a label is not a value and the name does not
        // resolve. The writer is what turns that into `any`. The corpus arm is
        // 209 lines, all 209 answering `any` upstream with an empty residue — the
        // signature of a position where upstream always holds `errorType`. See
        // docs/architecture/checker-notes-guard.md.
        let out = typed("outer: while (true) { break outer; }");
        let labels: Vec<_> =
            out.iter().filter(|(text, _)| text == "outer").map(|(_, ty)| ty.as_str()).collect();
        assert_eq!(labels, ["any", "any"], "in {out:?}");
    }

    #[test]
    fn a_label_shadowing_a_value_keeps_the_type_we_computed() {
        // Upstream's `IsTypeAny` precondition, and the case that separates a port
        // of the guard from "a label position prints `any`". A label shares no
        // namespace with a value upstream, but *this* checker resolves the
        // identifier to the variable and answers `1` — so the precondition is not
        // satisfied and the guard must not fire.
        //
        // The plausible wrong implementation — convert every label position
        // unconditionally — prints `any` on the label lines below. Those are
        // lines the 209-line measurement never claimed, so converting them would
        // ship an unmeasured change under a measured one.
        let out = typed("const outer = 1;\nouter: while (true) { outer; }");
        let labels: Vec<_> =
            out.iter().filter(|(text, _)| text == "outer").map(|(_, ty)| ty.as_str()).collect();
        assert_eq!(labels, ["1", "1", "1"], "in {out:?}");
    }

    #[test]
    fn a_binding_element_property_name_prints_any() {
        // The population the rule is for, and the discrimination that matters:
        // the *property* name converts and the *bound* name must not. Upstream's
        // guard covers both, and porting it whole is what the measurement says
        // not to do — the bound-name position converts 663 of 2,640 with a
        // 1,977-line residue, so `any` there would overwrite lines where
        // upstream printed a real type.
        //
        // `x` has no declared type here, so our checker gaps on `b` and prints
        // `error` for it. That is the honest answer and it stays.
        let out = typed("declare const x: any;\nconst { a: b } = x;");
        let a = out.iter().find(|(text, _)| text == "a").map(|(_, ty)| ty.as_str());
        assert_eq!(a, Some("any"), "in {out:?}");

        // A nested pattern: the property name of an inner binding element is the
        // same position and must convert too. A rule keyed on the outer
        // declaration would fix half the corpus population — the
        // `sourceMapValidationDestructuring*NestedObjectBindingPattern` cases are
        // 9 lines each and are exactly this shape.
        let out = typed("declare const x: any;\nconst { a: { c: d } } = x;");
        let names: Vec<_> = out
            .iter()
            .filter(|(text, _)| text == "a" || text == "c")
            .map(|(_, ty)| ty.as_str())
            .collect();
        assert_eq!(names, ["any", "any"], "in {out:?}");
    }

    #[test]
    fn a_binding_element_property_name_shadowing_a_value_keeps_the_type_we_computed() {
        // Upstream's `IsTypeAny` precondition, and the case that separates a port
        // of the sub-position from "a binding element's property name prints
        // `any`". Upstream's `getTypeOfNode` falls through to `errorType` for `a`
        // whatever else `a` means in the file; *this* producer reaches the
        // expression fall-through and resolves the identifier to the outer
        // `const a`, answering `1`.
        //
        // The plausible wrong implementation — convert the position
        // unconditionally — prints `any` on the second `a` below. Those lines
        // were never in the 428 the measurement claimed.
        let out = typed("const a = 1;\ndeclare const x: any;\nconst { a: b } = x;");
        let names: Vec<_> =
            out.iter().filter(|(text, _)| text == "a").map(|(_, ty)| ty.as_str()).collect();
        assert_eq!(names, ["1", "1"], "in {out:?}");
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
    fn the_base_of_an_extends_clause_is_the_base_type_and_a_reference_is_still_typeof() {
        // 1,086 corpus lines, and every one of them a *wrong* answer rather than
        // a gap. `class B extends A {}` records `>A : A`, not `>A : typeof A`.
        //
        // **Both directions, on the same source**, because the two are different
        // questions with different right answers and a fix for either one breaks
        // the other if it is applied indiscriminately. Routing everything through
        // `getDeclaredTypeOfSymbol` would make the heritage line pass and take
        // the `typeof` answer bucket — 15,912 corpus lines — to zero.
        let pairs = typed("abstract class A {}\nclass B extends A {}\nconst x = A;");
        assert_eq!(
            pairs,
            vec![
                ("A".to_string(), "A".to_string()),        // declaration name
                ("B".to_string(), "B".to_string()),        // declaration name
                ("A".to_string(), "A".to_string()),        // the base of `extends`
                ("x".to_string(), "typeof A".to_string()), // a variable holding it
                ("A".to_string(), "typeof A".to_string()), // a value reference
            ],
        );
    }

    #[test]
    fn the_left_of_a_qualified_name_is_any_except_inside_a_typeof() {
        // Both directions, because the first version of this rule had only one
        // and cost about 120 lines in the `typeof` bucket. `A.Outer` in type
        // position falls through `getTypeOfNode` to `errorType` and the writer
        // prints `any`; `typeof M.C` makes `M` an *expression node*, so it keeps
        // its value type. Pinned by `conformance/recursiveTypesWithTypeof.types`.
        let in_type = typed("namespace A { export type Outer = { id: string } }\nlet w: A.Outer;");
        assert!(
            in_type.contains(&("A".to_string(), "any".to_string())),
            "the left of a qualified type name is `any`: {in_type:?}"
        );

        let in_query = typed("namespace M { export class C {} }\ndeclare const v: typeof M.C;");
        assert!(
            in_query.contains(&("M".to_string(), "typeof M".to_string())),
            "inside a type query the left keeps its value type: {in_query:?}"
        );
    }

    #[test]
    fn the_entity_name_of_an_import_equals_keeps_its_value_type() {
        // Both directions in one test, because a single-direction test here
        // passes by breaking the other side: the corpus carries 133 lines that
        // want the value type and **34 that want `any`**, under the same
        // enclosing kind.
        //
        // Upstream reaches this through `isInRightSideOfImportOrExportAssignment`
        // — `getDeclaredTypeOfSymbol`, falling back to `getTypeOfSymbol` only
        // when that is the error type. For a namespace the declared type is the
        // error type, so the answer is the value type.
        // **Whole-vector equality, not `contains`.** The first version of this
        // test used `contains(&("M", "typeof M"))` and stayed green under a
        // mutation that disabled the rule entirely — because `namespace M`
        // emits its *own* line `("M", "typeof M")` at index 0, and the entity
        // name at index 4 is a different line with the same pair. A `contains`
        // over a bag of pairs cannot tell one occurrence from another, so it
        // asked a vacuously true question.
        assert_eq!(
            typed("namespace M { export const a = 1; }\nimport x = M.a;"),
            vec![
                ("M".to_string(), "typeof M".to_string()), // the declaration
                ("a".to_string(), "1".to_string()),
                ("1".to_string(), "1".to_string()),
                ("x".to_string(), "error".to_string()),
                ("M".to_string(), "typeof M".to_string()), // the entity name
                ("a".to_string(), "error".to_string()),
            ],
        );

        // **The declared-then-value ORDER, pinned.** A namespace cannot pin it:
        // its declared type *is* the error type, so both orders give the same
        // answer, and dropping the declared half left the two assertions above
        // green. An enum can — `getDeclaredTypeOfSymbol` returns the enum type,
        // which is not the error type, so upstream answers `E` and never reaches
        // `getTypeOfSymbol`. Without the declared half this reads `typeof E`.
        assert_eq!(
            typed("enum E { A }\nimport q = E.A;"),
            vec![
                ("E".to_string(), "E".to_string()), // the declaration
                // The member's *declaration name*, which `enumAssignmentCompat5.types`
                // records as `>A : E.A`. This read `error` until `getTypeOfSymbol`
                // grew its `ENUM_MEMBER` arm; the pin was the gap, not the answer.
                ("A".to_string(), "E.A".to_string()),
                ("q".to_string(), "error".to_string()),
                ("E".to_string(), "E".to_string()), // the entity name: DECLARED
                // Still a gap, and a different question: `E.A` on the right of a
                // qualified name, which no arm answers.
                ("A".to_string(), "error".to_string()),
            ],
        );

        // The other direction: a left that resolves to no namespace keeps the
        // `any` the general qualified-name rule gives it. This is the guard that
        // stops the rule turning 34 right answers into wrong ones.
        assert_eq!(
            typed("import y = Missing.thing;"),
            vec![
                ("y".to_string(), "error".to_string()),
                ("Missing".to_string(), "any".to_string()),
                ("thing".to_string(), "error".to_string()),
            ],
        );
    }

    #[test]
    fn a_base_that_names_no_type_falls_through_instead_of_being_hijacked() {
        // The branch resolves in `SymbolFlags::TYPE` meaning and bails when that
        // finds nothing, so a base naming a *value* keeps the answer the
        // expression path gives it — which is upstream's answer too.
        assert_eq!(
            typed("declare const V: any;\nclass C extends V {}"),
            vec![
                ("V".to_string(), "any".to_string()),
                ("C".to_string(), "C".to_string()),
                ("V".to_string(), "any".to_string()),
            ],
        );

        // **Three of the branch's guards are unobservable, and that is recorded
        // rather than covered by tests that would not bite.** Measured, not
        // assumed — each was mutated and turned nothing red:
        //
        // - **the `extends` token test** and **the class-like owner test**.
        //   Upstream's predicate is
        //   `IsExpressionWithTypeArgumentsInClassExtendsClause`
        //   (`ast/utilities.go:1426`), and both halves matter *there*. Here
        //   neither `implements` nor `interface I extends J` produces an
        //   assertion at all, because the walker drops both as type nodes — so
        //   no producer test can separate a guard that checks them from one that
        //   does not. The two assertions below pin that emptiness, which is the
        //   only part of the claim that is checkable.
        // - **`SymbolFlags::TYPE` rather than `VALUE`**. A class declares both a
        //   type and a value under one symbol, so both meanings resolve to it.
        //   The meaning would start to matter for a base that is a value and not
        //   a type, and that case falls through on the `declared != error` test
        //   instead — which *is* pinned, by this test's first assertion.
        //
        // All three are kept because each is upstream's, and because the first
        // two become load-bearing the moment the walker's treatment of a
        // heritage clause changes.
        assert!(typed("interface I {}\nclass C implements I {}").iter().all(|(t, _)| t != "I"));
        assert!(typed("interface J {}\ninterface I extends J {}").is_empty());
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

    // ---- The program the rewire builds -------------------------------------

    fn synthetic_case(source: &str) -> crate::TestCase {
        crate::TestCase::parse("compiler/synthetic", "synthetic.ts", source)
    }

    fn sections(names: &[&str]) -> Vec<FileTypes> {
        names
            .iter()
            .map(|name| FileTypes { file: (*name).to_string(), assertions: Vec::new() })
            .collect()
    }

    /// The structural claim of the rewire, asserted where the checker's own
    /// maturity cannot reach it.
    ///
    /// A `.types` line is only as good as the checker, so a test asserting a
    /// *rendered type* would measure the checker and report it as the producer.
    /// What the rewire changed is which files are in scope when a unit is
    /// checked, so that is what this asserts: both units and the bundled libs,
    /// in one program, with a lib global resolving from a case unit.
    #[test]
    fn a_case_is_one_program_holding_its_units_and_the_libs() {
        let case = synthetic_case(concat!(
            "// @filename: a.ts\n",
            "export const a = 1;\n",
            "// @filename: b.ts\n",
            "import { a } from \"./a\";\n",
            "const b = a;\n",
        ));
        let arena = tsr_core::Arena::new();
        let program = program_for_case(&arena, &case);

        assert!(program.source_file("a.ts").is_some());
        assert!(program.source_file("b.ts").is_some());
        if bundled_libs().is_empty() {
            return; // the submodule is not checked out; see docs/conventions.md
        }
        assert!(
            !program.lib_files().is_empty(),
            "a case's program loads the bundled libs — the rewire's whole point"
        );
        // Not merely present: bound into the same store, which is what per-file
        // identity could not do. `Array` is declared in `lib.es5.d.ts` and is
        // the name the corpus asks for most.
        let root = program.source_file("b.ts").expect("b.ts is in the program").source_file();
        let resolved = root.node_id.and_then(|id| {
            program.binder().resolve_name(
                program.nodes(),
                program.node_map(),
                id,
                "Array",
                SymbolFlags::TYPE,
            )
        });
        assert!(resolved.is_some(), "a lib global resolves from a case unit");
    }

    /// The regression this rewire could most easily cause and least easily
    /// notice: every case rendering nothing, which reads in the gate as a
    /// checker gap rather than as a broken producer.
    #[test]
    fn a_single_unit_case_still_renders_its_lines() {
        let case = synthetic_case("const x = 1;\n");
        let rendered = assertions_for_case(&case, &sections(&["synthetic.ts"]), false);
        assert_eq!(rendered.len(), 1);
        assert!(
            rendered[0].iter().any(|a| a.text == "x" && a.type_string == "1"),
            "expected a line for `x`, got {:?}",
            rendered[0].iter().map(Assertion::line).collect::<Vec<_>>()
        );
    }

    /// A section naming a unit the program has no file for renders empty,
    /// rather than panicking or borrowing another unit's tree — which is a live
    /// possibility now that one node table holds every unit.
    #[test]
    fn a_section_with_no_unit_renders_empty() {
        let case = synthetic_case("const x = 1;\n");
        let rendered = assertions_for_case(&case, &sections(&["absent.ts"]), false);
        assert_eq!(rendered, vec![Vec::new()]);
    }
}
