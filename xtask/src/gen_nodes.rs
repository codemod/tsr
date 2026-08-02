//! Generates `crates/tsr-ast/src/generated/nodes.rs` and `.../alias.rs`.
//!
//! Two artifacts, both derived from `ast.json`:
//!
//! - **Node structs** — one per concrete node definition. Fields are the node's
//!   own members plus everything it inherits from its bases, in upstream order.
//! - **Alias enums** — one per union (`Expression`, `Statement`, `TypeNode`, …).
//!   These are where the idiomatic-Rust bet pays off: upstream models unions as
//!   an untyped `*ast.Node` plus a runtime kind check, and we get exhaustive
//!   `match` instead.
//!
//! Fields marked `goOnly` upstream — `Symbol`, `Locals`, `FlowNode`,
//! `NextContainer`, `facts` — are deliberately **not** generated. Those are
//! binder and checker state, and under PLAN.md §3.2 they live in id-keyed side
//! tables rather than on the node.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use anyhow::{Context, Result, bail};
use convert_case::{Case, Casing};

use crate::ast_json::{AstDefinition, Field, NodeAlias, TypeRef};

/// A field resolved from a member and/or its base declaration.
struct ResolvedField {
    /// Upstream name.
    name: String,
    /// Rust field name.
    rust_name: String,
    /// Rust type, fully formed including `Option` and list wrappers.
    rust_type: String,
    /// Doc line describing the upstream type.
    doc: String,
}

/// Primitive and flag types that are not node references.
fn scalar_type(name: &str) -> Option<&'static str> {
    match name {
        "string" => Some("&'a str"),
        "bool" => Some("bool"),
        "NodeFlags" => Some("crate::NodeFlags"),
        "TokenFlags" => Some("crate::TokenFlags"),
        "ModifierFlags" => Some("crate::ModifierFlags"),
        "TKind" => Some("SyntaxKind"),
        _ => None,
    }
}

/// Go-only types that never reach the Rust AST.
fn is_go_only_type(name: &str) -> bool {
    name.starts_with('*') || name.starts_with("atomic.") || name == "SymbolTable"
}

/// Whether `name` denotes a token: either an instantiation of the generic `Token`
/// node (`AsteriskToken`, `QuestionToken`, …) or a named group of token kinds
/// (`BinaryOperatorToken`, `ImportPhaseModifierSyntaxKind`).
fn is_token_alias(ast: &AstDefinition, name: &str) -> bool {
    ast.kinds.aliases.contains_key(name)
        || ast
            .nodes
            .definitions
            .get("Token")
            .is_some_and(|t| t.instantiation_aliases.contains_key(name))
}

/// Escape Rust keywords for use as field names.
fn escape_field(name: &str) -> String {
    let snake = name.to_case(Case::Snake);
    match snake.as_str() {
        "type" | "ref" | "fn" | "in" | "for" | "else" | "box" | "as" | "if" | "loop" | "match"
        | "move" | "mut" | "static" | "super" | "true" | "false" | "let" | "const" | "impl"
        | "self" | "where" | "while" | "yield" | "default" | "do" | "enum" | "extern"
        | "abstract" => {
            format!("r#{snake}")
        }
        _ => snake,
    }
}

/// Compute the transitive field set of a base.
fn base_fields(ast: &AstDefinition, base: &str, out: &mut Vec<(String, Field)>) {
    let Some(b) = ast.bases.get(base) else { return };
    for parent in &b.extends {
        base_fields(ast, parent, out);
    }
    for (name, field) in &b.fields {
        if !out.iter().any(|(n, _)| n == name) {
            out.push((
                name.clone(),
                Field {
                    r#type: field.r#type.clone(),
                    optional: field.optional,
                    list: field.list.clone(),
                    go_only: field.go_only,
                    no_go: field.no_go,
                },
            ));
        }
    }
}

/// Whether `def` transitively extends `base`.
fn extends_base(ast: &AstDefinition, def_name: &str, base: &str) -> bool {
    let Some(def) = ast.nodes.definitions.get(def_name) else { return false };
    def.extends.iter().any(|b| base_reaches(ast, b, base))
}

fn base_reaches(ast: &AstDefinition, from: &str, target: &str) -> bool {
    if from == target {
        return true;
    }
    ast.bases.get(from).is_some_and(|b| b.extends.iter().any(|p| base_reaches(ast, p, target)))
}

/// Resolve an alias to the set of concrete node definitions it covers.
fn resolve_alias(ast: &AstDefinition, name: &str, seen: &mut BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if !seen.insert(name.to_string()) {
        return out;
    }
    match ast.nodes.aliases.get(name) {
        Some(NodeAlias::Base { base }) => {
            for def_name in ast.nodes.definitions.keys() {
                if extends_base(ast, def_name, base) {
                    out.insert(def_name.clone());
                }
            }
        }
        Some(NodeAlias::Members(members)) => {
            for m in members {
                if ast.nodes.definitions.contains_key(m) {
                    out.insert(m.clone());
                } else {
                    out.extend(resolve_alias(ast, m, seen));
                }
            }
        }
        None => {
            if ast.nodes.definitions.contains_key(name) {
                out.insert(name.to_string());
            }
        }
    }
    out
}

/// Map an upstream type reference to a Rust type.
fn rust_type(
    ast: &AstDefinition,
    ty: &TypeRef,
    optional: bool,
    list: Option<&str>,
) -> Result<Option<String>> {
    let base = match ty {
        // An inline union of token kinds collapses to a token carrying its kind;
        // the permitted set is recorded in the doc comment instead of the type.
        TypeRef::KindUnion(_) => "&'a Token<'a>".to_string(),
        TypeRef::Named(name) => {
            if is_go_only_type(name) {
                return Ok(None);
            }
            if let Some(scalar) = scalar_type(name) {
                scalar.to_string()
            } else if ast.nodes.definitions.contains_key(name) {
                format!("&'a {name}<'a>")
            } else if ast.nodes.aliases.contains_key(name) {
                format!("{name}<'a>")
            } else if is_token_alias(ast, name) {
                // `AsteriskToken`, `QuestionToken`, `BinaryOperatorToken`, … are
                // instantiations of the generic `Token` node, or named groups of
                // token kinds. Both are represented by a `Token` carrying its kind.
                "&'a Token<'a>".to_string()
            } else if name == "Node" || name == "any" {
                // `Node` is genuinely "any node"; upstream's single `any` is
                // `SyntheticExpression.Type`, a checker-produced type placeholder
                // with no syntactic shape.
                "Node<'a>".to_string()
            } else {
                // Nothing should reach here. Fail loudly rather than silently
                // degrading a field to an opaque node, which is how a conformance
                // gap would otherwise slip in unnoticed.
                bail!("unmapped AST type {name:?}; teach gen_nodes::rust_type about it");
            }
        }
    };

    let wrapped = if list.is_some() { format!("&'a [{base}]") } else { base };
    Ok(Some(if optional && list.is_none() { format!("Option<{wrapped}>") } else { wrapped }))
}

/// Build the ordered field list for a node definition.
fn resolve_fields(ast: &AstDefinition, def_name: &str) -> Result<Vec<ResolvedField>> {
    let def = ast
        .nodes
        .definitions
        .get(def_name)
        .with_context(|| format!("unknown node definition {def_name}"))?;

    let mut inherited: Vec<(String, Field)> = Vec::new();
    for base in &def.extends {
        base_fields(ast, base, &mut inherited);
    }
    let inherited_map: BTreeMap<&str, &Field> =
        inherited.iter().map(|(n, f)| (n.as_str(), f)).collect();

    let mut fields = Vec::new();
    let mut emitted = BTreeSet::new();

    let push = |name: &str,
                ty: Option<&TypeRef>,
                optional: bool,
                list: Option<&str>,
                fields: &mut Vec<ResolvedField>,
                emitted: &mut BTreeSet<String>|
     -> Result<()> {
        // `Flags` lives on the node envelope, not on each node struct.
        if name == "Flags" || name == "modifierFlags" || emitted.contains(name) {
            return Ok(());
        }
        let Some(ty) = ty else { return Ok(()) };
        let Some(rust) = rust_type(ast, ty, optional, list)? else { return Ok(()) };
        let doc = match ty {
            TypeRef::Named(n) => n.clone(),
            TypeRef::KindUnion(kinds) => kinds.join(" | "),
        };
        emitted.insert(name.to_string());
        fields.push(ResolvedField {
            name: name.to_string(),
            rust_name: escape_field(name),
            rust_type: rust,
            doc,
        });
        Ok(())
    };

    // Members define order, including inherited ones.
    for member in &def.members {
        if member.is_skipped() {
            continue;
        }
        let base = inherited_map.get(member.name.as_str()).copied();
        if let Some(b) = base {
            if b.is_skipped() && member.r#type.is_none() {
                continue;
            }
        }
        let ty = member.r#type.as_ref().or_else(|| base.and_then(|b| b.r#type.as_ref()));
        let optional =
            member.optional || base.is_some_and(|b| b.optional && member.r#type.is_none());
        let list = member.list.as_deref().or_else(|| base.and_then(|b| b.list.as_deref()));
        push(&member.name, ty, optional, list, &mut fields, &mut emitted)?;
    }

    // Base fields the members list did not mention are still present upstream via
    // struct embedding, so append them.
    for (name, field) in &inherited {
        if field.is_skipped() {
            continue;
        }
        push(
            name,
            field.r#type.as_ref(),
            field.optional,
            field.list.as_deref(),
            &mut fields,
            &mut emitted,
        )?;
    }

    Ok(fields)
}

/// Generate the node structs.
pub fn generate_nodes(ast: &AstDefinition) -> Result<String> {
    let mut out = String::with_capacity(256 * 1024);
    out.push_str(
        "//! Concrete AST node types.\n\
         //!\n\
         //! @generated by `cargo xtask codegen` from\n\
         //! `vendor/typescript-go/_scripts/ast.json`. Do not edit by hand.\n\
         //!\n\
         //! Corresponds to typescript-go's `internal/ast/ast_generated.go`.\n\
         //!\n\
         //! Fields upstream marks `goOnly` — `Symbol`, `Locals`, `FlowNode`,\n\
         //! `NextContainer`, `facts` — are intentionally absent. They are binder and\n\
         //! checker state, and live in id-keyed side tables here (PLAN.md §3.2).\n\n\
         #![allow(clippy::struct_excessive_bools, clippy::doc_markdown)]\n\n\
         use super::alias::*;\n\
         use crate::{SyntaxKind, Token};\n\n",
    );

    for name in ast.nodes.definitions.keys() {
        if name == "Token" {
            // Hand-written in `lib.rs`: upstream's generic `Token[TKind]` has no
            // distinct shape per instantiation, so it collapses to one type.
            continue;
        }
        let fields = resolve_fields(ast, name)?;
        writeln!(
            out,
            "/// The `{name}` node.\n\
             ///\n\
             /// Corresponds to typescript-go's `ast.{name}`.\n\
             #[derive(Debug)]\n\
             pub struct {name}<'a> {{"
        )?;
        // Nodes such as `KeywordExpression` carry only a kind, so nothing in the
        // struct mentions `'a`. Keep the parameter for uniformity — every node is
        // written `Foo<'a>` at use sites — and anchor it with a marker.
        if !fields.iter().any(|f| f.rust_type.contains("'a")) {
            out.push_str(
                "    /// No field borrows from the arena; anchors the `'a` parameter\n    \
                 /// so every node type is spelled uniformly as `Node<'a>`.\n    \
                 pub _marker: std::marker::PhantomData<&'a ()>,\n",
            );
        }
        for f in &fields {
            writeln!(
                out,
                "    /// `{}`: `{}`\n    pub {}: {},",
                f.name, f.doc, f.rust_name, f.rust_type
            )?;
        }
        out.push_str("}\n\n");
    }

    Ok(out)
}

/// Generate the alias union enums plus the top-level `Node` enum.
pub fn generate_aliases(ast: &AstDefinition) -> Result<String> {
    let mut out = String::with_capacity(256 * 1024);
    out.push_str(
        "//! Union types over AST nodes.\n\
         //!\n\
         //! @generated by `cargo xtask codegen` from\n\
         //! `vendor/typescript-go/_scripts/ast.json`. Do not edit by hand.\n\
         //!\n\
         //! Upstream models these as an untyped `*ast.Node` plus a runtime kind\n\
         //! check. Modelling them as Rust enums is the central idiomatic-Rust bet\n\
         //! of this port: a `match` over `Expression` is exhaustive, so a new node\n\
         //! kind upstream becomes a compile error here rather than a silent\n\
         //! fallthrough.\n\n\
         #![allow(clippy::doc_markdown, clippy::large_enum_variant)]\n\n\
         use super::nodes::*;\n\
         use crate::Token;\n\n",
    );

    // The universal node union.
    out.push_str(
        "/// Any AST node.\n\
         ///\n\
         /// Corresponds to typescript-go's `*ast.Node`.\n\
         #[derive(Debug, Clone, Copy)]\n\
         pub enum Node<'a> {\n",
    );
    for name in ast.nodes.definitions.keys() {
        writeln!(out, "    /// See [`{name}`].\n    {name}(&'a {name}<'a>),")?;
    }
    out.push_str("}\n\n");

    for alias_name in ast.nodes.aliases.keys() {
        let mut seen = BTreeSet::new();
        let members = resolve_alias(ast, alias_name, &mut seen);
        if members.is_empty() {
            continue;
        }
        writeln!(
            out,
            "/// The `{alias_name}` union.\n\
             ///\n\
             /// Corresponds to typescript-go's `ast.{alias_name}`.\n\
             #[derive(Debug, Clone, Copy)]\n\
             pub enum {alias_name}<'a> {{"
        )?;
        for m in &members {
            writeln!(out, "    /// See [`{m}`].\n    {m}(&'a {m}<'a>),")?;
        }
        out.push_str("}\n\n");

        // Widening into the universal node union is needed constantly by the
        // visitor and by the language service.
        writeln!(
            out,
            "impl<'a> From<{alias_name}<'a>> for Node<'a> {{\n    \
             fn from(value: {alias_name}<'a>) -> Self {{\n        \
             match value {{"
        )?;
        for m in &members {
            writeln!(out, "            {alias_name}::{m}(n) => Node::{m}(n),")?;
        }
        out.push_str("        }\n    }\n}\n\n");
    }

    Ok(out)
}

/// Generate the `Visit` trait and its `walk_*` free functions.
///
/// One `visit_*` method per node type, defaulting to the matching `walk_*`, which
/// descends into children. Overriding a method and calling `walk_*` from it is the
/// standard pattern; not calling it prunes the subtree.
///
/// Generating this is not optional at 192 node types: hand-maintaining a visitor
/// across a moving upstream is how traversal silently misses new syntax. oxc
/// reaches the same conclusion — its `tasks/ast_tools` is ~20k LOC for the same
/// reason.
pub fn generate_visit(ast: &AstDefinition) -> Result<String> {
    let mut out = String::with_capacity(256 * 1024);
    out.push_str(
        "//! AST traversal.\n\
         //!\n\
         //! @generated by `cargo xtask codegen` from\n\
         //! `vendor/typescript-go/_scripts/ast.json`. Do not edit by hand.\n\
         //!\n\
         //! Corresponds to typescript-go's `ForEachChild` / `NodeVisitor`\n\
         //! (`internal/ast/visitor.go`).\n\n\
         #![allow(unused_variables, clippy::too_many_lines)]\n\n\
         use super::{alias::*, nodes::*};\n\
         use crate::Token;\n\n",
    );

    // ---- the trait ------------------------------------------------------
    out.push_str(
        "/// Walks an AST.\n\
         ///\n\
         /// Every method defaults to the matching `walk_*` function, so an impl only\n\
         /// overrides what it cares about. Call the `walk_*` function from an override\n\
         /// to continue into children; omit it to prune the subtree.\n\
         pub trait Visit<'a>: Sized {\n\
         \x20   /// Visit any node, dispatching on its variant.\n\
         \x20   fn visit_node(&mut self, node: Node<'a>) {\n\
         \x20       walk_node(self, node);\n\
         \x20   }\n\n",
    );
    for name in ast.nodes.definitions.keys() {
        let snake = name.to_case(Case::Snake);
        writeln!(
            out,
            "    /// Visit a [`{name}`].\n    \
             fn visit_{snake}(&mut self, node: &'a {name}<'a>) {{\n        \
             walk_{snake}(self, node);\n    }}\n"
        )?;
    }
    out.push_str("}\n\n");

    // ---- dispatch -------------------------------------------------------
    out.push_str(
        "/// Dispatch to the visit method matching `node`'s variant.\n\
         pub fn walk_node<'a, V: Visit<'a>>(visitor: &mut V, node: Node<'a>) {\n    \
         match node {\n",
    );
    for name in ast.nodes.definitions.keys() {
        let snake = name.to_case(Case::Snake);
        writeln!(out, "        Node::{name}(n) => visitor.visit_{snake}(n),")?;
    }
    out.push_str("    }\n}\n\n");

    // ---- per-node walkers ----------------------------------------------
    for name in ast.nodes.definitions.keys() {
        let snake = name.to_case(Case::Snake);
        let fields = if name == "Token" { Vec::new() } else { resolve_fields(ast, name)? };

        writeln!(
            out,
            "/// Walk the children of a [`{name}`].\n\
             pub fn walk_{snake}<'a, V: Visit<'a>>(visitor: &mut V, node: &'a {name}<'a>) {{"
        )?;

        let mut emitted_body = false;
        for f in &fields {
            let Some(call) = child_visit_call(ast, f) else { continue };
            out.push_str(&call);
            emitted_body = true;
        }
        if !emitted_body {
            out.push_str("    // No child nodes.\n");
        }
        out.push_str("}\n\n");
    }

    Ok(out)
}

/// Emit the traversal statement for one field, or `None` for non-node fields.
fn child_visit_call(ast: &AstDefinition, field: &ResolvedField) -> Option<String> {
    let ty = &field.rust_type;
    let name = &field.rust_name;

    // Scalars and tokens carry no children worth descending into.
    if !ty.contains("'a") || ty.contains("str") {
        return None;
    }
    let inner = ty
        .trim_start_matches("Option<")
        .trim_end_matches('>')
        .trim_start_matches("&'a [")
        .trim_end_matches(']')
        .trim_start_matches("&'a ");
    let base = inner.split('<').next().unwrap_or(inner);
    if base == "Token" {
        return None;
    }

    let is_list = ty.contains("&'a [");
    let is_option = ty.starts_with("Option<");
    let is_alias = ast.nodes.aliases.contains_key(base) || base == "Node";
    let is_node = ast.nodes.definitions.contains_key(base);
    if !is_alias && !is_node {
        return None;
    }

    // Aliases widen into `Node` for dispatch; concrete nodes call their visitor
    // method directly, which keeps the common case one call deep.
    let visit_one = |expr: &str| -> String {
        if is_alias {
            if base == "Node" {
                format!("visitor.visit_node({expr});")
            } else {
                format!("visitor.visit_node(Node::from({expr}));")
            }
        } else {
            format!("visitor.visit_{}({expr});", base.to_case(Case::Snake))
        }
    };

    Some(match (is_list, is_option) {
        // Iterating a `&[&Foo]` yields `&&Foo`. The concrete-node call auto-derefs,
        // but `Node::from` takes the value, hence the different binding.
        (true, _) => {
            let bound = if is_alias { "*child" } else { "child" };
            format!("    for child in node.{name} {{\n        {}\n    }}\n", visit_one(bound))
        }
        (false, true) => format!(
            "    if let Some(child) = node.{name} {{\n        {}\n    }}\n",
            visit_one("child")
        ),
        (false, false) => format!("    {}\n", visit_one(&format!("node.{name}"))),
    })
}
