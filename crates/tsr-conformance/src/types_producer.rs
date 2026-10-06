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
/// The files of `tests/lib`, mounted under `/.lib` — upstream's
/// `testLibFolder` (`harnessutil.go:39`). Read once, like [`bundled_libs`],
/// and sorted for the same first-in-wins reason.
fn test_lib_files() -> &'static [(String, String)] {
    static LIBS: OnceLock<Vec<(String, String)>> = OnceLock::new();
    LIBS.get_or_init(|| {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .map(|root| root.join("vendor/typescript-go/_submodules/TypeScript/tests/lib"));
        let Some(dir) = dir else { return Vec::new() };
        let mut libs = Vec::new();
        let mut walk = vec![(dir, String::new())];
        while let Some((directory, prefix)) = walk.pop() {
            let Ok(entries) = std::fs::read_dir(&directory) else { continue };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = entry.path();
                if path.is_dir() {
                    walk.push((path, format!("{prefix}{name}/")));
                } else if let Ok(text) = std::fs::read_to_string(&path) {
                    libs.push((format!("/.lib/{prefix}{name}"), text));
                }
            }
        }
        libs.sort();
        libs
    })
}

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

/// Whether an identifier IS the name of an import or export statement.
///
/// §248, extracted from §178's inline condition so the WRITER can ask it too.
/// Ported from `isImportStatementName`/`isExportStatementName`
/// (`type_symbol_baseline.go:458-479`).
fn is_import_or_export_statement_name(id: NodeId, nodes: &NodeTable, map: &NodeMap<'_>) -> bool {
    if nodes.kind(id) != SyntaxKind::Identifier {
        return false;
    }
    let Some(parent) = nodes.parent(id) else { return false };
    match map.get(parent) {
        Some(Node::ImportSpecifier(specifier)) => {
            specifier.name.and_then(|n| n.node_id) == Some(id)
                || specifier.property_name.and_then(|n| n.node_id()) == Some(id)
        }
        Some(Node::ImportClause(clause)) => clause.name.and_then(|n| n.node_id) == Some(id),
        Some(Node::ImportEqualsDeclaration(declaration)) => {
            declaration.name.and_then(|n| n.node_id) == Some(id)
        }
        Some(Node::ExportAssignment(assignment)) => {
            assignment.expression.and_then(|e| e.node_id()) == Some(id)
        }
        Some(Node::ExportSpecifier(specifier)) => {
            specifier.name.and_then(|n| n.node_id()) == Some(id)
                || specifier.property_name.and_then(|n| n.node_id()) == Some(id)
        }
        _ => false,
    }
}

/// Whether `id` is an `ExpressionWithTypeArguments` in a class's `extends`
/// clause — upstream's `TryGetClassImplementingOrExtendingExpressionWithTypeArguments`
/// (`ast/utilities.go:1438`) with its `!isImplements` half applied.
fn is_ewta_in_class_extends_clause(id: NodeId, nodes: &NodeTable, map: &NodeMap<'_>) -> bool {
    if nodes.kind(id) != SyntaxKind::ExpressionWithTypeArguments {
        return false;
    }
    let Some(clause) = nodes.parent(id) else { return false };
    if nodes.kind(clause) != SyntaxKind::HeritageClause {
        return false;
    }
    let extends = match map.get(clause) {
        Some(Node::HeritageClause(heritage)) => heritage.token.kind == SyntaxKind::ExtendsKeyword,
        _ => false,
    };
    extends
        && matches!(
            nodes.parent(clause).map(|owner| nodes.kind(owner)),
            Some(SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression)
        )
}

/// `isInRightSideOfImportOrExportAssignment` (`checker/utilities.go:1107`):
/// walk up through qualified names, then the outermost must be an
/// import-equals' module reference or an export assignment's expression.
/// Answers which of the two, by kind.
fn right_side_of_import_or_export_assignment(
    id: NodeId,
    nodes: &NodeTable,
    map: &NodeMap<'_>,
) -> Option<SyntaxKind> {
    let mut node = id;
    let mut parent = nodes.parent(node)?;
    while nodes.kind(parent) == SyntaxKind::QualifiedName {
        node = parent;
        parent = nodes.parent(node)?;
    }
    match map.get(parent)? {
        Node::ImportEqualsDeclaration(declaration)
            if declaration.module_reference.and_then(|reference| reference.node_id())
                == Some(node) =>
        {
            Some(SyntaxKind::ImportEqualsDeclaration)
        }
        Node::ExportAssignment(assignment)
            if assignment.expression.and_then(|expression| expression.node_id()) == Some(node) =>
        {
            Some(SyntaxKind::ExportAssignment)
        }
        _ => None,
    }
}

/// `getDeclaredTypeOfSymbol` (`checker.go:23670`) with
/// `tryGetDeclaredTypeOfSymbol`'s alias arm (`checker.go:23690`), which it
/// tests after every type meaning: an alias not merged with a type
/// declaration answers `getDeclaredTypeOfAlias`.
fn declared_type_of_symbol(
    checker: &mut tsr_checker::Checker<'_, '_>,
    binder: &tsr_binder::BindResult<'_>,
    symbol: tsr_binder::SymbolId,
) -> tsr_checker::TypeId {
    let flags = binder.symbols().get(symbol).flags;
    if flags.intersects(SymbolFlags::ALIAS)
        && !flags.intersects(
            SymbolFlags::CLASS
                | SymbolFlags::INTERFACE
                | SymbolFlags::TYPE_PARAMETER
                | SymbolFlags::TYPE_ALIAS
                | SymbolFlags::ENUM
                | SymbolFlags::ENUM_MEMBER,
        )
    {
        return checker.get_declared_type_of_alias(symbol);
    }
    checker.get_declared_type_of_symbol(symbol)
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
    // `lineDelimiter` in tsbaseline/util.go is \r?\n, not every ECMAScript
    // line break. A lone CR, LS or PS inside a template remains source text.
    raw.replace("\r\n", "").replace('\n', "")
}

/// Advance past whitespace and comments, as `scanner.SkipTrivia` does.
fn skip_trivia(source: &str, mut pos: usize) -> usize {
    let bytes = source.as_bytes();
    if pos == 0 && tsr_scanner::is_shebang_trivia(source) {
        pos = tsr_scanner::scan_shebang_trivia(source);
    }
    while pos < bytes.len() {
        match bytes[pos] {
            b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c => pos += 1,
            b'/' if bytes.get(pos + 1) == Some(&b'/') => {
                pos += 2;
                pos += source[pos..]
                    .find(['\r', '\n', '\u{2028}', '\u{2029}'])
                    .unwrap_or(source.len() - pos);
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
            byte if !byte.is_ascii() => {
                let Some(ch) = source[pos..].chars().next() else { break };
                if !tsr_scanner::is_whitespace_single_line(ch) && !tsr_scanner::is_line_break(ch) {
                    break;
                }
                pos += ch.len_utf8();
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
pub fn type_at_location<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
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
///
/// The body itself is [`type_id_at_location_tracking`]; this discards its
/// `saw_checker_error` flag, which is what every caller but a gap-root board
/// wants.
pub fn type_id_at_location<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    id: NodeId,
) -> tsr_checker::TypeId {
    type_id_at_location_tracking(checker, binder, nodes, map, id, &mut false)
}

/// [`type_id_at_location`] plus the one fact it throws away: **did the CHECKER
/// answer `errorType` at this position while the producer printed `any`?**
///
/// # Why this exists, and why it is not a new classifier
///
/// The producer converts `error` to `any` on three branches below, faithfully —
/// upstream's own baseline writer does it (`type_symbol_baseline.go:383`), so a
/// position where *upstream's* checker holds `errorType` records `any` there too.
/// The consequence is that **a line where this port computed nothing is
/// indistinguishable, in the baseline text, from one where it computed `any`** —
/// which is ADR-0038's ceiling phenomenon at a specific class of positions.
///
/// That costs the project its ability to rank its own remaining work.
/// `examples/depend.rs`, `gaproot.rs` and `cyclegap.rs` all select lines by
/// `type_string == "error"`, so all three are **blind** to this population.
/// `STATUS.md` §4.-5's correction measured it at **8,826 of 13,295** audited
/// `any` lines — the gap column reads ~7,400 while the "computed nothing"
/// population is more than twice that.
///
/// `examples/any_audit.rs` can already see it, but only because it **mirrors**
/// this function's branch order in a parallel implementation. Exposing the fact
/// here instead is the `tsr_conformance::verdict` precedent: one computation, so
/// two probes cannot drift.
///
/// **Behaviour is unchanged by construction.** The returned `TypeId` is the same
/// on every path; `saw_checker_error` is written and never read internally, and
/// [`type_id_at_location`] delegates here with a throwaway. A `scorepair` run
/// across this change must read *no transitions*.
pub fn type_id_at_location_tracking<'a>(
    checker: &mut tsr_checker::Checker<'a, '_>,
    binder: &tsr_binder::BindResult<'a>,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
    id: NodeId,
    saw_checker_error: &mut bool,
) -> tsr_checker::TypeId {
    let error = checker.intrinsics().error;
    let Some(node) = map.get(id) else { return error };

    // The writer's base-class workaround, `type_symbol_baseline.go:370-374`:
    //
    // ```go
    // // Workaround to ensure we output 'C' instead of 'typeof C' for base class expressions
    // if ast.IsExpressionWithTypeArgumentsInClassExtendsClause(node.Parent) {
    //     t = fileChecker.GetTypeAtLocation(node.Parent)
    // }
    // if t == nil || checker.IsTypeAny(t) { t = fileChecker.GetTypeAtLocation(node) }
    // ```
    //
    // `GetTypeAtLocation(EWTA)` lands in `getTypeOfNode`'s class-extends arm
    // (`checker.go:31927`): an extends-clause EWTA is neither part of a type
    // node nor an expression node, so it answers
    // `getTypeWithThisArgument(getBaseTypes(classType)[0], thisType)`, or
    // errorType when the class has no base type. This port's references carry
    // no `this` argument, so `getTypeWithThisArgument` prints the base itself.
    // A missing or any-flagged base falls through to the node's own type.
    if let Some(parent) = nodes.parent(id)
        && is_ewta_in_class_extends_clause(parent, nodes, map)
        && let Some(Node::ExpressionWithTypeArguments(entry)) = map.get(parent)
        && entry.expression.and_then(|e| e.node_id()) == Some(id)
        && let Some(class) = nodes.parent(parent).and_then(|clause| nodes.parent(clause))
        && let Some(class_symbol) = binder.symbol_of(class)
        && let Some(&base) = checker.get_base_types(class_symbol).first()
        && !checker.type_of(base).flags.contains(tsr_checker::flags::TypeFlags::ANY)
    {
        return base;
    }

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
        // SS183 (`!ast.IsPropertyAccessOrQualifiedName(node.Parent)`,
        // type_symbol_baseline.go:383): a name whose parent IS a property
        // access never takes the intrinsic fast path — it renders through
        // the node builder, so an any-flagged answer prints `any`. The
        // module comment above claimed this guard was "already covered by a
        // rule reached along a different route"; it is not, for the case
        // where the ACCESS ITSELF fails: `Obj.fn = function(){}` records
        // `>Obj.fn : error` beside `>fn : any` — the same errorType, the
        // whole access taking the fast path and its NAME not.
        if computed == error || computed == checker.intrinsics().any {
            *saw_checker_error |= computed == error;
            return checker.intrinsics().any;
        }
        return computed;
    }

    // §841: the qualified half of `isRightSideOfQualifiedNameOrPropertyAccess`.
    //
    // Upstream's predicate is one function covering both spellings; the arm
    // above ported the property-access half and this is the other. Without it
    // the `foo` of `typeof properties.foo`, and the `properties.foo`
    // `QualifiedName` itself, fall through to the free-identifier path and
    // answer `any` — `bd tsr-tl8`'s exact bug, live in the other grammar.
    //
    // **The type-query gate is not optional.** A `QualifiedName` is usually a
    // TYPE name — the `M.I` of `let x: M.I` — and the oracle prints a type
    // there, not a value type; typing every qualified name as a value
    // expression would turn a large population of correct type-reference lines
    // into confident wrong ones. Inside `typeof`, and only there, a qualified
    // name denotes a value. This is the same gate the binder applies when it
    // decides to record a flow node for one at all
    // (`vendor/typescript-go/internal/binder/binder.go:605-608`).
    if is_part_of_type_query(id, nodes)
        && let Some(qualified) = qualified_name_to_check(id, nodes, map)
    {
        let computed = checker.check_qualified_name(qualified);
        if computed == error || computed == checker.intrinsics().any {
            *saw_checker_error |= computed == error;
            return checker.intrinsics().any;
        }
        return computed;
    }

    // §296: the PROPERTY NAME of an import/export specifier — the `default`
    // of `export { default as A } from "./a"` — types as the specifier's own
    // aliased target, exactly as its NAME does; upstream records both lines
    // identically (`plainJSGrammarErrors2` wants `default : 1` beside
    // `A : 1`). Without this the token fell to the free-identifier path and
    // resolved nothing.
    if let Some(parent) = nodes.parent(id)
        && let Some(specifier_symbol) = binder.symbol_of(parent)
    {
        let is_property_name = match map.get(parent) {
            Some(Node::ImportSpecifier(specifier)) => {
                specifier.property_name.and_then(|name| name.node_id()) == Some(id)
            }
            Some(Node::ExportSpecifier(specifier)) => {
                specifier.property_name.and_then(|name| name.node_id()) == Some(id)
            }
            _ => false,
        };
        if is_property_name {
            // `IsDeclarationNameOrImportPropertyName` (`ast/utilities.go:1311`)
            // takes a specifier's property name, and `getSymbolAtLocation`
            // answers it with `getImmediateAliasedSymbol` of the specifier
            // (`checker.go:31594`) — `resolve_alias`, which is the one-step
            // `getTargetOfAliasDeclaration`. A miss is `errorType`, which
            // the writer's import/export-statement-name guard prints `any`.
            //
            // One miss is this port's, not upstream's: a local `export { x
            // as y }` resolves `x` in scope (`getTargetOfExportSpecifier`),
            // and upstream's globals hold `globalThisSymbol` and
            // `undefinedSymbol` (`checker.go:963`), which this binder has no
            // entry for — the checker mints their types in
            // `checkIdentifier`'s unresolved arm instead (§33). Typing the
            // name as that identifier reads the same stand-in;
            // any other unresolved name answers `errorType` there too.
            // `globalThisGlobalExportAsGlobal` pins it.
            let local_export = matches!(map.get(parent), Some(Node::ExportSpecifier(_)))
                && matches!(
                    nodes.parent(parent).and_then(|clause| nodes.parent(clause)).and_then(|d| map.get(d)),
                    Some(Node::ExportDeclaration(declaration)) if declaration.module_specifier.is_none()
                );
            let computed = match checker.resolve_alias(specifier_symbol) {
                Some(target) => checker.get_type_of_symbol(binder.merged_symbol(target)),
                None if local_export => tsr_ast::Expression::try_from(node)
                    .map_or(error, |expression| checker.check_expression(expression)),
                None => error,
            };
            if computed == error {
                *saw_checker_error = true;
                return checker.intrinsics().any;
            }
            return computed;
        }
    }

    // §244, checker-1's handoff, and **the placement is the whole build**.
    //
    // `IsTypeDeclaration` (`ast/utilities.go:3585`):
    //
    // ```go
    // case KindImportSpecifier, KindExportSpecifier:
    //     return node.Parent.Parent.IsTypeOnly()
    // ```
    //
    // A specifier inside a type-only clause IS a type declaration, its name is
    // an `IsTypeDeclarationName` (`:3598`), and `getTypeOfNode` takes the
    // `getDeclaredTypeOfSymbol` branch. Upstream's ordering puts that ahead of
    // the declaration-name branch below, which asks `getTypeOfSymbol` and gets
    // the error type for a type-only symbol.
    //
    // ```text
    // interface A {}
    // export type { A };
    // >A : A            <- upstream; this port answered `error`
    // ```
    //
    // Witness `conformance/typeOnlyMerge1`, the same-file one of the two, so
    // the rule is isolated from the cross-file question.
    //
    // **This arm measured +0 when it sat lower in the function**, ahead of the
    // import/export writer guard but *behind* the declaration-name arm below —
    // which answers first for exactly these nodes and returns the alias's
    // value type. Every ingredient was already correct at that point:
    // `symbol_of` on the specifier answers, `resolve_alias` reaches the
    // interface, and its declared type prints `A`. Only the position was
    // wrong, which is why the zero looked like a missing capability. Ordering
    // is upstream's rule here in the same way it is in `getTypeOfNode`.
    //
    // `IsTypeDeclarationName` (`ast/utilities.go:3598`) covers the name of a
    // type-only `ImportClause` too (`IsTypeDeclaration`'s `KindImportClause`
    // arm), and only the declaration's NAME (`GetNameOfDeclaration`), not a
    // specifier's property name. The answer is `getDeclaredTypeOfSymbol` of
    // the alias, i.e. `getDeclaredTypeOfAlias` (`checker.go:24094`): the
    // declared type of the resolved target, errorType when it declares no
    // type — there is no fallthrough to the value type.
    if nodes.kind(id) == SyntaxKind::Identifier
        && let Some(parent) = nodes.parent(id)
        && map.get(parent).and_then(|p| p.name_id()) == Some(id)
        && match map.get(parent) {
            // An import clause spells type-only with its PHASE MODIFIER token,
            // not a bool — `import defer` is a different phase and must not
            // qualify.
            Some(Node::ImportClause(clause)) => {
                clause.phase_modifier.is_some_and(|token| token.kind == SyntaxKind::TypeKeyword)
            }
            Some(Node::ImportSpecifier(_) | Node::ExportSpecifier(_)) => {
                match nodes.parent(parent).and_then(|p| nodes.parent(p)).and_then(|g| map.get(g)) {
                    Some(Node::ImportClause(clause)) => clause
                        .phase_modifier
                        .is_some_and(|token| token.kind == SyntaxKind::TypeKeyword),
                    Some(Node::ExportDeclaration(declaration)) => declaration.is_type_only,
                    _ => false,
                }
            }
            _ => false,
        }
        && let Some(symbol) = binder.symbol_of(parent)
    {
        // `tryGetDeclaredTypeOfSymbol` (`checker.go:23678`) tests the type
        // meanings before the alias arm, so an alias merged with a local type
        // declaration answers that declaration's type.
        let symbol = binder.merged_symbol(symbol);
        let flags = binder.symbols().get(symbol).flags;
        if flags.intersects(
            SymbolFlags::CLASS
                | SymbolFlags::INTERFACE
                | SymbolFlags::TYPE_PARAMETER
                | SymbolFlags::TYPE_ALIAS
                | SymbolFlags::ENUM
                | SymbolFlags::ENUM_MEMBER,
        ) {
            return checker.get_declared_type_of_symbol(symbol);
        }
        return match checker.resolve_alias(symbol) {
            Some(target) => checker.get_declared_type_of_symbol(target),
            None => error,
        };
    }

    // A declaration name resolves through its parent's symbol.
    if let Some(parent) = nodes.parent(id)
        && map.get(parent).and_then(|p| p.name_id()) == Some(id)
        && let Some(symbol) = binder.symbol_of(parent)
    {
        let computed = checker.get_type_of_symbol(symbol);
        return computed;
    }

    // `isInRightSideOfImportOrExportAssignment` (`checker/utilities.go:1107`),
    // `getTypeOfNode`'s arm after the binding-pattern one (`checker.go:31927`):
    // a name in an `import a = b.c` module reference or the expression of an
    // `export =` / `export default`. Neither is an expression node
    // (`IsInExpressionContext` has no import-equals or export-assignment
    // parent arm), so nothing above claims them. `getSymbolAtLocation`
    // routes both through `getSymbolOfNameOrPropertyAccessExpression`
    // (`checker.go:31780`): the export-assignment arm resolves every meaning
    // including `Alias`, the import-equals one is
    // `getSymbolOfPartOfRightHandSideOfImportEquals` (`checker.go:14474`).
    // The answer is the declared type — through the alias for an alias,
    // `tryGetDeclaredTypeOfSymbol`'s last arm — and the value type when that
    // is the error type. A miss falls off the end of `getTypeOfNode`:
    // `errorType`, which the writer prints `any` for an export-assignment
    // expression (`isExportStatementName`) and for a qualified-name part
    // (`IsPropertyAccessOrQualifiedName(node.Parent)`,
    // `type_symbol_baseline.go:380`).
    if nodes.kind(id) == SyntaxKind::Identifier
        && let Some(parent) = nodes.parent(id)
        && let Some(assignment) = right_side_of_import_or_export_assignment(id, nodes, map)
    {
        let symbol = match assignment {
            SyntaxKind::ExportAssignment => checker.get_symbol_of_export_assignment_expression(id),
            _ => checker.get_symbol_of_part_of_right_hand_side_of_import_equals(id),
        };
        let computed = match symbol {
            Some(symbol) => {
                let declared = declared_type_of_symbol(checker, binder, symbol);
                if declared == error { checker.get_type_of_symbol(symbol) } else { declared }
            }
            None => error,
        };
        if computed == error
            && matches!(
                nodes.kind(parent),
                SyntaxKind::ExportAssignment | SyntaxKind::QualifiedName
            )
        {
            *saw_checker_error = true;
            return checker.intrinsics().any;
        }
        return computed;
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
    // The `IsTypeAny` precondition was originally carried across (convert only
    // when this port also computed `error`), on the theory that a non-error
    // answer of ours was information. Build 123 falsified that: as the checker
    // grew value resolution, the property name began resolving to a *sibling
    // value binding* (`duplicateIndetifiers2`'s parameter `name: string`) and
    // printing IT — a path upstream never takes, since `getTypeOfNode` falls
    // to `errorType` here **by construction** (the trace above). Dropping the
    // precondition and printing `any` unconditionally measured **+85 with zero
    // adverse** — the position's answer is decided by the trace, not by what
    // this port happens to compute.
    if let Some(parent) = nodes.parent(id)
        && let Some(Node::BindingElement(element)) = map.get(parent)
        && element.property_name.and_then(|name| name.node_id()) == Some(id)
    {
        return checker.intrinsics().any;
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
    //
    // **The precondition is gone with the expression-node gate below.** A
    // label is never an expression node (`IsInExpressionContext` has no
    // labeled/break/continue parent arm), so `getTypeOfNode` answers
    // `errorType` for it whatever this port's `check_expression` would find
    // — the `const outer = 1;` shape above included — and the writer's
    // `IsLabelName` guard prints `any`.
    if is_label_name(id, nodes, map) {
        *saw_checker_error = true;
        return checker.intrinsics().any;
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
    // and the property-access/qualified-name one was CLAIMED here to be
    // "already covered above by a rule reached along a different route" —
    // **that claim was false and SS183 measured it so**: the neighbouring
    // rule answers the access, and when the access ITSELF fails the name
    // still needs this guard (`Obj.fn = function(){}` records `>Obj.fk :
    // error` beside `>fn : any`). It is now ported at that site. The
    // sentence is left standing, corrected, because an unfalsified
    // redundancy claim is exactly the shape that hid it — checker-1's SS192
    // found the same failure mode in `getUnaryResultType` the same hour.
    // Original text: it is already covered above by a
    // rule reached along a different route. Porting them blind would present an
    // unmeasured net as a gain.
    //
    // The `IsTypeAny` precondition is upstream's and is load-bearing rather than
    // defensive: a **lowercase tag name that does resolve** — `const foo = () =>
    // {}` used as `<foo/>` — has a real type, never reaches the fast path at
    // all, and must keep printing it. The corpus discriminates this at 32 lines
    // printing `() => any` and 24 printing `typeof foo`.
    // SS179 (measured, reverted): the BINDING ELEMENT, LABEL NAME and
    // GLOBAL SCOPE AUGMENTATION guards were transcribed together and moved
    // **zero cases** (4,398 both ways) while churning lines both
    // directions. They are correct but case-inert in this corpus; the text
    // is in this commit's history. The remaining unported guards are
    // `hadErrorBaseline` and the meta-property one.
    //
    // **`hadErrorBaseline` is the largest guard left and it is REACHABLE.**
    // It is a whole-FILE flag: when a case has an `.errors.txt` baseline,
    // EVERY any-flagged type in it routes to the node builder and prints
    // `any` instead of the intrinsic `error`. The corpus already answers the
    // question — `CaseRef::has_varied_errors` / `expected_errors`
    // (`corpus.rs:163-175`) — but `render_case` receives a `TestCase`, which
    // carries only the case NAME and no link to the baselines directory, so
    // the flag has to be threaded in from the caller that owns the corpus.
    // That plumbing is the whole build; the guard itself is one condition.
    // Sizing: 155 of the 1,628 one-blocker files want `any` and get `error`,
    // and SS178's two guards took 6 of them.
    //
    // The meta-property guard WAS dead because the parser constructed no
    // `MetaProperty` node at all (checker-1's probe). §259: checker-1's §233
    // built it — `new.target` now parses as one — so the population exists and
    // the guard was RE-TESTED under conventions corollary 31, which says a
    // recorded `+0` is true of a tree rather than of an arm.
    //
    // Result: **still zero.** `+0` cases and `scorepair` reporting "no
    // transitions vs baseline", not one line in any direction.
    //
    // So the reason changes and the verdict does not. It is no longer dead for
    // want of nodes; it is inert because `new.target`'s type is not the error
    // type, and this guard only ever rewrites `error`. That moves it from the
    // DORMANT category (§254's, revived when its population appeared) to the
    // REDUNDANT one (§256's, waiting on a different arm), and the re-test
    // trigger changes with it: not "did something create my population" but
    // "did something start answering `error` here".
    //
    // Running tally for corollary 31's habit: **four re-tests, two verdict
    // changes** — §254 revived (+5), §255 inert -> 1:4 adverse, §256 unchanged,
    // §259 unchanged with a new reason.
    // SS182 (measured twice, both negative — do not re-derive): HOISTING
    // the SS178 guard above the declaration-name branch, so it can reach an
    // `import X = N` NAME, costs cases either way — 4,697 -> 4,685 computing
    // the type with `check_expression`, 4,697 -> 4,682 computing it the way
    // that branch does. The branch's answers for import/export names are
    // right more often than the guard's substitution is, so the guard stays
    // BELOW it and the `import X = N` population (aliasInaccessibleModule
    // and kin, ~28 one-blocker files) stays a gap. Whoever revisits it needs
    // the node builder's actual rendering of an unresolvable alias, not a
    // reordering.
    //
    // SS179/SS181: the BINDING ELEMENT, LABEL NAME and GLOBAL SCOPE
    // AUGMENTATION guards measured +0 cases TWICE — before SS180 and after,
    // against a population SS180 had shifted by 299 cases. Two independent
    // zeroes settle it: case-inert in this corpus.
    //
    // SS178: two more of the seven, TRANSCRIBED verbatim
    // (`isImportStatementName`/`isExportStatementName`,
    // type_symbol_baseline.go:458-479): an identifier that IS the name of an
    // import/export statement routes to the node builder, so an `any`-flagged
    // type prints `any` rather than the intrinsic `error`. Head case:
    // `export import X = N` over a non-exported namespace, where upstream
    // records `>X : any` beside `>N : error` — the SAME errorType, two
    // spellings, decided by this guard.
    if nodes.kind(id) == SyntaxKind::Identifier
        && let Some(parent) = nodes.parent(id)
        && match map.get(parent) {
            Some(Node::ImportSpecifier(specifier)) => {
                specifier.name.and_then(|n| n.node_id) == Some(id)
                    || specifier.property_name.and_then(|n| n.node_id()) == Some(id)
            }
            Some(Node::ImportClause(clause)) => clause.name.and_then(|n| n.node_id) == Some(id),
            Some(Node::ImportEqualsDeclaration(declaration)) => {
                declaration.name.and_then(|n| n.node_id) == Some(id)
            }
            Some(Node::ExportAssignment(assignment)) => {
                assignment.expression.and_then(|e| e.node_id()) == Some(id)
            }
            Some(Node::ExportSpecifier(specifier)) => {
                specifier.name.and_then(|n| n.node_id()) == Some(id)
                    || specifier.property_name.and_then(|n| n.node_id()) == Some(id)
            }
            _ => false,
        }
    {
        let computed = tsr_ast::Expression::try_from(node)
            .map_or(error, |expression| checker.check_expression(expression));
        if computed == error || computed == checker.intrinsics().any {
            *saw_checker_error |= computed == error;
            return checker.intrinsics().any;
        }
    }

    if nodes.kind(id) == SyntaxKind::Identifier
        && let Some(parent) = nodes.parent(id)
        && jsx_tag_name_of(parent, map) == Some(id)
        && let Some(Node::Identifier(name)) = map.get(id)
        && is_intrinsic_jsx_name(name.text)
    {
        let tag = tsr_ast::Expression::try_from(node)
            .map_or(error, |expression| checker.check_expression(expression));
        if tag == error || tag == checker.intrinsics().any {
            *saw_checker_error |= tag == error;
            return checker.intrinsics().any;
        }
    }

    // `getTypeOfNode` evaluates a node as a value only when
    // `ast.IsExpressionNode` holds (`checker.go:31955`); every other node
    // that no arm above claims falls off its end to `errorType`. Neither
    // part of a `JsxNamespacedName` (`<a:b>`) is one — `isInExpressionContext`
    // has no `JsxNamespacedName` parent arm — so both answer `errorType`
    // even when a `var a` is in scope (`jsxNamespacePrefixInName`).
    if predicates::is_expression_node(id, Tree { nodes, map })
        && let Ok(expression) = tsr_ast::Expression::try_from(node)
    {
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

/// The checker the producer renders through: the program as module host, its
/// JSDoc table, and its compiler options. **Probes that re-query a line must
/// build theirs here**, or they answer under different options from the line
/// they are explaining. `examples/depend.rs` built a bare
/// `Checker::with_module_host` and 108 of its "gap" lines typed in the probe's
/// checker at `7332f284` (`bd tsr-6.29`).
#[must_use]
pub fn configured_checker<'a>(
    program: &'a tsr_compiler::Program<'a>,
) -> tsr_checker::Checker<'a, 'a> {
    let mut checker = tsr_checker::Checker::with_module_host(
        program.binder(),
        program.nodes(),
        program.node_map(),
        Some(program),
    );
    for file in program.root_and_referenced_files() {
        checker.set_jsdoc(file.jsdoc().iter());
    }
    checker.apply_compiler_options(program.compiler_options());
    checker
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
    let mut checker = configured_checker(program);
    // `GetStrictOptionValue(strictNullChecks)` (`checker.go:919`) over the
    // case's directives: the explicit flag wins, `@strict` is the fallback.
    // **The default is `true`, measured off the baselines rather than
    // assumed**: `compiler/genericDefaults` carries no strict directive and
    // records `a : T | undefined` for `a?: T` (the strict-mode added
    // undefined), while `compiler/promiseType` says `@strict: false` outright
    // and renders `lib.es5.d.ts`'s `then` with its written
    // `| undefined | null` stripped. A `false` default was tried first and
    // lost 1,221 lines across 345 strict-by-default cases. Reduced to the two
    // directives rather than widening `CompilerOptions`, because this is the
    // only site the gradient constructs a checker through; the field moves
    // into `CompilerOptions` when a second consumer arrives. The union
    // constructor is what consumes it (`checker.go:25783`).
    // One derivation, upstream's, shared with `diagnostics_suite` (ADR-0042).
    //
    // # This replaced two *measured* defaults, and the measurements were stale
    //
    // What stood here read the raw `@directive` map and defaulted
    // `noImplicitAny` and `useUnknownInCatchVariables` to **false** when neither
    // they nor `@strict` were written — against upstream, where
    // `GetStrictOptionValue` answers `Strict != TSFalse` and an unset option is
    // therefore **true**. It was not a guess: both defaults were measured off
    // the `.types` baselines and `false` won at the time (the notes' §21 and §65
    // pairs), and the sibling suite's contradicting reading was recorded as a
    // known disagreement.
    //
    // **Re-measured at this commit, the faithful reading wins: 3,842 → 3,863
    // cases (+21) and 84.13% → 84.14% of lines.** The earlier number is not
    // disowned — it was true of the checker that produced it. What changed is
    // the checker underneath: with `catch (e)` typed `unknown` and an
    // un-annotated parameter implicitly erroring, the baselines those defaults
    // were compensating for now render correctly on their own.
    //
    // The lesson worth keeping is narrower than "measure less": a default tuned
    // against a partial implementation measures *the gap*, not the language, and
    // it has to be re-measured whenever the gap closes. An unfaithful default
    // that scores better is a marker for an unported rule somewhere else.
    //
    // Both the JSDoc table and the options are applied in `configured_checker`.

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
                let mut answer = type_at_location(&mut checker, bound, nodes, node_map, id);
                // SS180 `hadErrorBaseline` (`type_symbol_baseline.go:379`,
                // the FIRST condition of the guard chain): in a case that
                // produced diagnostics, the intrinsic-name fast path is
                // skipped entirely and every `any`-flagged type — the error
                // type included — renders through the node builder, which
                // prints `any`. Measured: 125 of the 152 files whose SINGLE
                // remaining blocker is `want any, got error` have an
                // `.errors.txt` baseline.
                if case.had_error_baseline && answer == "error" {
                    answer = "any".to_string();
                }
                // SS204 `!ast.IsPropertyAccessOrQualifiedName(node.Parent)`
                // (`type_symbol_baseline.go:383`, the THIRD condition). SS183
                // ported this guard for one position only — the NAME side of a
                // property access — because that was the position its witness
                // occupied. Upstream's test is on `node.Parent` alone: EITHER
                // side of EITHER construct. `moduleOuterQualification` is the
                // half SS183 missed, and it is the left side of a qualified
                // name:
                //
                //     namespace inner { export interface Beta extends outer.Beta {} }
                //     >outer : any        <- parent is a QualifiedName
                //
                // The guard belongs to the WRITER, not the checker, which is
                // why it sits here beside SS180 rather than inside
                // `type_at_location`: it does not change what any type IS, only
                // whether an any-flagged one takes the intrinsic fast path.
                // §248. The SAME guard as §178, applied at the WRITER instead
                // of as an early return in `type_at_location`.
                //
                // §182 measured HOISTING §178's guard above the
                // declaration-name branch and it lost cases twice
                // (4,697 → 4,685 and → 4,682), because that replaced the
                // branch's COMPUTATION, whose answers for import/export names
                // are right more often than the guard's substitution. This is
                // a different change and §182's negatives do not cover it:
                // here nothing is recomputed. A non-error answer is left
                // exactly as the branch produced it, and only the `error`
                // string is rewritten — which is where upstream's guard
                // applies, since `writeTypeOrSymbol` decides the SPELLING of
                // an any-flagged type after the checker has answered.
                //
                // Witnesses `moduleResolution_packageJson_yesAtPackageRoot`
                // and `moduleLocalImportNotIncorrectlyRedirected`, opened
                // independently: both are `import { x } from "…"` specifier
                // names over a module this port does not resolve.
                if answer == "error" && is_import_or_export_statement_name(id, nodes, node_map) {
                    answer = "any".to_string();
                }
                // §256, RE-TESTED AND STILL ZERO — the third of §179/§181's
                // batch, and the one that keeps corollary 31 honest. The LABEL
                // NAME guard (`!ast.IsLabelName(node)`,
                // `type_symbol_baseline.go:384`) applied at the writer, on the
                // tree where its two siblings changed verdict:
                //
                //     +0 cases, and `scorepair` reports "no transitions vs
                //     baseline" — not one line moved in any direction.
                //
                // So the batch is 2 of 3, not 3 of 3: §254's guard revived
                // (+5), §255's went from inert to 1:4 adverse, and this one is
                // genuinely unchanged. Recorded because a habit with a real
                // denominator is worth more than one with a perfect record —
                // corollary 31 names exactly this as its own falsifier, and
                // this is the datum that keeps it from reading as free.
                //
                // The reason it stays zero is visible in the checker-side arm
                // above (`is_label_name` in `type_at_location`): that arm
                // ALREADY converts an error-typed label to `any`, so by the
                // time the writer sees the answer there is nothing left to
                // rewrite. A guard whose work another arm has already done is
                // a different kind of zero from an empty population, and it
                // will stay zero until that arm changes.
                // §254 `!ast.IsGlobalScopeAugmentation(node.Parent)`
                // (`type_symbol_baseline.go:385`, the FIFTH condition), on
                // checker-1's diagnosis.
                //
                // Upstream's error type is `newIntrinsicType(TypeFlagsAny,
                // "error")` (`checker.go:979`) — intrinsic name `"error"`, not
                // `"any"` — so upstream's baselines CAN print `error`, and
                // which spelling appears depends only on which arm of
                // `writeTypeOrSymbol` ran. The fast path prints the intrinsic
                // name; the node-builder path prints `any` for anything
                // `TypeFlagsAny`. This guard routes a global augmentation away
                // from the fast path, so upstream prints `any` where this port
                // printed `error` — the SAME type, two spellings.
                //
                // That distinction is worth carrying beyond this arm:
                // *upstream printing `any` where we print `error`* is
                // sometimes ADR-0038's ceiling and sometimes a missing writer
                // guard, and **the discriminator is which arm upstream took**,
                // not what it printed.
                //
                // §179/§181 MEASURED THIS GUARD AT +0 TWICE and recorded it as
                // case-inert. That was true then and is not a contradiction
                // now: §253 fixed `IsAmbientModule`'s missing disjunct, which
                // is what makes a global augmentation's name reach this walk at
                // all. The guard was inert because its population was empty —
                // an arm can be correct and unmeasurable until an unrelated
                // fix creates the nodes it acts on. Neither zero was wrong;
                // both were about a different tree.
                if answer == "error"
                    && let Some(parent) = nodes.parent(id)
                    && matches!(
                        node_map.get(parent),
                        Some(Node::ModuleDeclaration(module))
                            if module.keyword.kind == SyntaxKind::GlobalKeyword
                    )
                {
                    answer = "any".to_string();
                }
                // §255, REVERTED, and the re-test was worth running. §254
                // established that a recorded +0 is true of a TREE rather than
                // of an ARM, so §179/§181's other two zeroes were re-measured
                // rather than assumed still zero. The BINDING ELEMENT guard
                // (`!ast.IsBindingElement(node.Parent)`,
                // `type_symbol_baseline.go:382`) applied whole:
                //
                //     +2 cases
                //     GAP->RIGHT   112
                //     GAP->WRONG   436   <-- 1 : 4 against
                //
                // So it is no longer case-inert — it is ACTIVELY HARMFUL at
                // line level, and the case tally says +2. That is corollary
                // 24's sharpest instance in this file: a two-case gain hiding
                // 436 gaps turned into confident wrong answers.
                //
                // The reason is already documented at the binding-element note
                // further down: upstream's guard covers EVERY child of a
                // binding element, and those positions were measured NOT to
                // behave alike — the bound name, the property name, the
                // initialiser and the dots each want something different. The
                // guard whole is wrong here; the property-name subset (already
                // ported) is the part that holds.
                //
                // Re-tested 2026-08-12. The record changes from "case-inert"
                // to "measured 1:4 adverse", which is a much stronger refusal
                // than the zero it replaces.
                if answer == "error"
                    && let Some(parent) = nodes.parent(id)
                    && matches!(
                        nodes.kind(parent),
                        SyntaxKind::PropertyAccessExpression | SyntaxKind::QualifiedName
                    )
                {
                    answer = "any".to_string();
                }
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
///
/// **Public because a second consumer arrived**: the `diagnostics` suite and its
/// probes need a case compiled the way the `checker_types` gradient compiles it,
/// and `docs/conventions.md`'s *"a probe that re-implements the harness is
/// measuring a different compiler"* makes copying this function a defect rather
/// than a convenience. The `/.lib` mount and the `@libFiles` roots below are
/// exactly the parts a copy silently loses.
#[must_use]
pub fn program_for_case<'a>(
    arena: &'a tsr_core::Arena,
    case: &crate::TestCase,
) -> tsr_compiler::Program<'a> {
    program_and_config_for_case(arena, case).0
}

/// [`program_for_case`], with the case's parsed `tsconfig.json` when it has
/// one: the `diagnostics` suite reports that parse's errors
/// (`GetConfigFileParsingDiagnostics`), so it must be the same parse the
/// program was built from.
#[must_use]
pub fn program_and_config_for_case<'a>(
    arena: &'a tsr_core::Arena,
    case: &crate::TestCase,
) -> (tsr_compiler::Program<'a>, Option<tsr_tsoptions::ParsedCommandLine>) {
    // **`@currentDirectory` is a directive, not decoration** (50 corpus cases).
    // The *default* stays `/` — `trace_case` defaults to `/.src` because the
    // resolution traces are baselined against those paths and the diagnostics
    // baselines against bare names, so the two suites' conventions genuinely
    // differ and aligning them would break the one that is right. Only the
    // directive is honoured, over this producer's own default, which is what
    // upstream's harness does. §539.
    let current_directory = case.current_directory.as_deref().map_or_else(
        || CURRENT_DIRECTORY.to_string(),
        |dir| tsr_path::get_normalized_absolute_path(dir, CURRENT_DIRECTORY),
    );
    let current_directory = current_directory.as_str();
    // `@useCaseSensitiveFileNames` (9 corpus cases), read exactly as
    // `trace_case` reads it: absent or anything but `false` means sensitive.
    let case_sensitive = case
        .options
        .get("usecasesensitivefilenames")
        .is_none_or(|value| !value.eq_ignore_ascii_case("false"));
    // The case's `tsconfig.json`, parsed once: §533 needs its options and §535
    // needs its file list. `trace_case::compilation` reads both; this producer
    // read neither.
    let parsed_config = case
        .files
        .iter()
        .find(|unit| crate::trace_case::config_name_from_file_name(&unit.name).is_some())
        .map(|config| {
            let config_file_name =
                tsr_path::get_normalized_absolute_path(&config.name, current_directory);
            let config_fs = crate::trace_case::build_file_system(
                &case.files,
                case,
                current_directory,
                case_sensitive,
            );
            tsr_tsoptions::parse_config_file(
                &config_file_name,
                &config.content,
                tsr_path::get_directory_path(&config_file_name),
                &config_fs,
            )
        });

    let mut files: Vec<(String, String)> = bundled_libs().to_vec();
    let mut roots = Vec::new();
    for unit in &case.files {
        let name = tsr_path::get_normalized_absolute_path(&unit.name, current_directory);
        files.push((name.clone(), unit.content.clone()));
    }
    // **Roots are chosen, not assumed.** With a config they are its file list
    // (§535); without one they are `harnessutil`'s heuristic on the last unit —
    // a `require(` or a `/// <reference path` there means it pulls the rest in
    // and is the only root (§537). `.json` and `.tsbuildinfo` are never roots.
    // The producer made every unit a root under both conditions.
    roots.extend(match &parsed_config {
        Some(parsed) => case
            .files
            .iter()
            .map(|unit| tsr_path::get_normalized_absolute_path(&unit.name, current_directory))
            .filter(|name| parsed.file_names.contains(name))
            .collect::<Vec<_>>(),
        None => {
            crate::trace_case::root_files_without_a_config(case, &case.files, current_directory)
        }
    });

    // The `/.lib` test-library folder — `harnessutil.go:39` and the copy-in
    // rule at `:141`: the folder is present exactly when some input file
    // mentions it (`/// <reference path="/.lib/react.d.ts" />`), or when the
    // case names files from it with `@libFiles`, which upstream additionally
    // makes program **roots**. Without this mapping every such reference
    // silently resolved to nothing, and `declare module "react"` — the module
    // the whole tsx corpus imports — was never in any program.
    let mentions_lib = case.files.iter().any(|unit| unit.content.contains("/.lib/"));
    let lib_files = case.options.get("libfiles");
    if mentions_lib || lib_files.is_some() {
        files.extend(test_lib_files().iter().cloned());
    }
    if let Some(list) = lib_files {
        for name in list.split(',').map(str::trim).filter(|name| !name.is_empty()) {
            roots.push(format!("/.lib/{name}"));
        }
    }

    // **A case's `tsconfig.json` is an input, not decoration.** This producer
    // built every program from `CompilerOptions::default()` plus `@`-directives
    // and left the config unit inert, so any option a case sets only in its
    // tsconfig — and `config_file_path` itself, which upstream resolves
    // `@typescript/lib-*` relative to — was simply absent.
    // `trace_case::compilation` has had this branch all along.
    // `docs/architecture/checker-notes-diag2.md` §533.
    let base = parsed_config
        .as_ref()
        .map_or_else(tsr_core::CompilerOptions::default, |config| config.compiler_options.clone());
    let options = crate::trace_case::apply_test_directives(base, case, current_directory);
    // §118 (`checker-notes-narrow.md`): the case's `@symlink` links, normalized
    // exactly as `trace_case::build_file_system` normalizes them. The VFS and
    // resolver have followed links since the module_resolution suite landed;
    // this harness was the one road handing them an empty link table, which
    // made every symlink-reached module "unfindable" at the checker.
    let symlinks = case
        .symlinks
        .iter()
        .map(|(link, target)| {
            (
                tsr_path::get_normalized_absolute_path(link, current_directory),
                tsr_path::get_normalized_absolute_path(target, current_directory),
            )
        })
        .collect::<Vec<_>>();
    let host = CaseHost { fs: tsr_vfs::InMemoryFileSystem::new(files, symlinks, case_sensitive) };
    let program = tsr_compiler::Program::from_root_files(
        arena,
        &host,
        tsr_compiler::LoadOptions {
            compiler_options: options,
            root_file_names: roots,
            default_library_path: LIB_DIRECTORY.to_string(),
        },
    );
    (program, parsed_config)
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
    if let Some(property) = checker.get_property_of_type(receiver_type, name.text) {
        // §829: this reading was 649 lines before §827 re-rooted the genuinely
        // downstream half and 339 after, and a single string cannot price the
        // remainder. The column worth reading is whether the property has a type
        // AT ALL: `get_type_of_symbol` on it answering `error` means the lookup is
        // still downstream of the property's own declaration, while a real type
        // there means the property types fine and only its projection through
        // THIS receiver fails — which is `bd tsr-4qx`'s substitution seam and a
        // different piece of work.
        //
        // The receiver's shape comes with it on the same axis. It is read off the
        // PRINTED text rather than the flags, because the producer has no flags
        // accessor and adding public checker surface for a probe is worse than a
        // coarse label that says it is coarse.
        let own = checker.get_type_of_symbol(property);
        let downstream = own == error;
        let printed = checker.type_to_string(receiver_type);
        let shape = if printed.contains('&') {
            "intersection"
        } else if printed.contains('|') {
            "union"
        } else if printed.starts_with('{') {
            "anonymous"
        } else if printed.contains('<') {
            "generic reference"
        } else if printed.len() <= 2 {
            "type parameter (probably)"
        } else {
            "named"
        };
        return format!(
            "the property has no type [{}; receiver {shape}]",
            if downstream {
                "DOWNSTREAM: the property itself gaps"
            } else {
                "the property TYPES; the projection fails"
            }
        );
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

/// Is `id` inside a `typeof` type query?
///
/// The binder's own `is_part_of_type_query` (`crates/tsr-binder/src/binder.rs`),
/// which is `ast.IsPartOfTypeQuery`: walk up while the node is still part of an
/// entity name, and answer whether what stops the walk is the query.
fn is_part_of_type_query(id: NodeId, nodes: &NodeTable) -> bool {
    let mut current = id;
    loop {
        let kind = nodes.kind(current);
        if !matches!(kind, SyntaxKind::QualifiedName | SyntaxKind::Identifier) {
            return kind == SyntaxKind::TypeQuery;
        }
        match nodes.parent(current) {
            Some(parent) => current = parent,
            None => return false,
        }
    }
}

/// The `QualifiedName` whose value type `id` should be reported as: `id` itself
/// when it is one, or its parent when `id` is that parent's right-hand name.
///
/// Both lines exist in the oracle and both want the property's type:
///
/// ```text
/// type FooOK = typeof properties.foo;
/// >properties.foo : { aaa: string; bbb: string; }
/// >foo : { aaa: string; bbb: string; }
/// ```
fn qualified_name_to_check<'a>(
    id: NodeId,
    nodes: &NodeTable,
    map: &NodeMap<'a>,
) -> Option<&'a tsr_ast::QualifiedName<'a>> {
    if let Some(Node::QualifiedName(qualified)) = map.get(id) {
        return Some(qualified);
    }
    if nodes.kind(id) != SyntaxKind::Identifier {
        return None;
    }
    let parent = nodes.parent(id)?;
    let Some(Node::QualifiedName(qualified)) = map.get(parent) else { return None };
    (qualified.right.and_then(|right| right.node_id) == Some(id)).then_some(qualified)
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
            &arena,
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
            &arena,
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
    fn a_label_shadowing_a_value_still_prints_any() {
        // A label shares no namespace with a value upstream. This checker
        // resolves the identifier to the variable and answers `1`, but
        // `getTypeOfNode` never asks it: a label is not an expression node
        // (`IsInExpressionContext` has no labeled/jump parent arm), so the
        // label answers `errorType` and the writer prints `any`. The
        // declaration and the expression statement keep their `1`.
        let out = typed("const outer = 1;\nouter: while (true) { outer; }");
        let labels: Vec<_> =
            out.iter().filter(|(text, _)| text == "outer").map(|(_, ty)| ty.as_str()).collect();
        assert_eq!(labels, ["1", "any", "1"], "in {out:?}");
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
    fn a_binding_element_property_name_shadowing_a_value_still_prints_any() {
        // This test used to pin the OPPOSITE: that shadowing kept the type we
        // computed (`1`), guarding the `IsTypeAny` precondition. Build 123
        // reversed the arm on measurement (+85/0 unconditional): upstream's
        // `getTypeOfNode` falls to `errorType` for the property-name `a`
        // whatever else `a` means in the file, so the position's answer is
        // `any` by construction — resolving the identifier to the outer
        // `const a` was this port's own divergence, not upstream's behaviour.
        // The declaration's own `a` (first line) still answers `1`.
        let out = typed("const a = 1;\ndeclare const x: any;\nconst { a: b } = x;");
        let names: Vec<_> =
            out.iter().filter(|(text, _)| text == "a").map(|(_, ty)| ty.as_str()).collect();
        assert_eq!(names, ["1", "any"], "in {out:?}");
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
    fn new_member_chains_keep_native_preorder_and_callee_source() {
        // Pinned ForEachChild/SkipTrivia controls, not reordered baseline text.
        // The index belongs inside the constructor callee; an index following
        // its argument list instead belongs to the constructed result.
        assert_eq!(
            texts("new d[1].f(2, ...args);"),
            ["new d[1].f(2, ...args)", "d[1].f", "d[1]", "d", "1", "f", "2", "...args", "args"],
        );
        assert_eq!(
            texts("new ns.C[key].D<T>(3);"),
            [
                "new ns.C[key].D<T>(3)",
                "ns.C[key].D",
                "ns.C[key]",
                "ns.C",
                "ns",
                "C",
                "key",
                "D",
                "3"
            ],
        );
        assert_eq!(
            texts("new C()[1].f(4);"),
            ["new C()[1].f(4)", "new C()[1].f", "new C()[1]", "new C()", "C", "1", "f", "4"],
        );
        assert_eq!(texts("new C[0]!();"), ["new C[0]!()", "C[0]!", "C[0]", "C", "0"]);
        assert_eq!(
            texts("new new C[1](5);"),
            ["new new C[1](5)", "new C[1](5)", "C[1]", "C", "1", "5"],
        );
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
    fn a_class_alias_base_uses_its_instance_but_other_alias_positions_use_its_value() {
        // Pinned native 5b1047d: GetTypeAtLocation(E) is typeof E, but the
        // baseline consumer asks the parent heritage entry for its base E.
        // Alias declaration order must not change either semantic question.
        for (body, expected) in [
            (
                "class B extends E {} import E = A.C; const value = E;",
                vec!["E", "typeof E", "typeof E"],
            ),
            (
                "import E = A.C; class B extends E {} const value = E;",
                vec!["typeof E", "E", "typeof E"],
            ),
        ] {
            let source = format!(
                "namespace Host {{ namespace A {{ export class C {{ tag = 7; }} }} {body} }}"
            );
            let pairs = typed(&source);
            let aliases: Vec<_> =
                pairs.iter().filter(|(text, _)| text == "E").map(|(_, ty)| ty.as_str()).collect();
            assert_eq!(aliases, expected, "in {pairs:?}");
        }
        // The original ambient use-before-declaration witness, whose class
        // is implicitly exported from its ambient namespace.
        let pairs = typed(
            "declare module 'test' { namespace A { class C {} } class B extends E {} import E = A.C; }",
        );
        let aliases: Vec<_> =
            pairs.iter().filter(|(text, _)| text == "E").map(|(_, ty)| ty.as_str()).collect();
        assert_eq!(aliases, ["E", "typeof E"], "in {pairs:?}");
    }

    #[test]
    fn a_type_only_or_cyclic_alias_base_keeps_the_expression_fallback() {
        // Opposite meaning: resolving a type-only interface alias must not
        // turn an invalid value base into the interface's declared type.
        // Native errorType prints any; the isolated helper retains our error
        // gap instead of applying the corpus writer's had-errors handling.
        // Cycles and missing required arguments also have no native instance
        // base, so retain typeof E.
        for (declaration, expected) in [
            ("export interface I { tag: string; }", "error"),
            ("export class I extends B {}", "typeof E"),
            ("export class I<T> { tag!: T; }", "typeof E"),
        ] {
            let source = format!(
                "namespace Host {{ namespace A {{ {declaration} }} import E = A.I; class B extends E {{}} const value = E; }}"
            );
            let pairs = typed(&source);
            let aliases: Vec<_> =
                pairs.iter().filter(|(text, _)| text == "E").map(|(_, ty)| ty.as_str()).collect();
            assert_eq!(aliases, [expected; 3], "in {pairs:?}");
        }
        let pairs = typed(
            "namespace Host { namespace A { export class C {} } import E = A.C; function f(E: number) { class B extends E {} return E; } }",
        );
        let aliases: Vec<_> =
            pairs.iter().filter(|(text, _)| text == "E").map(|(_, ty)| ty.as_str()).collect();
        assert_eq!(aliases, ["typeof E", "number", "number", "number"], "in {pairs:?}");
    }

    #[test]
    fn an_import_equals_alias_inside_a_base_cycle_preserves_value_types() {
        let pairs = typed(
            "namespace Host { namespace A { export class C extends F {} } import E = A.C; namespace Other { export class D extends B {} } import F = Other.D; class B extends E {} const value = E; }",
        );
        for (name, expected) in [("E", vec!["typeof E"; 3]), ("F", vec!["typeof F"; 2])] {
            let aliases: Vec<_> =
                pairs.iter().filter(|(text, _)| text == name).map(|(_, ty)| ty.as_str()).collect();
            assert_eq!(aliases, expected, "in {pairs:?}");
        }
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
                // §145 (`checker-notes-narrow.md`): a qualified ImportEquals
                // whose leaf is a nameless-text VALUE (const, function,
                // property) reads its target's type; this was "error" while
                // the qualified arm was wholly gapped.
                ("x".to_string(), "1".to_string()),
                ("M".to_string(), "typeof M".to_string()), // the entity name
                // §243. This read `error` and the comment beside it said so:
                // the LEAF of the entity name, which no arm answered. It is
                // the same declared-then-value order as the root — `a` is a
                // const, whose declared type IS the error type, so the answer
                // falls through to its value type. The pin was the gap.
                ("a".to_string(), "1".to_string()),
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
                // §145 (`checker-notes-narrow.md`): the enum-MEMBER leaf is
                // ENUM_MEMBER-flagged, not ENUM — it takes the nameless-leaf
                // arm and reads its own member type, whose text spells the
                // TARGET chain. "error" here was the gap, not the answer.
                ("q".to_string(), "E.A".to_string()),
                ("E".to_string(), "E".to_string()), // the entity name: DECLARED
                // §243 answers this too, and it is the half that pins the
                // ORDER on the leaf rather than on the root: an enum member's
                // DECLARED type is `E.A` and is not the error type, so the
                // fallback never runs. Were the leaf collapsed to "answer the
                // value type", this would still read `E.A` — which is why the
                // const case above is the one that discriminates.
                ("A".to_string(), "E.A".to_string()),
            ],
        );

        // The other direction: a left that resolves to no namespace keeps the
        // `any` the general qualified-name rule gives it. This is the guard that
        // stops the rule turning 34 right answers into wrong ones.
        // **`y` flipped by §144** (the thirty-eighth stand-in): the alias to
        // an unresolvable-root entity reads the same error-any its pieces do.
        // `thing` misses too (`getSymbolOfPartOfRightHandSideOfImportEquals`
        // finds no `Missing`), so `getTypeOfNode` answers `errorType`, and its
        // parent is a qualified name, so the writer's
        // `IsPropertyAccessOrQualifiedName` guard prints `any`, not `error`.
        assert_eq!(
            typed("import y = Missing.thing;"),
            vec![
                ("y".to_string(), "any".to_string()),
                ("Missing".to_string(), "any".to_string()),
                ("thing".to_string(), "any".to_string()),
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
    fn source_text_preserves_lone_cr_and_unicode_separators_inside_nodes() {
        assert_eq!(texts("`a\rb\r\nc\nd\u{2028}e\u{2029}f`;"), ["`a\rbcd\u{2028}e\u{2029}f`"]);
        // Not `str::lines`: that would also discard the lone CR, and Unicode
        // separators are whitespace only at the node's leading-trivia boundary.
        let source = "\u{feff}\u{a0} // c\r\u{2028}/* λ */ \u{200b}x";
        let span = tsr_core::Span::new(0, u32::try_from(source.len()).unwrap());
        assert_eq!(source_text(source, span), "x");
        for delimiter in ['\r', '\n', '\u{2028}', '\u{2029}'] {
            let source = format!("// λ{delimiter}x");
            assert_eq!(skip_trivia(&source, 0), source.len() - 1);
        }
        assert_eq!(skip_trivia("#! /usr/bin/env node\r\nx", 0), 22);
        assert_eq!(skip_trivia("x#! /usr/bin/env node\nx", 1), 1);
        assert_eq!(source_text(" /* λ */ ", tsr_core::Span::at(0)), "");
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
