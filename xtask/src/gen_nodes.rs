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

/// TS-side type names with no node of their own.
///
/// `JsxTagNamePropertyAccess` is a documentation name for a
/// `PropertyAccessExpression` whose chain is a JSX tag; upstream's Go widens the
/// whole union to `Node` and loses the distinction entirely.
fn alias_only_name(name: &str) -> Option<&'static str> {
    match name {
        "JsxTagNamePropertyAccess" => Some("PropertyAccessExpression"),
        _ => None,
    }
}

/// The node a name denotes when it is an *instantiation* of a generic node.
///
/// `ThisExpression` is `KeywordExpression` carrying `ThisKeyword`, and
/// `AsteriskToken` is `Token` carrying `AsteriskToken`. Upstream records these
/// under each definition's `instantiationAliases`.
fn instantiation_of(ast: &AstDefinition, name: &str) -> Option<String> {
    if let Some((node, _)) =
        ast.nodes.definitions.iter().find(|(_, def)| def.instantiation_aliases.contains_key(name))
    {
        return Some(node.clone());
    }

    // A node whose `Kind` member is an inline union covers each of those kinds:
    // `ObjectBindingPattern` and `ArrayBindingPattern` are both `BindingPattern`.
    let wanted = format!("SyntaxKind.{name}");
    ast.nodes
        .definitions
        .iter()
        .find(|(_, def)| {
            def.members.iter().any(|member| {
                member.name == "Kind"
                    && matches!(
                        member.r#type.as_ref(),
                        Some(TypeRef::KindUnion(kinds)) if kinds.contains(&wanted)
                    )
            })
        })
        .map(|(node, _)| node.clone())
}

/// Resolve an alias to the set of concrete node definitions it covers.
///
/// **Bails on anything it does not recognise.** Four separate silent drops have
/// been found in this generator — `{name, comment}` kind elements, kind-alias
/// members, inherited optionality, and generic instantiations — each shipping a
/// quietly wrong AST. A member that cannot be resolved is now a hard error, not a
/// skipped entry.
fn resolve_alias(
    ast: &AstDefinition,
    name: &str,
    seen: &mut BTreeSet<String>,
) -> Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    if !seen.insert(name.to_string()) {
        return Ok(out);
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
                } else if ast.kinds.aliases.contains_key(m) {
                    // A set of *token kinds*, not a node — `Modifier =
                    // ["ModifierSyntaxKind"]`. Admits a `Token` of those kinds.
                    out.insert("Token".to_string());
                } else if let Some(node) = alias_only_name(m) {
                    out.insert(node.to_string());
                } else if let Some(node) = instantiation_of(ast, m) {
                    out.insert(node);
                } else if ast.bases.contains_key(m) {
                    // A member naming a *base* means every node extending it:
                    // `IncrementExpression = ["UpdateExpressionBase"]`.
                    for def_name in ast.nodes.definitions.keys() {
                        if extends_base(ast, def_name, m) {
                            out.insert(def_name.clone());
                        }
                    }
                } else if ast.nodes.aliases.contains_key(m) {
                    out.extend(resolve_alias(ast, m, seen)?);
                } else {
                    bail!(
                        "alias {name}: member {m:?} resolves to nothing — teach \
                         gen_nodes::resolve_alias about it rather than dropping it"
                    );
                }
            }
        }
        None => {
            if ast.nodes.definitions.contains_key(name) {
                out.insert(name.to_string());
            }
        }
    }
    Ok(out)
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

/// Fields upstream's generated Go declares as nullable pointers.
///
/// `ast.json`'s `optional` flag describes TypeScript's *public API* — whether the
/// property is written `foo?:` in the `.d.ts`. It does not describe whether the
/// field can be absent at runtime, because Go expresses that with a pointer and
/// every node-typed field is a pointer.
///
/// Trusting `optional` alone made fields mandatory that nothing supplies:
/// `PropertyAssignment` came out requiring a `TypeNode`, which no property
/// assignment has. So nullability is read from `ast_generated.go`, which is the
/// authority on what the compiler actually permits — the same principle as
/// [ADR-0006](../../docs/adr/0006-conformance-oracle.md).
#[derive(Debug, Default)]
pub struct GoNullability {
    /// `(struct name, field name)` pairs declared as `*T`.
    pointers: BTreeSet<(String, String)>,
}

impl GoNullability {
    /// Parse struct field declarations out of upstream's generated Go.
    pub fn parse(source: &str) -> Self {
        let mut pointers = BTreeSet::new();
        let mut current: Option<String> = None;
        for line in source.lines() {
            if let Some(rest) = line.strip_prefix("type ") {
                current = rest
                    .split_whitespace()
                    .next()
                    .filter(|_| rest.contains("struct {"))
                    .map(str::to_string);
                continue;
            }
            if line == "}" {
                current = None;
                continue;
            }
            let Some(struct_name) = current.as_deref() else { continue };
            let trimmed = line.trim();
            let mut parts = trimmed.split_whitespace();
            let (Some(field), Some(ty)) = (parts.next(), parts.next()) else { continue };
            if ty.starts_with('*') {
                pointers.insert((struct_name.to_string(), field.to_string()));
            }
        }
        Self { pointers }
    }

    /// Whether `node.field` is a nullable pointer upstream.
    fn is_nullable(&self, node: &str, field: &str) -> bool {
        self.pointers.contains(&(node.to_string(), field.to_string()))
    }
}

/// Build the ordered field list for a node definition.
fn resolve_fields(
    ast: &AstDefinition,
    nullability: &GoNullability,
    def_name: &str,
) -> Result<Vec<ResolvedField>> {
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
        // Optionality is inherited even when the member overrides the *type*.
        // Requiring it back would make `PropertyAssignment.type` mandatory, which
        // no real property assignment supplies.
        let optional = member.optional
            || base.is_some_and(|b| b.optional)
            || nullability.is_nullable(def_name, &member.name);
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
pub fn generate_nodes(ast: &AstDefinition, nullability: &GoNullability) -> Result<String> {
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
         #![allow(\n\
         \x20   clippy::struct_excessive_bools,\n\
         \x20   clippy::doc_markdown,\n\
         \x20   // Node constructors take one parameter per syntax child; some nodes\n\
         \x20   // genuinely have nine. Splitting them would obscure the shape.\n\
         \x20   clippy::too_many_arguments,\n\
         \x20   // `Foo<'a>` is written uniformly even where the lifetime is elidable.\n\
         \x20   clippy::needless_lifetimes,\n\
         \x20   // A node with no children still gets `new()`; `Default` would imply\n\
         \x20   // these are meaningful standalone values, which they are not.\n\
         \x20   clippy::new_without_default,\n\
         )]\n\n\
         use std::cell::Cell;\n\n\
         use super::alias::*;\n\
         use crate::{NodeId, SyntaxKind, Token};\n\n",
    );

    for name in ast.nodes.definitions.keys() {
        if name == "Token" {
            // Hand-written in `lib.rs`: upstream's generic `Token[TKind]` has no
            // distinct shape per instantiation, so it collapses to one type.
            continue;
        }
        let fields = resolve_fields(ast, nullability, name)?;
        writeln!(
            out,
            "/// The `{name}` node.\n\
             ///\n\
             /// Corresponds to typescript-go's `ast.{name}`.\n\
             #[derive(Debug)]\n\
             pub struct {name}<'a> {{\n    \
             /// Key into the side tables holding this node's kind, span, and parent.\n    \
             ///\n    \
             /// `None` until the parser registers the node. A `Cell` so registration\n    \
             /// does not need `&mut` on a tree the parser is still building.\n    \
             pub node_id: Cell<Option<NodeId>>,"
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

        // A constructor per node, so callers never spell out `node_id`. Without
        // this the parser would repeat the same placeholder at ~200 sites.
        let params = fields
            .iter()
            .map(|f| format!("{}: {}", f.rust_name, f.rust_type))
            .collect::<Vec<_>>()
            .join(", ");
        let inits = fields.iter().map(|f| f.rust_name.clone()).collect::<Vec<_>>().join(", ");
        let marker = if fields.iter().any(|f| f.rust_type.contains("'a")) {
            String::new()
        } else {
            ", _marker: std::marker::PhantomData".to_string()
        };
        // A node with no arena-borrowing fields does not use `'a` in its impl, and
        // spelling it would be an elidable lifetime.
        let impl_header = if marker.is_empty() {
            format!("impl<'a> {name}<'a>")
        } else {
            format!("impl {name}<'_>")
        };
        writeln!(
            out,
            "{impl_header} {{\n    \
             /// Construct a `{name}`, unregistered.\n    \
             ///\n    \
             /// [`Self::node_id`] stays `None` until the parser records the node's\n    \
             /// kind and span in the side tables.\n    \
             #[must_use]\n    \
             pub fn new({params}) -> Self {{\n        \
             Self {{ node_id: Cell::new(None){}{} }}\n    }}\n}}\n",
            if inits.is_empty() { String::new() } else { format!(", {inits}") },
            marker,
        )?;

        // Lets the parser register any node generically.
        writeln!(
            out,
            "impl crate::HasNodeId for {name}<'_> {{\n    \
             fn set_node_id(&self, id: NodeId) {{\n        \
             self.node_id.set(Some(id));\n    }}\n\n    \
             fn node_id(&self) -> Option<NodeId> {{\n        \
             self.node_id.get()\n    }}\n}}\n"
        )?;
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

    // Every node carries a `NodeId`, but reaching it through the union needs a
    // match over all variants — the one thing a hand-written impl would rot on
    // the next time `ast.json` grows a node. Anything keyed by node identity (the
    // side tables, JSDoc attachment, the binder's symbol map) goes through here.
    out.push_str(
        "impl Node<'_> {\n             /// The id assigned when the parser registered this node.\n             ///\n             /// `None` only for a node that has not been registered, which the parser\n             /// does at construction; in a finished tree this is always `Some`.\n             #[must_use]\n             pub fn node_id(&self) -> Option<crate::NodeId> {\n                 use crate::HasNodeId as _;\n                 match self {\n",
    );
    for name in ast.nodes.definitions.keys() {
        writeln!(out, "            Node::{name}(n) => n.node_id(),")?;
    }
    out.push_str("        }\n    }\n}\n\n");

    let mut alias_members: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for alias_name in ast.nodes.aliases.keys() {
        let mut seen = BTreeSet::new();
        let members = resolve_alias(ast, alias_name, &mut seen)?;
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

        // Reaching a node's id without widening to `Node` first. `Node::node_id`
        // is a match over all 192 variants and shows up in profiles at ~1.8%;
        // an alias match is a handful of arms, and the caller usually has the
        // narrow type already.
        writeln!(
            out,
            "impl {alias_name}<'_> {{\n    \
             /// The id assigned when the parser registered this node.\n    \
             #[must_use]\n    \
             pub fn node_id(&self) -> Option<crate::NodeId> {{\n        \
             use crate::HasNodeId as _;\n        \
             match self {{"
        )?;
        for m in &members {
            writeln!(out, "            {alias_name}::{m}(n) => n.node_id(),")?;
        }
        out.push_str("        }\n    }\n}\n\n");

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

        // Narrowing from the universal union. Fallible, because a `Node` may hold
        // a variant outside this alias.
        writeln!(
            out,
            "impl<'a> TryFrom<Node<'a>> for {alias_name}<'a> {{\n    \
             type Error = Node<'a>;\n\n    \
             /// Returns the original node as the error when it is not a\n    \
             /// `{alias_name}`, so callers can recover it without a second match.\n    \
             fn try_from(value: Node<'a>) -> Result<Self, Self::Error> {{\n        \
             match value {{"
        )?;
        for m in &members {
            writeln!(out, "            Node::{m}(n) => Ok({alias_name}::{m}(n)),")?;
        }
        out.push_str("            other => Err(other),\n        }\n    }\n}\n\n");

        alias_members.insert(alias_name.clone(), members);
    }

    // Widening between aliases, where one union's members are a subset of
    // another's — `Expression` into `ForInitializer`, say. Delegating through
    // `Node` keeps this to one small impl per pair instead of re-emitting a full
    // match, which at 280 pairs would be tens of thousands of lines.
    let mut pairs = 0usize;
    for (narrow, narrow_members) in &alias_members {
        for (wide, wide_members) in &alias_members {
            if narrow == wide || narrow_members.is_empty() || wide == "Node" {
                continue;
            }
            if !narrow_members.is_subset(wide_members) || narrow_members == wide_members {
                continue;
            }
            writeln!(
                out,
                "impl<'a> From<{narrow}<'a>> for {wide}<'a> {{\n    \
                 /// Infallible: every `{narrow}` variant is also a `{wide}` variant.\n    \
                 fn from(value: {narrow}<'a>) -> Self {{\n        \
                 Self::try_from(Node::from(value))\n            \
                 .unwrap_or_else(|_| unreachable!(\"{narrow} is a subset of {wide}\"))\n    }}\n}}\n"
            )?;
            pairs += 1;
        }
    }
    let _ = pairs;

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
pub fn generate_visit(ast: &AstDefinition, nullability: &GoNullability) -> Result<String> {
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

    // Widening a concrete node reference into the union. `Node`'s variants hold
    // `&'a T`, so `Node::from(&foo)` needs one impl per node type — without them
    // any generic function holding a `&'a T` has no way to reach `Node`, which is
    // what `Parser::finish_node` needs to record parents as it builds.
    for name in ast.nodes.definitions.keys() {
        writeln!(
            out,
            "impl<'a> From<&'a {name}<'a>> for Node<'a> {{\n    \
             fn from(value: &'a {name}<'a>) -> Self {{\n        \
             Node::{name}(value)\n    }}\n}}\n"
        )?;
    }

    // ---- immediate children --------------------------------------------
    //
    // A `Visit` impl cannot collect immediate children: the walkers call *typed*
    // methods (`visit_identifier`) for concretely-typed fields, so a visitor that
    // overrides only `visit_node` silently misses them — and the typed defaults
    // recurse, so it also does not stop at one level. Anything that needs "the
    // children of this node, as `Node`" needs a separate function, and it must be
    // generated for the same reason the walkers are.
    out.push_str(
        "/// Append `node`'s immediate children to `out`, in source order.\n\
         ///\n\
         /// Does not recurse. This is the primitive for an iterative tree walk —\n\
         /// which is what a walk over parser output has to be, since tree depth is\n\
         /// a function of the source and a recursive walk over hostile input\n\
         /// overflows the stack.\n\
         pub fn push_children<'a>(node: Node<'a>, out: &mut Vec<Node<'a>>) {\n    \
         match node {\n",
    );
    let mut childless: Vec<String> = Vec::new();
    for name in ast.nodes.definitions.keys() {
        let fields =
            if name == "Token" { Vec::new() } else { resolve_fields(ast, nullability, name)? };
        let pushes: Vec<String> =
            fields.iter().filter_map(|field| child_push_call(ast, field)).collect();
        if pushes.is_empty() {
            childless.push(name.clone());
        } else {
            writeln!(out, "        Node::{name}(n) => {{")?;
            for push in pushes {
                out.push_str(&push);
            }
            out.push_str("        }\n");
        }
    }
    if !childless.is_empty() {
        // Leaves — identifiers, literals, keyword types — share one arm; sixty
        // identical `=> {}` arms would be noise clippy is right to object to.
        let arms: Vec<String> = childless.iter().map(|name| format!("Node::{name}(_)")).collect();
        writeln!(out, "        {} => {{}}", arms.join("\n        | "))?;
    }
    out.push_str("    }\n}\n\n");

    // Visiting children *as ids*, without materialising them.
    //
    // `push_children` into a `Vec<Node>` then reading ids back out costs a push
    // per child plus a 192-arm `Node::node_id` match per child. Recording parents
    // — which happens for every node the parser finishes — wants neither: it only
    // ever needs the id, and at each field the concrete or alias type is known, so
    // the dispatch collapses to a direct `Cell` read or a handful of arms.
    out.push_str(
        "/// Call `f` with the id of each immediate child of `node`.\n\
         ///\n\
         /// Children that were never registered are skipped. Does not recurse, and\n\
         /// does not build an intermediate collection.\n\
         pub fn for_each_child_id(node: Node<'_>, mut f: impl FnMut(crate::NodeId)) {\n    \
         // Concrete nodes carry a `node_id` *field*; without the trait in scope\n    \
         // `child.node_id()` resolves to that field rather than the accessor.\n    \
         use crate::HasNodeId as _;\n    \
         match node {\n",
    );
    let mut idless: Vec<String> = Vec::new();
    for name in ast.nodes.definitions.keys() {
        let fields =
            if name == "Token" { Vec::new() } else { resolve_fields(ast, nullability, name)? };
        let calls: Vec<String> =
            fields.iter().filter_map(|field| child_id_call(ast, field)).collect();
        if calls.is_empty() {
            idless.push(name.clone());
        } else {
            writeln!(out, "        Node::{name}(n) => {{")?;
            for call in calls {
                out.push_str(&call);
            }
            out.push_str("        }\n");
        }
    }
    if !idless.is_empty() {
        let arms: Vec<String> = idless.iter().map(|name| format!("Node::{name}(_)")).collect();
        writeln!(out, "        {} => {{}}", arms.join("\n        | "))?;
    }
    out.push_str("    }\n}\n\n");

    // ---- per-node walkers ----------------------------------------------
    for name in ast.nodes.definitions.keys() {
        let snake = name.to_case(Case::Snake);
        let fields =
            if name == "Token" { Vec::new() } else { resolve_fields(ast, nullability, name)? };

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
/// How to visit one field's children as ids, for `for_each_child_id`.
///
/// Uses the narrowest `node_id` available at the field's static type: a concrete
/// node reads its `Cell` directly, an alias matches a handful of arms, and only a
/// field typed as `Node` pays the full dispatch.
fn child_id_call(ast: &AstDefinition, field: &ResolvedField) -> Option<String> {
    let ty = &field.rust_type;
    let name = &field.rust_name;

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
    let is_list = ty.contains("&'a [");
    let is_option = ty.starts_with("Option<");
    let is_alias = ast.nodes.aliases.contains_key(base) || base == "Node";
    let is_node = ast.nodes.definitions.contains_key(base);
    if !is_alias && !is_node && base != "Token" {
        return None;
    }

    // `Token` is a concrete node with a `node_id` cell like any other; `Node` and
    // the aliases each have their own `node_id`, generated above.
    let visit = |expr: &str| format!("if let Some(id) = {expr}.node_id() {{ f(id); }}");

    Some(match (is_list, is_option) {
        (true, _) => format!(
            "            for child in n.{name} {{\n                {}\n            }}\n",
            visit("child")
        ),
        (false, true) => format!(
            "            if let Some(child) = n.{name} {{\n                {}\n            }}\n",
            visit("child")
        ),
        (false, false) => format!("            {}\n", visit(&format!("n.{name}"))),
    })
}

/// How to push one field's children as `Node`, for `push_children`.
///
/// Mirrors [`child_visit_call`]'s type analysis but always widens to `Node`,
/// because the caller wants a uniform list rather than a typed dispatch.
fn child_push_call(ast: &AstDefinition, field: &ResolvedField) -> Option<String> {
    let ty = &field.rust_type;
    let name = &field.rust_name;

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
    let is_list = ty.contains("&'a [");
    let is_option = ty.starts_with("Option<");
    let is_alias = ast.nodes.aliases.contains_key(base) || base == "Node";
    let is_node = ast.nodes.definitions.contains_key(base);
    // Unlike the walkers, `Token` children are included: they are real nodes with
    // ids, and anything keyed by node identity needs to reach them.
    if !is_alias && !is_node && base != "Token" {
        return None;
    }

    // Three shapes, because `Node` has no blanket `From`: it is already `Node`,
    // it is an alias that converts, or it is a concrete node whose variant has to
    // be named.
    let widen = |expr: &str| -> String {
        if base == "Node" {
            format!("out.push({expr});")
        } else if is_alias {
            format!("out.push(Node::from({expr}));")
        } else {
            format!("out.push(Node::{base}({expr}));")
        }
    };

    Some(match (is_list, is_option) {
        (true, _) => {
            // `&'a [&'a T]` iterates as `&&'a T`: the alias conversion takes a
            // value so it needs the deref, the concrete variant auto-derefs.
            let bound = if is_alias || base == "Node" { "*child" } else { "child" };
            format!(
                "            for child in n.{name} {{\n                {}\n            }}\n",
                widen(bound)
            )
        }
        (false, true) => format!(
            "            if let Some(child) = n.{name} {{\n                {}\n            }}\n",
            widen("child")
        ),
        (false, false) => format!("            {}\n", widen(&format!("n.{name}"))),
    })
}

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
