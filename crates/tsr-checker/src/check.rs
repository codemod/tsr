//! The **check traversal** — the second road into the checker, the one that
//! reports.
//!
//! # Why this module exists, and why it is not a hook on the query road
//!
//! [ADR-0040](../../../docs/adr/0040-diagnostics-come-from-a-check-traversal-and-assignability-gets-a-reporting-twin.md)
//! decisions (1) and (2), which are the two the ADR's own falsifiers left
//! standing. Upstream has two entry points into one engine:
//!
//! | entry | purpose | here |
//! |---|---|---|
//! | `checkSourceFile` → `checkSourceElement` (`checker.go:2196`, `:2241`) | reports diagnostics | **this module** |
//! | `getTypeOfNode` (`checker.go:31927`) | answers "what type is this node" | `types_producer::type_at_location` |
//!
//! Until this file existed the port had built the query road and none of the
//! traversal road, which is the whole reason `diagnostics` read **80/5,488
//! (1.46%)** while `checker_types` went 36% → 73.65%. A diagnostic is an eager
//! **side effect of a walk**, not a return value a consumer can ask for, so no
//! amount of work on the query road can produce one.
//!
//! # What it walks today, and why so little
//!
//! Only the declarations that carry a **module specifier**, plus the module
//! bodies that can contain them. That is deliberately far short of upstream's
//! `checkSourceElement`, and the reason is `docs/conventions.md`'s rule that a
//! diagnostic emitter is the consumer that *acts on the negative*: every node
//! kind this walk learns to visit is a new opportunity to report something
//! upstream does not, and under the suite's **exact multiset equality** an
//! invented diagnostic fails a case exactly as a missing one does — except that
//! it can also break a case that passes today.
//!
//! So the traversal grows one rule at a time, each sized against
//! `crates/tsr-conformance/examples/diaggap.rs` before it is written and scored
//! against a registered bar afterwards. See
//! `docs/architecture/checker-notes-diag2.md`.
//!
//! # Diagnostics carry the file they are in
//!
//! Upstream's `ast.DiagnosticsCollection` keys by file because a `Diagnostic`
//! there holds its `*ast.SourceFile`. [`tsr_diagnostics::Diagnostic`] holds a
//! [`tsr_core::Span`] and nothing else, and under
//! [ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md) one
//! `NodeTable` spans every file of a program — so a bare span is ambiguous
//! across files and the collection stores `(source file, diagnostic)` pairs.
//! Getting this wrong would put every cross-file diagnostic on the wrong line of
//! the wrong unit, which is the failure mode that looks like a checker bug for a
//! week.

use tsr_ast::{ClassElement, ModifierLike, Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

/// What the caller knows about a file that the tree does not say.
///
/// Both fields stand in for facts upstream's parser records and this port's does
/// not — `NodeFlagsAmbient` for the first, and for the second the simple fact
/// that upstream is comparing its *own* recovery against baselines produced by
/// it. Each is documented at its reader; they travel together because a caller
/// that knows one knows the other.
#[derive(Debug, Clone, Copy)]
pub struct FileContext {
    /// Upstream's `node.Flags & ast.NodeFlagsAmbient` for the file: is this a
    /// declaration file?
    pub ambient: bool,
    /// Did the parser report anything in this file?
    pub has_parse_errors: bool,
}

/// The walk's stack budget, matching `tsr-binder`'s `MAX_DEPTH`.
///
/// Not a correctness bound: a file deeper than this loses diagnostics from the
/// deep part, which is a missing diagnostic and therefore a *failed* case rather
/// than a wrong one. ADR-0029's budget is what makes a fixed number necessary
/// here at all — Go grows a goroutine's stack on demand and upstream needs no
/// equivalent.
const MAX_CHECK_DEPTH: u32 = 1_000;

/// Children held without allocating, before falling back to a `Vec`.
///
/// The walk cannot borrow `self.node_map` across a `&mut self` recursion, so the
/// child ids are collected first. Almost every node has few children; the ones
/// that do not are statement lists and argument lists, and paying an allocation
/// for those alone keeps the common node allocation-free.
const INLINE_CHILDREN: usize = 8;

impl Checker<'_, '_> {
    /// Report every diagnostic this port can produce for one source file.
    ///
    /// `checkSourceFile` (`checker.go:2196`) → `checkSourceElements` →
    /// `checkSourceElement` (`checker.go:2241`). Upstream's version also runs
    /// grammar checks and deferred nodes; neither is ported.
    ///
    /// Idempotent by construction is *not* claimed: calling this twice on one
    /// file appends its diagnostics twice, exactly as upstream's would without
    /// its `checkSourceFileWorker` memo. Callers run it once per file.
    /// `in_ambient_context` is upstream's `node.Flags & ast.NodeFlagsAmbient`,
    /// **passed in because this port's parser never sets that flag** — it is
    /// declared in `tsr_ast::NodeFlags` and written nowhere
    /// (`grep -rn AMBIENT crates/tsr-parser/src` is empty). Upstream's parser
    /// sets it as a context flag on every node of a declaration file and inside
    /// every `declare`d declaration, and the checker reads it in ~40 places.
    ///
    /// Here the caller supplies the *file-level* half — is this a `.d.ts` — and
    /// [`Checker::check_source_element`] carries the `declare`-modifier half
    /// down the walk. That is a faithful reproduction of the effect and an
    /// unfaithful reproduction of the mechanism; when the parser learns to set
    /// the flag this parameter goes away and every reader gets it for free.
    /// `bd tsr-o9tl` carries it.
    ///
    /// It was not optional: reading the unset flag instead reported TS2564 on
    /// **every property of every `declare class` in the corpus**, ~25 of the 86
    /// wrong lines that measurement produced.
    pub fn check_source_file(&mut self, file: NodeId, context: FileContext) {
        self.file_has_parse_errors = context.has_parse_errors;
        self.check_node(file, context.ambient, 0);
    }

    /// One node: its own rules, then its children.
    ///
    /// # Why a generic child walk rather than a typed `checkSourceElement`
    ///
    /// Upstream's `checkSourceElement` (`checker.go:2241`) is a 120-arm switch
    /// that dispatches each kind to its own `checkXxx`, and each `checkXxx`
    /// recurses into exactly the children that kind checks. Reproducing that
    /// shape means writing 120 arms before the first rule beyond declarations
    /// can fire.
    ///
    /// This walks every registered child via
    /// [`tsr_ast::for_each_child_id`] and lets each rule decide, at the node it
    /// cares about, whether it applies. The two produce the same *set of visited
    /// nodes* for the rules ported so far; they differ in that upstream's order
    /// is a checking order with deferred work, and this one is document order
    /// with none. Nothing ported yet depends on either.
    ///
    /// The consequence to accept, and it is the one that matters: **a rule here
    /// sees nodes upstream's corresponding `checkXxx` would never be handed**,
    /// so every rule must carry its own position test rather than relying on the
    /// walk to have filtered for it. [`Checker::is_value_reference`] is that test
    /// for identifiers and it is written as an *allow*-list precisely because a
    /// missing arm then costs silence rather than a false positive.
    ///
    /// `depth` is bounded for the same reason `tsr-binder`'s walk is
    /// ([ADR-0029](../../../docs/adr/0029-stack-discipline-is-guards-plus-a-budget.md)):
    /// the corpus contains files written to break compilers, and
    /// `compiler/binderBinaryExpressionStress` is 4,971 operands of one
    /// left-leaning chain.
    fn check_node(&mut self, node: NodeId, ambient: bool, depth: u32) {
        if depth > MAX_CHECK_DEPTH {
            return;
        }
        let Some(typed) = self.node_map.get(node) else { return };
        let ambient = match typed {
            Node::ImportDeclaration(declaration) => {
                // `import "x"` with no clause is a **side-effect import**, and
                // upstream gives it its own message — `checkImportDeclaration`'s
                // `else if` branch at `checker.go:5321`, guarded by
                // `NoUncheckedSideEffectImports.IsTrueOrUnknown()`, which is
                // true when the option is unset. Same site, same resolution,
                // a different code.
                let side_effect = declaration.import_clause.is_none();
                // `checker.go:5321`'s guard: with the option explicitly off,
                // upstream does not resolve the specifier at all, so there is no
                // diagnostic of any code to report there.
                if !side_effect || self.no_unchecked_side_effect_imports {
                    self.check_module_specifier(
                        node,
                        declaration.module_specifier.and_then(|s| s.node_id()),
                        side_effect,
                    );
                }
                ambient
            }
            Node::ExportDeclaration(declaration) => {
                self.check_module_specifier(
                    node,
                    declaration.module_specifier.and_then(|s| s.node_id()),
                    false,
                );
                ambient
            }
            Node::ImportEqualsDeclaration(declaration) => {
                // Only the `require("x")` spelling names a module; `import a = b.c`
                // is an entity-name alias and resolves through the scope.
                if let Some(tsr_ast::ModuleReference::ExternalModuleReference(reference)) =
                    declaration.module_reference
                {
                    self.check_module_specifier(
                        node,
                        reference.expression.and_then(|e| e.node_id()),
                        false,
                    );
                }
                ambient
            }
            Node::ClassDeclaration(declaration) => {
                // `declare class C { x: number }` puts every member in an
                // ambient context, which is where upstream's flag would already
                // be set on the members themselves.
                let ambient =
                    ambient || has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword);
                self.check_property_initialization(declaration.members, ambient);
                ambient
            }
            Node::ClassExpression(declaration) => {
                let ambient =
                    ambient || has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword);
                self.check_property_initialization(declaration.members, ambient);
                ambient
            }
            // `declare module "m" { … }` and `declare namespace N { … }` are
            // ambient contexts, and so is an *ambient* module's body whether or
            // not the keyword is repeated inside it.
            Node::ModuleDeclaration(declaration) => {
                ambient
                    || has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword)
                    || self.is_ambient_module_node(node)
            }
            Node::VariableStatement(statement) => {
                ambient || has_modifier(statement.modifiers, SyntaxKind::DeclareKeyword)
            }
            Node::FunctionDeclaration(declaration) => {
                ambient || has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword)
            }
            Node::EnumDeclaration(declaration) => {
                ambient || has_modifier(declaration.modifiers, SyntaxKind::DeclareKeyword)
            }
            Node::Identifier(identifier) => {
                self.check_value_identifier(node, identifier.text);
                self.check_used_before_assigned(node, identifier.text);
                ambient
            }
            _ => ambient,
        };
        let mut children = [const { None }; INLINE_CHILDREN];
        let mut count = 0usize;
        let mut overflow: Vec<NodeId> = Vec::new();
        tsr_ast::for_each_child_id(typed, |child| {
            if count < INLINE_CHILDREN {
                children[count] = Some(child);
            } else {
                overflow.push(child);
            }
            count += 1;
        });
        for child in children.into_iter().flatten() {
            self.check_node(child, ambient, depth + 1);
        }
        for child in overflow {
            self.check_node(child, ambient, depth + 1);
        }
    }

    /// `checkExternalImportOrExportDeclaration` (`checker.go:5332`) — the gate
    /// that runs **before** any resolution and can stop it.
    ///
    /// Only the arm that matters for a specifier that will not resolve is
    /// ported: an import or export declaration whose parent is neither a
    /// `SourceFile` nor a module block belonging to an **ambient** module is a
    /// grammar error (TS1147 `Import_declarations_in_a_namespace_cannot_reference_a_module`,
    /// or TS1148 for the export spelling) and upstream `return`s without
    /// resolving anything.
    ///
    /// This port emits neither 1147 nor 1148 — they are grammar checks with
    /// their own sizing — so the arm is a **refusal**: silence where upstream
    /// says something else. Reporting TS2307 inside a namespace was 14 of the 60
    /// wrong lines the first counterfactual measured, all in
    /// `compiler/privacyImportParseErrors` and its sibling.
    fn external_import_is_positioned_for_resolution(&self, declaration: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(declaration) else { return false };
        if self.nodes.kind(parent) == SyntaxKind::SourceFile {
            return true;
        }
        // `inAmbientExternalModule` (`checker.go:5343`): a module *block* whose
        // own parent is an ambient module declaration.
        self.nodes.kind(parent) == SyntaxKind::ModuleBlock
            && self.nodes.parent(parent).is_some_and(|owner| self.is_ambient_module_node(owner))
    }

    /// `ast.IsAmbientModule` (`ast/utilities.go:1652`): a module declaration
    /// named by a **string literal**, or a `declare global` augmentation
    /// (`IsGlobalScopeAugmentation`, `:1690`).
    fn is_ambient_module_node(&self, node: NodeId) -> bool {
        let Some(Node::ModuleDeclaration(declaration)) = self.node_map.get(node) else {
            return false;
        };
        matches!(declaration.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
            || declaration.keyword.kind == SyntaxKind::GlobalKeyword
    }

    /// TS2307 — `Cannot find module '{0}' or its corresponding type declarations.`
    ///
    /// Upstream's site is the **fallthrough** of `resolveExternalModule`
    /// (`checker.go:15149`), reached from `resolveExternalModuleName`
    /// (`checker.go:15100`) after `getCannotResolveModuleNameErrorForSpecificModule`
    /// has declined to substitute a more specific message. The error node is the
    /// specifier literal itself, so the reported column is the **opening quote**
    /// — `badExternalModuleReference.errors.txt` records `(1,21)` for
    /// `import a1 = require("garbage")`, and 21 is the `"`.
    ///
    /// # This emits on a strict subset of upstream's condition, on purpose
    ///
    /// Upstream's function is 190 lines carrying **fourteen** distinct messages,
    /// and TS2307 is what is left when every one of them declines. Each of the
    /// guards below is a situation where upstream reports a *different code*, so
    /// emitting TS2307 there would be a false positive — and under the suite's
    /// exact-multiset rule a false positive costs a case exactly as a missing
    /// diagnostic does, while additionally being able to break a case that
    /// passes today. `docs/conventions.md`: the emitter is the consumer that
    /// acts on the negative, so its bound is part of the design and not an
    /// implementation shortcut.
    ///
    /// | declined here | upstream's code there |
    /// |---|---|
    /// | the specifier resolves to a file | TS2306 `File_0_is_not_a_module`, TS7016, the `node16` mode family — never 2307 |
    /// | a `declare module "x"` names it | resolved; no diagnostic |
    /// | a **pattern** ambient module could match (`declare module "foo/*"`) | resolved by `FindBestPatternMatch` (`checker.go:15364`), which this port does not implement — so a match is *possible* and silence is the only sound answer |
    /// | a Node core module name (`fs`, `path`, …) | TS2580/TS2591, substituted by `getCannotResolveModuleNameErrorForSpecificModule` (`checker.go:15109`) |
    /// | `@types/…` | TS6137 is emitted *as well*, so the multiset would still differ |
    ///
    /// # Where a `None` from resolution is *not* a missing module
    ///
    /// [`Checker::resolve_external_module_name`] answers `None` both for "no
    /// file resolved" and for "a file resolved but it is not a module". Those
    /// are two upstream codes, so this rule cannot be written against that
    /// function's `Option` — it asks the host directly. That distinction is the
    /// single most load-bearing line in this file.
    fn check_module_specifier(
        &mut self,
        declaration: NodeId,
        specifier: Option<NodeId>,
        side_effect: bool,
    ) {
        let Some(specifier) = specifier else { return };
        let Some(Node::StringLiteral(literal)) = self.node_map.get(specifier) else { return };
        let text = literal.text;

        if !self.external_import_is_positioned_for_resolution(declaration) {
            return;
        }
        // `tryFindAmbientModule` (`checker.go:15154`), consulted before the host
        // exactly as `resolveExternalModule` does.
        if self.ambient_module_for_diagnostics(text).is_some() {
            return;
        }
        // A pattern ambient module is unported (`checker.go:15364`); declining
        // whenever one *exists* is the sound bound, not whenever one matches.
        if self.has_pattern_ambient_module() {
            return;
        }
        if is_node_core_module(text) {
            return;
        }
        if text.starts_with("@types/") {
            return;
        }
        let Some(importing) = self.source_file_of_for_diagnostics(specifier) else { return };
        // No host means no resolution was ever attempted, and "we did not look"
        // must not read as "it is not there".
        let Some(host) = self.module_host else { return };
        // `resolvedModule.IsResolved()` (`checker.go:15208`). A resolution that
        // named a file the program does not hold is upstream's TS7016 / TS6142 /
        // TS2306 territory — a *different code at the same position* — so it
        // must not read as "cannot find". Distinguishing that from "found
        // nothing" is why [`crate::resolution::ModuleHost`] grew a second
        // method; it was 15 of the 60 wrong lines the first counterfactual
        // measured.
        if host.module_resolution_found(importing, text) {
            return;
        }
        let span = self.nodes.span(specifier);
        let message = if side_effect {
            &messages::CANNOT_FIND_MODULE_OR_TYPE_DECLARATIONS_FOR_SIDE_EFFECT_IMPORT_OF_0
        } else {
            &messages::CANNOT_FIND_MODULE_0_OR_ITS_CORRESPONDING_TYPE_DECLARATIONS
        };
        self.report(importing, Diagnostic::with_args(message, span, [text.to_string()]));
    }

    /// TS2564 — `Property '{0}' has no initializer and is not definitely
    /// assigned in the constructor.`
    ///
    /// `checkPropertyInitialization` (`checker.go:4933`), called from
    /// `checkClassLikeDeclaration`'s last line (`checker.go:4390`). The error
    /// node is the **member's name**, so the reported column is the property
    /// name rather than the declaration or its type.
    ///
    /// # The bound: no constructor with a body, or nothing is said
    ///
    /// Upstream's condition is `constructor == nil ||
    /// !isPropertyInitializedInConstructor(...)` (`checker.go:4947`). The second
    /// disjunct synthesises a `this.x` property access, hangs it off the
    /// constructor's `ReturnFlowNode` and asks `getFlowTypeOfReference` whether
    /// `undefined` survives (`checker.go:4960` shows the sibling doing it for
    /// static blocks). **This port cannot synthesise that node**: the tree is
    /// arena-allocated and immutable after parsing
    /// ([ADR-0012](../../../docs/adr/0012-ast-is-sync.md)), and a flow query
    /// needs a *registered* node with a parent and a flow node.
    ///
    /// So only the first disjunct is ported, and a class that **has** a
    /// constructor with a body is declined outright. That is silence, never a
    /// wrong answer: upstream reports there only when the constructor fails to
    /// assign, and this port cannot tell those apart. The cost is measured
    /// rather than assumed — see `docs/architecture/checker-notes-diag2.md` §6.
    ///
    /// # `strictPropertyInitialization` is `strictNullChecks` here
    ///
    /// Upstream reads two separate `GetStrictOptionValue` results
    /// (`checker.go:919`, `:922`). This port models one strictness flag
    /// ([`Checker::strict_null_checks`], set from the case's directives), and
    /// both options default to `strict`, so the single flag is the faithful
    /// reading for every case that does not set them apart. A case writing
    /// `strictPropertyInitialization: false` under `strict: true` would be
    /// over-reported; the harness reads the directive and turns the rule off,
    /// which is where that divergence is repaired.
    fn check_property_initialization(&mut self, members: &[ClassElement<'_>], ambient: bool) {
        if !self.strict_null_checks || !self.strict_property_initialization || ambient {
            return;
        }
        // `ast.FindConstructorDeclaration` (`ast/utilities.go:2498`) — a
        // constructor **with a body**; an overload signature does not count.
        let has_constructor = members.iter().any(|member| {
            matches!(member, ClassElement::ConstructorDeclaration(ctor) if ctor.body.is_some())
        });
        if has_constructor {
            return;
        }
        for member in members {
            let ClassElement::PropertyDeclaration(property) = member else { continue };
            if has_modifier(property.modifiers, SyntaxKind::DeclareKeyword)
                || has_modifier(property.modifiers, SyntaxKind::StaticKeyword)
                || has_modifier(property.modifiers, SyntaxKind::AbstractKeyword)
            {
                continue;
            }
            // `isPropertyWithoutInitializer` (`checker.go:4956`): no `!`
            // postfix, no initialiser. A `?` postfix is *not* excluded here —
            // upstream lets it through and the `containsUndefinedType` test
            // below is what stops it, because an optional property's type
            // carries `undefined` under `strictNullChecks`.
            if property.initializer.is_some()
                || property
                    .postfix_token
                    .is_some_and(|token| token.kind == SyntaxKind::ExclamationToken)
            {
                continue;
            }
            // `IsIdentifier || IsPrivateIdentifier || IsComputedPropertyName`
            // (`checker.go:4944`). A string- or number-named property is
            // skipped by upstream too.
            let name = match property.name {
                tsr_ast::PropertyName::Identifier(identifier) => identifier.text,
                _ => continue,
            };
            let Some(id) = property.node_id else { continue };
            let Some(symbol) = self.binder.symbol_of(id) else { continue };
            let declared = self.get_type_of_symbol(symbol);
            if declared == self.intrinsics.error
                || declared == self.intrinsics.any
                || declared == self.intrinsics.unknown
                || self.contains_undefined_type(declared)
            {
                continue;
            }
            let Some(name_id) = property.name.node_id() else { continue };
            let Some(file) = self.source_file_of_for_diagnostics(name_id) else { continue };
            let span = self.nodes.span(name_id);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::PROPERTY_0_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
                    span,
                    [name.to_string()],
                ),
            );
        }
    }

    /// `containsUndefinedType` (`checker.go`): the type *is* `undefined`, or is
    /// a union with `undefined` among its constituents.
    ///
    /// **A gap counts as containing it.** This port answers `errorType` where it
    /// cannot compute, and a property whose type it cannot compute must not
    /// produce a diagnostic that depends on what that type is — which is why the
    /// caller tests `error` beside this.
    fn contains_undefined_type(&self, ty: crate::types::TypeId) -> bool {
        if ty == self.intrinsics.undefined {
            return true;
        }
        match &self.store.get(ty).data {
            crate::types::TypeData::Union { types, .. } => {
                types.contains(&self.intrinsics.undefined)
            }
            _ => false,
        }
    }

    /// TS2304 — `Cannot find name '{0}'.`
    ///
    /// `getResolvedSymbol` (`checker.go:13890`) resolves every identifier
    /// expression with `SymbolFlagsValue|SymbolFlagsExportValue` and a
    /// *nameNotFoundMessage*; failing to resolve lands in
    /// `onFailedToResolveSymbol` (`checker.go:1564`), whose **last line** is this
    /// diagnostic.
    ///
    /// # Everything above that last line is a decline, and there are ten
    ///
    /// `onFailedToResolveSymbol` runs seven `checkAndReportErrorFor…`
    /// predicates, a missing-lib suggestion and a spelling suggestion before it
    /// falls through, and **each of them reports a different code at the same
    /// position** — TS2662/2663 for a missing `this.`/`super.` prefix, TS2689
    /// for extending an interface, TS2702 for a type used as a namespace,
    /// TS2708/2709 for a namespace used as a value or type, TS2693/2749 for a
    /// type used as a value and the reverse, TS2583 for a name that needs a
    /// different `lib`, TS2552 for a spelling suggestion. On top of that
    /// `getCannotFindNameDiagnosticForName` (`checker.go:13915`) substitutes the
    /// whole message for fourteen well-known names before resolution even
    /// starts.
    ///
    /// This is the TS2307 shape again and worse: the diagnostic is the *residue*
    /// of a decision tree, so porting it means porting the tree. What is ported
    /// here is the subset whose declines are cheap and total:
    ///
    /// | declined | upstream's code |
    /// |---|---|
    /// | the fourteen names of `getCannotFindNameDiagnosticForName` | TS2580–TS2593 |
    /// | the name resolves under `TYPE` or `NAMESPACE` meaning | TS2693 / TS2709 / TS2749 / TS2702 |
    /// | any other symbol in the file is spelled within one edit | TS2552, approximated — see [`Checker::has_spelling_suggestion`] |
    /// | the identifier is not in an allow-listed value slot | not an identifier expression at all |
    ///
    /// The position test is an **allow**-list ([`Checker::is_value_reference`])
    /// rather than a deny-list, because the generic walk hands this rule every
    /// identifier in the file — declaration names, member names, labels, type
    /// references, import specifiers — and a missing deny-list arm is a false
    /// positive while a missing allow-list arm is only a missed conversion.
    fn check_value_identifier(&mut self, node: NodeId, text: &str) {
        // A file the parser could not read cleanly has a tree this port
        // *recovered*, and upstream recovered a different one. Reporting an
        // unresolvable name there is reporting about a program upstream never
        // saw — see [`crate::checker::Checker::file_has_parse_errors`]. It was
        // the single largest family in the residual: `jsxUnclosedParserRecovery`
        // 21 lines, `arrowFunctionsMissingTokens` 15,
        // `parserUnterminatedGeneric2` 8, and a long tail of `parserSkippedTokens`
        // and conflict-marker cases.
        if self.file_has_parse_errors {
            return;
        }
        if !self.is_value_reference(node) || is_specially_diagnosed_name(text) {
            return;
        }
        // Inside a `with` block upstream reports TS2410 — *"All symbols in a
        // 'with' block will have type 'any'"* — and resolves nothing
        // (`NodeFlagsInWithStatement`, read at `checker.go:29344` and four other
        // sites). The flag is another the parser here never sets, so the
        // question is asked of the ancestors.
        if self.is_inside_with_statement(node) {
            return;
        }
        // `!ast.NodeIsMissing(node)` (`checker.go:13894`) — upstream does not
        // resolve, and therefore never reports, an identifier the parser
        // synthesised while recovering. `NodeIsMissing` is `pos == end`, and a
        // missing identifier also carries empty text.
        let span = self.nodes.span(node);
        if text.is_empty() || span.start == span.end {
            return;
        }
        if self
            .binder
            .resolve_name(self.nodes, self.node_map, node, text, SymbolFlags::VALUE)
            .is_some()
        {
            return;
        }
        // `checkAndReportErrorForUsingTypeAsValue` / `…NamespaceAsTypeOrValue`
        // (`checker.go:1681`, `:1643`): a name that resolves under another
        // meaning gets a *different* code, so silence is the only sound answer
        // until those arms are ported.
        if self
            .binder
            .resolve_name(self.nodes, self.node_map, node, text, SymbolFlags::TYPE)
            .is_some()
            || self
                .binder
                .resolve_name(self.nodes, self.node_map, node, text, SymbolFlags::NAMESPACE)
                .is_some()
        {
            return;
        }
        if self.has_spelling_suggestion(node, text) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        self.report(
            file,
            Diagnostic::with_args(&messages::CANNOT_FIND_NAME_0, span, [text.to_string()]),
        );
    }

    /// Would `getSuggestedSymbolForNonexistentSymbol` (`checker.go:1591`) find
    /// something, so that upstream reports TS2552 rather than TS2304?
    ///
    /// The **whole** of TS2304 turns on this. `onFailedToResolveSymbol`
    /// (`checker.go:1564`) tries a spelling suggestion immediately before its
    /// fallthrough, so every name with a near neighbour in scope is a TS2552 and
    /// reporting TS2304 there is a wrong code at a right position. A first
    /// attempt used a hand-rolled within-one-edit test and
    /// `conformance/parserS7.6_A4.2_T1` alone produced **20 wrong lines** from
    /// it: `$ERROR` against `Error` is one deletion plus five case differences,
    /// which upstream's weighted distance accepts and a plain edit count does
    /// not. The algorithm is ported instead — see [`spelling_suggestion`].
    fn has_spelling_suggestion(&self, node: NodeId, text: &str) -> bool {
        let candidates = self.binder.names_in_scope(self.nodes, self.node_map, node);
        spelling_suggestion(text, &candidates).is_some()
    }

    /// Is this identifier in a slot where upstream would call
    /// `getResolvedSymbol` on it?
    ///
    /// An allow-list over the **parent's** shape: the identifier must be the
    /// node sitting in one of the parent's expression-typed fields. Every arm
    /// names a field rather than a kind, because the discriminating question is
    /// never "what is the parent" but "which of its slots is this" — a
    /// `PropertyAccessExpression` resolves its `expression` and never its
    /// `name`, and conflating those reports `Cannot find name 'length'` on every
    /// `a.length` in the corpus.
    #[allow(
        clippy::too_many_lines,
        reason = "one arm per expression-bearing node kind; splitting it would \
                  hide the exhaustiveness that is the point of the list"
    )]
    fn is_value_reference(&self, node: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(node) else { return false };
        let Some(typed) = self.node_map.get(parent) else { return false };
        let is = |slot: Option<NodeId>| slot == Some(node);
        let is_any = |slots: &[Option<NodeId>]| slots.contains(&Some(node));
        match typed {
            Node::ExpressionStatement(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::PropertyAccessExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::ElementAccessExpression(n) => is_any(&[
                n.expression.and_then(|e| e.node_id()),
                n.argument_expression.and_then(|e| e.node_id()),
            ]),
            Node::CallExpression(n) => {
                is(n.expression.and_then(|e| e.node_id()))
                    || n.arguments.iter().any(|a| a.node_id() == Some(node))
            }
            Node::NewExpression(n) => {
                is(n.expression.and_then(|e| e.node_id()))
                    || n.arguments.iter().any(|a| a.node_id() == Some(node))
            }
            Node::BinaryExpression(n) => {
                is_any(&[n.left.and_then(|e| e.node_id()), n.right.and_then(|e| e.node_id())])
            }
            Node::PrefixUnaryExpression(n) => is(n.operand.and_then(|e| e.node_id())),
            Node::PostfixUnaryExpression(n) => is(n.operand.and_then(|e| e.node_id())),
            Node::ConditionalExpression(n) => is_any(&[
                n.condition.and_then(|e| e.node_id()),
                n.when_true.and_then(|e| e.node_id()),
                n.when_false.and_then(|e| e.node_id()),
            ]),
            Node::ParenthesizedExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::ArrayLiteralExpression(n) => n.elements.iter().any(|e| e.node_id() == Some(node)),
            // The *initialiser*, never the name — `{ a: b }` resolves `b`.
            Node::PropertyAssignment(n) => is(n.initializer.and_then(|e| e.node_id())),
            // `{ a }` is both a name and a reference, which is the one place a
            // declaration name is also resolved.
            Node::ShorthandPropertyAssignment(n) => {
                is(n.name.node_id())
                    || is(n.object_assignment_initializer.and_then(|e| e.node_id()))
            }
            Node::SpreadElement(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::SpreadAssignment(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::TemplateSpan(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::TaggedTemplateExpression(n) => is(n.tag.and_then(|e| e.node_id())),
            Node::TypeAssertion(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::AsExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::SatisfiesExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::NonNullExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::AwaitExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::YieldExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::TypeOfExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::VoidExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::DeleteExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::VariableDeclaration(n) => is(n.initializer.and_then(|e| e.node_id())),
            Node::ParameterDeclaration(n) => is(n.initializer.and_then(|e| e.node_id())),
            Node::PropertyDeclaration(n) => is(n.initializer.and_then(|e| e.node_id())),
            Node::BindingElement(n) => is(n.initializer.and_then(|e| e.node_id())),
            Node::EnumMember(n) => is(n.initializer.and_then(|e| e.node_id())),
            Node::ReturnStatement(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::ThrowStatement(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::IfStatement(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::WhileStatement(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::DoStatement(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::SwitchStatement(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::CaseOrDefaultClause(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::ForStatement(n) => is_any(&[
                n.initializer.and_then(|e| e.node_id()),
                n.condition.and_then(|e| e.node_id()),
                n.incrementor.and_then(|e| e.node_id()),
            ]),
            Node::ForInOrOfStatement(n) => is_any(&[
                n.initializer.and_then(|e| e.node_id()),
                n.expression.and_then(|e| e.node_id()),
            ]),
            Node::ComputedPropertyName(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::Decorator(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::ExportAssignment(n) => is(n.expression.and_then(|e| e.node_id())),
            Node::JsxExpression(n) => is(n.expression.and_then(|e| e.node_id())),
            // `class C extends B` resolves `B` as a value; `implements I` does
            // not, and the two share this node kind. The heritage clause's
            // keyword is what separates them.
            Node::ExpressionWithTypeArguments(n) => {
                is(n.expression.and_then(|e| e.node_id()))
                    && self.nodes.parent(parent).is_some_and(|clause| {
                        matches!(
                            self.node_map.get(clause),
                            Some(Node::HeritageClause(heritage))
                                if heritage.token.kind == SyntaxKind::ExtendsKeyword
                        )
                    })
            }
            _ => false,
        }
    }

    /// TS2454 — `Variable '{0}' is used before being assigned.`
    ///
    /// `checkIdentifier` (`checker.go:11191`), the arm reached when
    /// `assumeInitialized` is false and the *flow* type carries `undefined`
    /// while the declared type does not.
    ///
    /// # This is a bound, and the bound is `assumeInitialized`
    ///
    /// Upstream's `assumeInitialized` (`checker.go:11150`) is a nine-way
    /// disjunction, and every disjunct that is false is a diagnostic. Rather
    /// than port the ones that need machinery this port lacks —
    /// `isSymbolAssignedDefinitely` needs `markNodeAssignments`,
    /// `isPastLastAssignment` needs assignment positions — the rule **requires
    /// the shape where those disjuncts cannot matter**:
    ///
    /// - the symbol's declaration is a plain `VariableDeclaration` with a type
    ///   annotation, so `isParameter`, `isAlias`, `isSameScopedBindingElement`
    ///   and the auto-typed path are all excluded by construction;
    /// - the reference's control-flow container **is** the declaration's, so
    ///   `isOuterVariable` is false and `isNeverInitialized` — the only consumer
    ///   of `isSymbolAssignedDefinitely` — is never consulted;
    /// - the reference is not a definite assignment target, which
    ///   `checker.go:11109` returns early for.
    ///
    /// The remaining disjuncts are syntactic and are ported: a `!` on the
    /// declaration, an ambient declaration, `typeof x`, an ambient-or-type-node
    /// position, an `ExportSpecifier` parent, a `NonNullExpression` parent.
    ///
    /// What the bound gives up is every `let x: T` referenced from inside a
    /// nested function — measured rather than assumed, in
    /// `docs/architecture/checker-notes-diag2.md` §8.
    fn check_used_before_assigned(&mut self, node: NodeId, text: &str) {
        if self.file_has_parse_errors || !self.strict_null_checks || !self.is_value_reference(node)
        {
            return;
        }
        // `assignmentKind == AssignmentKindDefinite` returns before the flow
        // section (`checker.go:11109`), so `x = 1` never reports even though the
        // flow type at `x` carries `undefined`.
        if self.is_definite_assignment_target(node) {
            return;
        }
        if self.is_inside_with_statement(node) || self.is_in_type_query_or_type_node(node) {
            return;
        }
        let Some(parent) = self.nodes.parent(node) else { return };
        if matches!(
            self.nodes.kind(parent),
            SyntaxKind::ExportSpecifier | SyntaxKind::NonNullExpression
        ) {
            return;
        }
        let Some(symbol) =
            self.binder.resolve_name(self.nodes, self.node_map, node, text, SymbolFlags::VALUE)
        else {
            return;
        };
        let Some(declaration) = self.binder.symbols().get(symbol).declarations.first().copied()
        else {
            return;
        };
        let Some(Node::VariableDeclaration(variable)) = self.node_map.get(declaration) else {
            return;
        };
        // No initialiser, no `!`, and an explicit annotation — the annotation is
        // what keeps the auto-typed path (`t == autoType`, a different
        // diagnostic entirely) out of this rule.
        if variable.initializer.is_some()
            || variable.exclamation_token.is_some()
            || variable.r#type.is_none()
        {
            return;
        }
        // A `const` with no initialiser only occurs in an ambient context or
        // after a grammar error (TS1155), and upstream reaches neither: the
        // ambient flag short-circuits `assumeInitialized`
        // (`checker.go:11158`). `declare const b: B` supplied **227 of the
        // first measurement's 4,781 wrong lines from one case**
        // (`compiler/genericDefaults`), which is what put both tests here.
        let Some(list) = self.nodes.parent(declaration) else { return };
        if self.nodes.flags(list).intersects(tsr_ast::NodeFlags::CONST) {
            return;
        }
        if self.nodes.parent(list).and_then(|statement| self.node_map.get(statement)).is_some_and(
            |statement| match statement {
                Node::VariableStatement(variable) => {
                    has_modifier(variable.modifiers, SyntaxKind::DeclareKeyword)
                }
                _ => false,
            },
        ) {
            return;
        }
        // `for (x of …)` and `for (x in …)` assign on entry.
        if self.nodes.parent(list).is_some_and(|owner| {
            matches!(
                self.nodes.kind(owner),
                SyntaxKind::ForInStatement | SyntaxKind::ForOfStatement
            )
        }) {
            return;
        }
        // `isOuterVariable` (`checker.go:11128`): with different containers the
        // graph cannot be analysed from the declaration, and upstream assumes
        // initialised. Here it is a refusal rather than an assumption, which is
        // the same behaviour and a different reason.
        if self.control_flow_container(node) != self.control_flow_container(declaration) {
            return;
        }
        let declared = self.get_type_of_symbol(symbol);
        if declared == self.intrinsics.error
            || declared == self.intrinsics.any
            || declared == self.intrinsics.unknown
            || declared == self.intrinsics.void
            || self.contains_undefined_type(declared)
        {
            return;
        }
        let initial = self.get_optional_type(declared, false);
        let flow = self.get_flow_type_of_reference_ex(node, Some(symbol), declared, Some(initial));
        if flow == self.intrinsics.error || !self.contains_undefined_type(flow) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.nodes.span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::VARIABLE_0_IS_USED_BEFORE_BEING_ASSIGNED,
                span,
                [text.to_string()],
            ),
        );
    }

    /// `getControlFlowContainer` (`checker.go:11438`): the innermost enclosing
    /// function, module block, source file or property declaration.
    fn control_flow_container(&self, node: NodeId) -> Option<NodeId> {
        let mut current = self.nodes.parent(node);
        while let Some(id) = current {
            if matches!(
                self.nodes.kind(id),
                SyntaxKind::FunctionDeclaration
                    | SyntaxKind::FunctionExpression
                    | SyntaxKind::ArrowFunction
                    | SyntaxKind::MethodDeclaration
                    | SyntaxKind::Constructor
                    | SyntaxKind::GetAccessor
                    | SyntaxKind::SetAccessor
                    | SyntaxKind::ModuleBlock
                    | SyntaxKind::SourceFile
                    | SyntaxKind::PropertyDeclaration
            ) {
                return Some(id);
            }
            current = self.nodes.parent(id);
        }
        None
    }

    /// Is this identifier the left side of a plain `=`?
    ///
    /// `AssignmentKindDefinite`. A compound assignment (`x += 1`) reads before
    /// it writes and is *not* excluded, which is upstream's split at
    /// `checker.go:11110` (`isInCompoundLikeAssignment`).
    fn is_definite_assignment_target(&self, node: NodeId) -> bool {
        self.nodes.parent(node).is_some_and(|parent| {
            matches!(
                self.node_map.get(parent),
                Some(Node::BinaryExpression(binary))
                    if binary.operator_token.is_some_and(|token| token.kind == SyntaxKind::EqualsToken)
                        && binary.left.and_then(|left| left.node_id()) == Some(node)
            )
        })
    }

    /// `IsInTypeQuery` and `isInAmbientOrTypeNode` (`utilities.go:1057`),
    /// collapsed: both are ancestor walks and both mean "not a value position
    /// the flow graph describes".
    fn is_in_type_query_or_type_node(&self, node: NodeId) -> bool {
        self.nodes.ancestors(node).any(|id| {
            matches!(
                self.nodes.kind(id),
                SyntaxKind::TypeQuery
                    | SyntaxKind::InterfaceDeclaration
                    | SyntaxKind::TypeAliasDeclaration
                    | SyntaxKind::TypeLiteral
            )
        })
    }

    /// Append to the collection upstream keeps as `c.diagnostics`
    /// (`checker.go:661`), drained by `GetDiagnostics` (`checker.go:13951`).
    fn report(&mut self, file: NodeId, diagnostic: Diagnostic) {
        self.diagnostics.push((file, diagnostic));
    }

    /// Every diagnostic this checker has reported, with the file each is in.
    ///
    /// The consumer *drains*; it does not compute. That is ADR-0040 decision
    /// (1), and the reason the collection lives on the `Checker` rather than
    /// being assembled by whoever walks the tree.
    #[must_use]
    pub fn diagnostics(&self) -> &[(NodeId, Diagnostic)] {
        &self.diagnostics
    }

    /// Is there any `declare module "…*…"` in the program?
    ///
    /// Upstream keeps `c.patternAmbientModules`, filled while collecting
    /// globals; this port has no such list, so the question is asked of the
    /// binder's global table. **A name-shape test, and it is deliberately
    /// coarse**: a global whose name contains `*` can only have come from a
    /// pattern module declaration, and the cost of a false `true` is silence on
    /// one case rather than a wrong diagnostic on another.
    fn has_pattern_ambient_module(&self) -> bool {
        self.binder.globals().keys().any(|name| name.contains('*'))
    }

    /// [`Checker::ambient_module`] is private to `symbols`; this is the same
    /// question asked from here.
    ///
    /// Duplicated rather than exposed because the `symbols` version is on the
    /// resolution path and its `pub(crate)` surface is what
    /// `checker-notes-modobj.md` §10 pins; a second caller changing that
    /// function's visibility for a diagnostic is the kind of coupling that makes
    /// the next resolution change look risky.
    fn ambient_module_for_diagnostics(&self, name: &str) -> Option<tsr_binder::SymbolId> {
        if name == "." || name == ".." || name.starts_with("./") || name.starts_with("../") {
            return None;
        }
        let &symbol = self.binder.globals().get(name)?;
        let symbol = self.binder.merged_symbol(symbol);
        self.binder
            .symbols()
            .get(symbol)
            .flags
            .intersects(tsr_binder::SymbolFlags::VALUE_MODULE)
            .then_some(symbol)
    }

    /// `node.Flags & ast.NodeFlagsInWithStatement`, recomputed from the tree.
    ///
    /// The *statement* of a `with`, not its expression: `with (a) { b }`
    /// resolves `a` normally and refuses `b`.
    fn is_inside_with_statement(&self, node: NodeId) -> bool {
        let mut current = node;
        while let Some(parent) = self.nodes.parent(current) {
            if let Some(Node::WithStatement(with)) = self.node_map.get(parent)
                && with.statement.and_then(|s| s.node_id()) == Some(current)
            {
                return true;
            }
            current = parent;
        }
        false
    }

    /// `ast.GetSourceFileOfNode`, reachable from this module.
    fn source_file_of_for_diagnostics(&self, node: NodeId) -> Option<NodeId> {
        let mut current = node;
        loop {
            if self.nodes.kind(current) == SyntaxKind::SourceFile {
                return Some(current);
            }
            current = self.nodes.parent(current)?;
        }
    }
}

/// `core.NodeCoreModules()` (`internal/core/nodemodules.go`).
///
/// Upstream substitutes a *different* message for these
/// (`getCannotResolveModuleNameErrorForSpecificModule`, `checker.go:15109`), so
/// the list is a **refusal list** here rather than a resolution one: a name on
/// it is never reported as TS2307. Transcribed from upstream rather than
/// guessed, because a name missing from it becomes a false positive and a name
/// wrongly on it costs only silence.
fn is_node_core_module(name: &str) -> bool {
    let bare = name.strip_prefix("node:").unwrap_or(name);
    NODE_CORE_MODULES.contains(&bare) || name.starts_with("node:")
}

/// The names `core.NodeCoreModules()` returns.
const NODE_CORE_MODULES: &[&str] = &[
    "assert",
    "assert/strict",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "dns/promises",
    "domain",
    "events",
    "fs",
    "fs/promises",
    "http",
    "http2",
    "https",
    "inspector",
    "inspector/promises",
    "module",
    "net",
    "os",
    "path",
    "path/posix",
    "path/win32",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "readline/promises",
    "repl",
    "stream",
    "stream/consumers",
    "stream/promises",
    "stream/web",
    "string_decoder",
    "sys",
    "timers",
    "timers/promises",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "util/types",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
];

/// `ast.HasSyntacticModifier` for one keyword.
///
/// A decorator in the modifier list is not a modifier; the enum keeps them
/// together because the parser does (`ModifierLike`).
fn has_modifier(modifiers: &[ModifierLike<'_>], keyword: SyntaxKind) -> bool {
    modifiers
        .iter()
        .any(|modifier| matches!(modifier, ModifierLike::Token(token) if token.kind == keyword))
}

/// The fourteen names `getCannotFindNameDiagnosticForName` (`checker.go:13915`)
/// substitutes a different message for.
///
/// Upstream picks TS2580/2581/2582/2583/2584/2591/2593 for these depending on
/// the name and on `UsesWildcardTypes`, so reporting TS2304 for any of them is a
/// wrong code at a right position. A refusal list, like the Node core modules
/// above.
fn is_specially_diagnosed_name(name: &str) -> bool {
    matches!(
        name,
        // Not one of upstream's fourteen: `arguments` is *synthesised* by
        // `resolveName`'s own `arguments` arm (`nameresolver.go`) for every
        // function-like container, and this binder declares no such symbol. So
        // every `arguments` reference in the corpus would resolve to nothing
        // here and to `IArguments` upstream. `bd tsr-o9tl`; the row is 399 lines
        // of the `.types` gradient too (STATUS §4.3).
        "arguments"
            // `globalThis` is a *synthesised* global upstream declares in
            // `initializeGlobals`; this binder declares no symbol for it, so
            // every reference would be reported. A refusal, not a resolution.
            | "globalThis"
            | "document"
            | "console"
            | "$"
            | "beforeEach"
            | "describe"
            | "suite"
            | "it"
            | "test"
            | "process"
            | "require"
            | "Buffer"
            | "module"
            | "NodeJS"
            | "Bun"
    )
}

/// `core.GetSpellingSuggestion` (`internal/core/core.go:559`) — the closest
/// candidate to `name`, or `None` when nothing is close enough.
///
/// # Ported exactly, because an approximation is a wrong diagnostic
///
/// The distance is **not** plain Levenshtein. A case-only substitution costs
/// `0.1` and any other substitution costs `2` (`core.go:650`-`:653`), which is
/// what makes `$ERROR` a suggestion for `Error` — one deletion at cost 1 plus
/// five case differences at 0.1 each — while five *character* differences would
/// be 10 and miss by a mile. The acceptance threshold is
/// `floor(0.4 * len) + 0.9` and the length filter is `max(2, 0.34 * len)`.
///
/// Two deliberate departures, both stated:
///
/// - Upstream compares `len(candidateName)` in **bytes** against
///   `len(runeName)` in **runes** (`core.go:583`), a Go slip that only shows on
///   non-ASCII names. This compares runes on both sides. The corpus case that
///   drove this rule — `parserS7.6_A4.2_T1`, the Cyrillic alphabet — is exactly
///   where the two differ, and matching upstream's *behaviour* there would mean
///   reproducing the slip.
/// - The candidate ordering tie-break (`compare(candidate, bestCandidate)`) is
///   dropped: this caller asks only whether a suggestion exists, never which.
///
/// It lives here rather than in `tsr-core` — upstream's home for it — because
/// there is one consumer. The second consumer is the scanner's regular-
/// expression property-name suggestions (`scanner/regexp.go:955`), unported;
/// it moves when that lands.
fn spelling_suggestion<'a>(name: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let target: Vec<char> = name.chars().collect();
    #[allow(clippy::cast_precision_loss, reason = "identifier lengths are small")]
    let length = target.len() as f64;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "0.34 * a small positive length"
    )]
    let maximum_length_difference = usize::max(2, (length * 0.34) as usize);
    let mut best_distance = (length * 0.4).floor() + 0.9;
    let mut best: Option<&'a str> = None;

    for candidate in candidates {
        if candidate.is_empty() || *candidate == name {
            continue;
        }
        let other: Vec<char> = candidate.chars().collect();
        if usize::abs_diff(other.len(), target.len()) > maximum_length_difference {
            continue;
        }
        // "Only consider candidates less than 3 characters long when they
        // differ by case" (`core.go:589`).
        if other.len() < 3 && !candidate.eq_ignore_ascii_case(name) {
            continue;
        }
        if let Some(distance) = levenshtein_with_max(&target, &other, best_distance) {
            if distance < best_distance {
                best_distance = distance;
            }
            best = Some(candidate);
        }
    }
    best
}

/// `core.levenshteinWithMax` (`internal/core/core.go:627`).
///
/// Returns `None` for upstream's `-1`: the distance exceeded `max_value` and no
/// column can recover, so the candidate is rejected.
fn levenshtein_with_max(s1: &[char], s2: &[char], max_value: f64) -> Option<f64> {
    let width = s2.len() + 1;
    let big = max_value + 0.01;
    #[allow(clippy::cast_precision_loss, reason = "identifier lengths are small")]
    let mut previous: Vec<f64> = (0..width).map(|i| i as f64).collect();
    let mut current: Vec<f64> = vec![0.0; width];

    for i in 1..=s1.len() {
        #[allow(clippy::cast_precision_loss, reason = "identifier lengths are small")]
        let row = i as f64;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to at least 1 by the max below"
        )]
        let min_j = usize::max((row - max_value).ceil().max(1.0) as usize, 1);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to s2's length by the min below"
        )]
        let max_j = usize::min((max_value + row).floor().max(0.0) as usize, s2.len());
        let mut column_min = row;
        current[0] = row;
        for slot in current.iter_mut().take(min_j).skip(1) {
            *slot = big;
        }
        for j in min_j..=max_j {
            // A case-only difference costs 0.1; any other substitution costs 2.
            let substitution = if s1[i - 1].to_lowercase().eq(s2[j - 1].to_lowercase()) {
                previous[j - 1] + 0.1
            } else {
                previous[j - 1] + 2.0
            };
            let distance = if s1[i - 1] == s2[j - 1] {
                previous[j - 1]
            } else {
                (previous[j] + 1.0).min((current[j - 1] + 1.0).min(substitution))
            };
            current[j] = distance;
            column_min = column_min.min(distance);
        }
        for slot in current.iter_mut().take(s2.len() + 1).skip(max_j + 1) {
            *slot = big;
        }
        if column_min > max_value {
            return None;
        }
        std::mem::swap(&mut previous, &mut current);
    }
    let result = previous[s2.len()];
    (result <= max_value).then_some(result)
}
