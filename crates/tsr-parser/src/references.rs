//! Which modules a file names, and how it names them.
//!
//! Ported from `internal/parser/references.go` (`collectExternalModuleReferences`,
//! `collectModuleReferences`) plus the parts of `internal/ast/parseoptions.go` and
//! `internal/ast/utilities.go` that decide what counts
//! (`isFileProbablyExternalModule`, `ForEachDynamicImportOrRequireCall`) at the
//! pinned commit.
//!
//! # Why this lives in the parser and not the loader
//!
//! It is where upstream puts it, and for a reason that survives the port: this is
//! syntax. Deciding that `import x = require("y")` names a module and
//! `import x = N` does not is a question about the tree, and the file loader —
//! which is about *program* structure — should be handed the answer rather than
//! re-deriving it. The loader's job starts at "here is a specifier and the syntax
//! that produced it".
//!
//! # Why a specifier carries its context instead of a parent pointer
//!
//! Upstream hands the loader `*ast.Node`s and reads `usage.Parent` in
//! `getModeForUsageLocation` to decide whether a specifier is an `import`
//! statement, a `require()` call, a dynamic `import()`, or an `import type`. The
//! tree here has no back-edges ([ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md)),
//! so the parent would have to be looked up in [`tsr_ast::NodeTable`] and then
//! matched against six kinds — reconstructing, one table read at a time,
//! something this walk already knows at the moment it finds the specifier.
//!
//! So each specifier records its [`SpecifierContext`] and its
//! `resolution-mode` override directly. That is strictly more information than
//! the parent pointer gives (the override lives two nodes further up), and it
//! makes the loader's mode derivation a `match` rather than a tree query.
//!
//! # The dynamic-import scan is a tree walk, not a text scan
//!
//! Upstream's `ForEachDynamicImportOrRequireCall` scans the file *text* for
//! `import`/`require`, maps each hit back to a node, and keeps the ones that are
//! genuinely calls. That is a way of avoiding a full tree walk on files that
//! contain neither word; it is not a different answer. This walks the tree and
//! sorts the results by position, which yields the same specifiers in the same
//! order.
//!
//! One upstream guard is deliberately dropped: the walk is gated on
//! `NodeFlagsPossiblyContainsDynamicImport`, a flag the parser sets exactly when
//! it parses an `import(` call or an `import` type. A file without the flag has
//! neither node, so the walk over it finds nothing — the flag is an optimisation,
//! and this port does not track source flags. The `|| IsInJSFile(file)` half of
//! the same condition *is* load-bearing (`require()` calls set no flag) and is
//! preserved as [`CollectOptions::is_js_file`].

use tsr_ast::{
    CallExpression, ImportAttributes, ModifierLike, ModuleBody, ModuleName, Node, NodeTable,
    SourceFile, Statement, SyntaxKind, push_children,
};
use tsr_path::is_external_module_name_relative;

use crate::pragma::ResolutionMode;

/// The syntax that produced a module specifier.
///
/// Enough to reproduce `getModeForUsageLocation`'s parent tests without a parent
/// pointer. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecifierContext {
    /// `import … from "x"` — including the bodyless `import "x"`.
    ImportDeclaration,
    /// `export … from "x"`.
    ExportDeclaration,
    /// `import x = require("x")`.
    ImportEquals,
    /// `import("x")` in expression position.
    ImportCall,
    /// `require("x")` in a JavaScript file.
    RequireCall,
    /// `import("x")` in type position.
    ImportType,
    /// `declare module "x" { … }` augmenting an existing module.
    ModuleAugmentation,
    /// Not written in the file: `importHelpers`' `tslib`, or the JSX runtime.
    Synthetic,
}

/// One module specifier, with what the loader needs to resolve it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSpecifier {
    /// The specifier text, unquoted.
    pub text: String,
    /// Where the *enclosing* syntax starts, which is what orders the dynamic
    /// scan. Zero for [`SpecifierContext::Synthetic`].
    pub pos: u32,
    /// What syntax named it.
    pub context: SpecifierContext,
    /// A `with { "resolution-mode": … }` attribute on a type-only import, or on
    /// an `import type` node (`ImportAttributes.GetResolutionModeOverride`).
    pub resolution_mode_override: ResolutionMode,
}

/// What a file says about other modules
/// (`SourceFile.Imports`/`ModuleAugmentations`/`AmbientModuleNames`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExternalModuleReferences {
    /// Every specifier the file imports, in the order upstream resolves them:
    /// statement-level specifiers in source order, then dynamic `import()`,
    /// `require()`, and `import` types in position order.
    pub imports: Vec<ModuleSpecifier>,
    /// `declare module "x"` blocks that augment an existing module. Only the
    /// string-named ones — `declare global` names an identifier and resolves
    /// nothing.
    pub module_augmentations: Vec<ModuleSpecifier>,
    /// `declare module "x"` blocks that *declare* a module rather than augment
    /// one. These are not resolved; they satisfy imports of that name.
    pub ambient_module_names: Vec<String>,
}

/// What the walk needs to know that the tree does not say.
#[derive(Debug, Clone, Copy)]
pub struct CollectOptions {
    /// Whether the file is a `.d.ts`, which makes every top-level
    /// `module "x"` ambient without a `declare` modifier.
    pub is_declaration_file: bool,
    /// Whether the file is JavaScript, which is the only place `require()` is
    /// followed.
    pub is_js_file: bool,
    /// Whether the file is an external module
    /// (`ast.IsExternalModule` — the *resolved* indicator, which depends on
    /// compiler options and so is computed by the loader, not here).
    pub is_external_module: bool,
}

/// Collect every module this file names (`collectExternalModuleReferences`).
#[must_use]
pub fn collect_external_module_references(
    file: &SourceFile<'_>,
    nodes: &NodeTable,
    options: CollectOptions,
) -> ExternalModuleReferences {
    let mut result = ExternalModuleReferences::default();
    for statement in file.statements {
        collect_module_references(*statement, false, nodes, options, &mut result);
    }
    result.imports.extend(collect_dynamic_imports(file, nodes, options.is_js_file));
    result
}

/// `collectModuleReferences`.
fn collect_module_references(
    statement: Statement<'_>,
    in_ambient_module: bool,
    nodes: &NodeTable,
    options: CollectOptions,
    result: &mut ExternalModuleReferences,
) {
    // `ast.IsAnyImportOrReExport` + `ast.GetExternalModuleName`, fused: the
    // union of kinds and the field to read from each are the same match.
    let import = match statement {
        Statement::ImportDeclaration(node) => node.module_specifier.map(|specifier| {
            (
                specifier,
                SpecifierContext::ImportDeclaration,
                node.attributes,
                is_type_only_import(node.import_clause),
            )
        }),
        Statement::ExportDeclaration(node) => node.module_specifier.map(|specifier| {
            (specifier, SpecifierContext::ExportDeclaration, node.attributes, node.is_type_only)
        }),
        Statement::ImportEqualsDeclaration(node) => match node.module_reference {
            Some(tsr_ast::ModuleReference::ExternalModuleReference(reference)) => reference
                .expression
                .map(|expression| (expression, SpecifierContext::ImportEquals, None, false)),
            // `import M = N` names no module. One of the three corpus cases
            // upstream records an *empty* trace for turns on this arm.
            _ => None,
        },
        _ => None,
    };
    if let Some((expression, context, attributes, type_only)) = import {
        // TypeScript 1.0 spec (April 2014): 12.1.6 — an import inside an ambient
        // external module may only name top-level modules.
        if let Node::StringLiteral(literal) = Node::from(expression)
            && !literal.text.is_empty()
            && (!in_ambient_module || !is_external_module_name_relative(literal.text))
        {
            result.imports.push(ModuleSpecifier {
                text: literal.text.to_string(),
                pos: node_pos(nodes, Node::from(statement)),
                context,
                // Upstream only consults the override for a type-only import;
                // `getModeForUsageLocation` gates on
                // `IsExclusivelyTypeOnlyImportOrExport` before reading it.
                resolution_mode_override: if type_only {
                    resolution_mode_override(attributes)
                } else {
                    ResolutionMode::None
                },
            });
        }
        return;
    }

    // `declare module "x"` — an augmentation, an ambient declaration, or
    // neither.
    let Statement::ModuleDeclaration(module) = statement else { return };
    let Some(ModuleName::StringLiteral(name)) = module.name else { return };
    if !(in_ambient_module || has_declare_modifier(module.modifiers) || options.is_declaration_file)
    {
        return;
    }

    if options.is_external_module
        || (in_ambient_module && !is_external_module_name_relative(name.text))
    {
        // An ambient module declaration inside a module — or nested inside
        // another ambient module under a non-relative name — augments an
        // existing module, so the loader must resolve it.
        result.module_augmentations.push(ModuleSpecifier {
            text: name.text.to_string(),
            pos: node_pos(nodes, Node::from(statement)),
            context: SpecifierContext::ModuleAugmentation,
            resolution_mode_override: ResolutionMode::None,
        });
    } else if !in_ambient_module {
        result.ambient_module_names.push(name.text.to_string());
        // The body of an ambient module is always a module block if present.
        if let Some(ModuleBody::ModuleBlock(block)) = module.body {
            for nested in block.statements {
                collect_module_references(*nested, true, nodes, options, result);
            }
        }
    }
}

/// Dynamic `import()`, `require()` in JS, and `import` types, in position order
/// (`ForEachDynamicImportOrRequireCall`).
fn collect_dynamic_imports(
    file: &SourceFile<'_>,
    nodes: &NodeTable,
    is_js_file: bool,
) -> Vec<ModuleSpecifier> {
    let mut found: Vec<ModuleSpecifier> = Vec::new();
    let mut stack: Vec<Node<'_>> = vec![Node::from(file)];
    let mut children: Vec<Node<'_>> = Vec::new();

    while let Some(node) = stack.pop() {
        match node {
            Node::CallExpression(call) => {
                if let Some(specifier) = dynamic_call_specifier(call, nodes, is_js_file) {
                    found.push(specifier);
                }
            }
            Node::ImportTypeNode(import_type) => {
                // `IsLiteralImportTypeNode`: the argument must be a literal type
                // wrapping a string. `import(typeof x)` names nothing.
                if let Some(tsr_ast::TypeNode::LiteralTypeNode(literal_type)) = import_type.argument
                    && let Some(Node::StringLiteral(literal)) = literal_type.literal
                {
                    found.push(ModuleSpecifier {
                        text: literal.text.to_string(),
                        pos: node_pos(nodes, node),
                        context: SpecifierContext::ImportType,
                        resolution_mode_override: resolution_mode_override(import_type.attributes),
                    });
                }
            }
            _ => {}
        }
        children.clear();
        push_children(node, &mut children);
        stack.extend(children.iter().copied());
    }

    // The stack walk visits siblings back to front; upstream's text scan is
    // strictly left to right, and `resolutionsInFile` is keyed by insertion.
    found.sort_by_key(|specifier| specifier.pos);
    found
}

/// The specifier of an `import("x")` or, in JavaScript, a `require("x")`.
fn dynamic_call_specifier(
    call: &CallExpression<'_>,
    nodes: &NodeTable,
    is_js_file: bool,
) -> Option<ModuleSpecifier> {
    let context = match call.expression? {
        // `import(...)`: the callee is the keyword itself. `KeywordExpression`
        // carries its kind in the node table rather than in a field, so the
        // check is a table read.
        tsr_ast::Expression::KeywordExpression(keyword)
            if node_kind(nodes, Node::from(keyword)) == Some(SyntaxKind::ImportKeyword) =>
        {
            SpecifierContext::ImportCall
        }
        // `require(...)`: `IsRequireCall` also demands exactly one argument.
        tsr_ast::Expression::Identifier(name)
            if is_js_file && name.text == "require" && call.arguments.len() == 1 =>
        {
            SpecifierContext::RequireCall
        }
        _ => return None,
    };
    // `requireStringLiteralLikeArgument`: `import(String())` names nothing, and
    // one of the three empty-trace corpus cases is exactly that.
    let argument = *call.arguments.first()?;
    let text = match Node::from(argument) {
        Node::StringLiteral(literal) => literal.text,
        Node::NoSubstitutionTemplateLiteral(literal) => literal.text,
        _ => return None,
    };
    Some(ModuleSpecifier {
        text: text.to_string(),
        pos: node_pos(nodes, Node::from(argument)),
        context,
        resolution_mode_override: ResolutionMode::None,
    })
}

/// Whether the file is an external module on syntax alone
/// (`isFileProbablyExternalModule`).
///
/// The `import.meta` arm is checked by walking the tree rather than by reading
/// `NodeFlagsPossiblyContainsImportMeta`, for the reason given in the module
/// docs: this port does not track source flags, and the walk gives the same
/// answer.
///
/// **That arm is currently unreachable.** This parser builds `import.meta` as a
/// `PropertyAccessExpression` over an `ImportKeyword` token rather than as a
/// `MetaProperty` (bd tsr-9or.4), so a file whose only module indicator is
/// `import.meta` reads as a script. It is written against the node upstream
/// produces rather than the one this parser produces, so that fixing the parser
/// fixes this too instead of breaking it. No `.trace.json` baseline reaches it.
#[must_use]
pub fn is_file_probably_external_module(file: &SourceFile<'_>) -> bool {
    for statement in file.statements {
        if is_an_external_module_indicator(*statement) {
            return true;
        }
    }
    contains_node(
        file,
        |node| matches!(node, Node::MetaProperty(meta) if meta.keyword_token.kind == SyntaxKind::ImportKeyword),
    )
}

/// Whether the file contains a JSX tag (`isFileModuleFromUsingJSXTag`).
///
/// Only consulted under `jsx: react-jsx`/`react-jsxdev`, where a tag alone makes
/// the file a module because the transform inserts an import into it.
#[must_use]
pub fn contains_jsx_tag(file: &SourceFile<'_>) -> bool {
    contains_node(file, |node| {
        matches!(
            node,
            Node::JsxOpeningElement(_)
                | Node::JsxSelfClosingElement(_)
                | Node::JsxFragment(_)
                | Node::JsxOpeningFragment(_)
        )
    })
}

/// `isAnExternalModuleIndicatorNode`.
fn is_an_external_module_indicator(statement: Statement<'_>) -> bool {
    match statement {
        Statement::ImportDeclaration(_)
        | Statement::ExportAssignment(_)
        | Statement::ExportDeclaration(_) => true,
        Statement::ImportEqualsDeclaration(node) => {
            matches!(
                node.module_reference,
                Some(tsr_ast::ModuleReference::ExternalModuleReference(_))
            ) || has_modifier(node.modifiers, SyntaxKind::ExportKeyword)
        }
        _ => modifiers_of(statement).is_some_and(|m| has_modifier(m, SyntaxKind::ExportKeyword)),
    }
}

/// Whether any node in the tree satisfies `predicate` (`findChildNode`,
/// reduced to the boolean its two callers want).
fn contains_node(file: &SourceFile<'_>, predicate: impl Fn(Node<'_>) -> bool) -> bool {
    let mut stack: Vec<Node<'_>> = vec![Node::from(file)];
    let mut children: Vec<Node<'_>> = Vec::new();
    while let Some(node) = stack.pop() {
        if predicate(node) {
            return true;
        }
        children.clear();
        push_children(node, &mut children);
        stack.extend(children.iter().copied());
    }
    false
}

/// `ImportAttributes.GetResolutionModeOverride`.
fn resolution_mode_override(attributes: Option<&ImportAttributes<'_>>) -> ResolutionMode {
    let Some(attributes) = attributes else { return ResolutionMode::None };
    // Exactly one attribute, named `resolution-mode`, valued `import` or
    // `require`. Anything else is a grammar error upstream reports elsewhere.
    let [attribute] = attributes.attributes else { return ResolutionMode::None };
    let name = match attribute.name {
        Some(tsr_ast::ImportAttributeName::StringLiteral(literal)) => literal.text,
        _ => return ResolutionMode::None,
    };
    if name != "resolution-mode" {
        return ResolutionMode::None;
    }
    match attribute.value.map(Node::from) {
        Some(Node::StringLiteral(literal)) if literal.text == "import" => ResolutionMode::ESNext,
        Some(Node::StringLiteral(literal)) if literal.text == "require" => ResolutionMode::CommonJS,
        _ => ResolutionMode::None,
    }
}

/// `IsExclusivelyTypeOnlyImportOrExport`, for an import declaration.
///
/// `import type` and `import defer` share one slot upstream
/// (`ImportClause.PhaseModifier`), so "type-only" is the `type` keyword
/// specifically, not merely "a modifier is present".
fn is_type_only_import(import_clause: Option<&tsr_ast::ImportClause<'_>>) -> bool {
    import_clause.is_some_and(|clause| {
        clause.phase_modifier.is_some_and(|token| token.kind == SyntaxKind::TypeKeyword)
    })
}

/// A registered node's kind.
fn node_kind(nodes: &NodeTable, node: Node<'_>) -> Option<SyntaxKind> {
    node.node_id().map(|id| nodes.kind(id))
}

fn has_declare_modifier(modifiers: &[ModifierLike<'_>]) -> bool {
    has_modifier(modifiers, SyntaxKind::DeclareKeyword)
}

fn has_modifier(modifiers: &[ModifierLike<'_>], kind: SyntaxKind) -> bool {
    modifiers
        .iter()
        .any(|modifier| matches!(modifier, ModifierLike::Token(token) if token.kind == kind))
}

/// The modifier list of a statement that has one.
///
/// Only the `export` test needs this, and only to decide module-ness, so the
/// list is deliberately the declarations that can carry `export` rather than
/// every node with a `modifiers` field.
fn modifiers_of(statement: Statement<'_>) -> Option<&[ModifierLike<'_>]> {
    Some(match statement {
        Statement::ClassDeclaration(node) => node.modifiers,
        Statement::EnumDeclaration(node) => node.modifiers,
        Statement::FunctionDeclaration(node) => node.modifiers,
        Statement::InterfaceDeclaration(node) => node.modifiers,
        Statement::ModuleDeclaration(node) => node.modifiers,
        Statement::TypeAliasDeclaration(node) => node.modifiers,
        Statement::VariableStatement(node) => node.modifiers,
        _ => return None,
    })
}

/// A node's start offset, used only to order the dynamic scan.
///
/// Spans live in [`tsr_ast::NodeTable`] rather than on the node
/// ([ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md)), so the walk
/// carries the table. An unregistered node cannot occur in parser output, but
/// sorting must be total, so it sorts last rather than panicking.
fn node_pos(nodes: &NodeTable, node: Node<'_>) -> u32 {
    node.node_id().map_or(u32::MAX, |id| nodes.span(id).start)
}

#[cfg(test)]
mod tests {
    use tsr_core::Arena;

    use super::*;
    use crate::{ParsedSourceFile, parse};

    fn collect<'a>(parsed: &'a ParsedSourceFile<'a>, is_js_file: bool) -> ExternalModuleReferences {
        collect_external_module_references(
            parsed.source_file,
            &parsed.nodes,
            CollectOptions {
                is_declaration_file: false,
                is_js_file,
                is_external_module: is_file_probably_external_module(parsed.source_file),
            },
        )
    }

    fn texts(references: &ExternalModuleReferences) -> Vec<(&str, SpecifierContext)> {
        references.imports.iter().map(|s| (s.text.as_str(), s.context)).collect()
    }

    #[test]
    fn every_syntax_that_names_a_module_is_collected_once_and_in_order() {
        let arena = Arena::new();
        let parsed = parse(
            &arena,
            "import \"a\";\n\
             export { x } from \"b\";\n\
             import c = require(\"c\");\n\
             const d = import(\"d\");\n\
             type E = import(\"e\").T;\n",
        );
        assert_eq!(
            texts(&collect(&parsed, false)),
            [
                ("a", SpecifierContext::ImportDeclaration),
                ("b", SpecifierContext::ExportDeclaration),
                ("c", SpecifierContext::ImportEquals),
                // Statement-level specifiers first, then the position-ordered
                // dynamic scan — which is upstream's order, not source order.
                ("d", SpecifierContext::ImportCall),
                ("e", SpecifierContext::ImportType),
            ]
        );
    }

    #[test]
    fn require_is_a_module_reference_only_in_javascript() {
        // `compiler/moduleResolutionWithRequire` is baselined with an *empty*
        // trace precisely because of this: a `.ts` file's `require()` is an
        // ordinary call to whatever `declare const require` names.
        let arena = Arena::new();
        let parsed = parse(&arena, "declare const require: any;\nconst a = require(\"x\");\n");
        assert!(collect(&parsed, false).imports.is_empty());
        assert_eq!(texts(&collect(&parsed, true)), [("x", SpecifierContext::RequireCall)]);
    }

    #[test]
    fn a_dynamic_import_of_a_non_literal_names_nothing() {
        let arena = Arena::new();
        let parsed = parse(&arena, "var v = import(String());\n");
        assert!(collect(&parsed, false).imports.is_empty());
    }

    #[test]
    fn import_equals_a_namespace_names_nothing() {
        let arena = Arena::new();
        let parsed = parse(&arena, "import M = N;\n");
        assert!(collect(&parsed, false).imports.is_empty());
    }

    #[test]
    fn declare_global_is_not_a_module_augmentation() {
        // The third empty-trace case. `declare global` has an *identifier*
        // name, so `GetExternalModuleName` returns nothing to resolve.
        let arena = Arena::new();
        let parsed = parse(&arena, "export {};\ndeclare global { var x: number; }\n");
        let references = collect(&parsed, false);
        assert!(references.imports.is_empty());
        assert!(references.module_augmentations.is_empty());
    }

    #[test]
    fn an_ambient_module_augments_a_module_and_declares_one_otherwise() {
        let arena = Arena::new();
        // With an `export`, the file is an external module, so the block
        // augments `"m"` — which the loader resolves.
        let augmenting = parse(&arena, "export {};\ndeclare module \"m\" { }\n");
        let references = collect(&augmenting, false);
        assert_eq!(
            references.module_augmentations.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
            ["m"]
        );
        assert!(references.ambient_module_names.is_empty());

        // Without one, it *declares* `"m"`, which resolves nothing.
        let declaring = parse(&arena, "declare module \"m\" { }\n");
        let references = collect(&declaring, false);
        assert!(references.module_augmentations.is_empty());
        assert_eq!(references.ambient_module_names, ["m"]);
    }

    #[test]
    fn a_relative_import_inside_an_ambient_module_is_not_collected() {
        // TypeScript 1.0 spec 12.1.6: an ambient external module may only name
        // top-level modules.
        let arena = Arena::new();
        let parsed =
            parse(&arena, "declare module \"m\" { import \"./relative\"; import \"bare\"; }\n");
        assert_eq!(
            texts(&collect(&parsed, false)),
            [("bare", SpecifierContext::ImportDeclaration)]
        );
    }

    #[test]
    fn a_resolution_mode_attribute_is_read_only_for_a_type_only_import() {
        let arena = Arena::new();
        let type_only = parse(
            &arena,
            "import type { A } from \"a\" with { \"resolution-mode\": \"require\" };\n",
        );
        assert_eq!(
            collect(&type_only, false).imports[0].resolution_mode_override,
            ResolutionMode::CommonJS
        );
        // A value import with the same attribute: upstream gates the override
        // on `IsExclusivelyTypeOnlyImportOrExport`, so it does not apply.
        let value =
            parse(&arena, "import { A } from \"a\" with { \"resolution-mode\": \"require\" };\n");
        assert_eq!(
            collect(&value, false).imports[0].resolution_mode_override,
            ResolutionMode::None
        );
    }

    #[test]
    fn a_file_is_an_external_module_when_it_imports_exports_or_names_import_meta() {
        let arena = Arena::new();
        for source in ["import \"a\";", "export {};", "export = 1;"] {
            let parsed = parse(&arena, source);
            assert!(
                is_file_probably_external_module(parsed.source_file),
                "expected an external module: {source}"
            );
        }
        let parsed = parse(&arena, "const x = 1;");
        assert!(!is_file_probably_external_module(parsed.source_file));

        // Known gap, asserted so it is visible rather than merely absent: this
        // parser does not build a `MetaProperty` for `import.meta`, so the
        // `getImportMetaIfNecessary` arm never fires (bd tsr-9or.4). When the
        // parser is fixed this assertion flips and this test must too.
        let import_meta = parse(&arena, "const u = import.meta.url;");
        assert!(!is_file_probably_external_module(import_meta.source_file));
    }
}
