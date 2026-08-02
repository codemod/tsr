//! Deserialization of typescript-go's `_scripts/ast.json`.
//!
//! That file is the single machine-readable definition of the TypeScript AST: it
//! is what upstream's `_scripts/generate-go-ast.ts` consumes to emit
//! `internal/ast/ast_generated.go` and `internal/ast/kind_generated.go`, and it is
//! schema-validated against `_scripts/ast.schema.json`.
//!
//! Generating our Rust AST from the same input is what makes "full conformance"
//! a property of the build rather than a claim in a document: if upstream adds a
//! node kind, regeneration picks it up, and the conformance test in
//! `crates/tsr-ast/tests/` fails until we regenerate.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The root of `ast.json`.
#[derive(Debug, Deserialize)]
pub struct AstDefinition {
    /// Syntax kinds: the `SyntaxKind` enum and its named markers/aliases.
    pub kinds: Kinds,
    /// Shared field groups that node definitions inherit from.
    pub bases: BTreeMap<String, Base>,
    /// Node definitions and the union aliases over them.
    pub nodes: Nodes,
}

/// An entry in the `kinds.elements` array: either a kind or a section comment.
///
/// Upstream interleaves grouping comments (`{"comment": "Punctuation"}`) with kind
/// names. Comments do **not** consume a discriminant, so they must be filtered out
/// before numbering — getting this wrong shifts every subsequent kind.
/// Variants are ordered most-specific first: serde's untagged matching tries them
/// in declaration order, and a bare `{"comment": …}` must not be able to swallow a
/// `{"name": …, "comment": …}` entry — doing so silently drops a syntax kind.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum KindElement {
    /// A syntax kind name, bare.
    Kind(String),
    /// A syntax kind carrying an explanatory comment.
    NamedKind {
        /// The kind name.
        name: String,
        /// The attached comment.
        #[serde(default)]
        comment: Option<String>,
    },
    /// A grouping comment, preserved as a section header in the generated enum.
    Comment {
        /// The comment text.
        comment: String,
    },
}

impl KindElement {
    /// The kind name, if this element declares one.
    pub fn kind_name(&self) -> Option<&str> {
        match self {
            Self::Kind(name) | Self::NamedKind { name, .. } => Some(name),
            Self::Comment { .. } => None,
        }
    }
}

/// The `kinds` section.
#[derive(Debug, Deserialize)]
pub struct Kinds {
    /// Kinds and section comments in declaration order. Ordinal position among the
    /// *kinds* is the discriminant, so this order is load-bearing for conformance.
    pub elements: Vec<KindElement>,
    /// Named aliases for particular kinds, e.g. `FirstAssignment = EqualsToken`.
    pub markers: Vec<Marker>,
    /// Named sets or ranges of kinds, e.g. `TriviaSyntaxKind`.
    pub aliases: BTreeMap<String, KindAlias>,
}

/// A named alias for a single kind.
#[derive(Debug, Deserialize)]
pub struct Marker {
    /// The alias name, without the `Kind` prefix.
    pub name: String,
    /// The kind it refers to.
    pub value: String,
}

/// A named set of kinds — either an explicit list or an inclusive range.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum KindAlias {
    /// An explicit list of kind names.
    List(Vec<String>),
    /// An inclusive range between two markers or kinds.
    Range {
        /// `[first, last]`, inclusive at both ends.
        range: [String; 2],
    },
}

/// The `nodes` section.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Nodes {
    /// Concrete node definitions, keyed by node name.
    pub definitions: BTreeMap<String, NodeDef>,
    /// Union aliases over node definitions, keyed by alias name.
    pub aliases: BTreeMap<String, NodeAlias>,
    /// Named list types, mapping a list alias to its element type.
    #[serde(rename = "listAliases")]
    pub list_aliases: BTreeMap<String, String>,
}

/// A shared group of fields that node definitions inherit.
#[derive(Debug, Default, Deserialize)]
pub struct Base {
    /// Other bases this one inherits from.
    #[serde(default)]
    pub extends: Vec<String>,
    /// Fields contributed by this base.
    #[serde(default)]
    pub fields: BTreeMap<String, Field>,
}

/// A concrete node definition.
#[derive(Debug, Deserialize)]
pub struct NodeDef {
    /// Bases this node inherits fields from.
    #[serde(default)]
    pub extends: Vec<String>,
    /// Members in declaration order. Entries marked `inherited` refer to a base
    /// field and may override parts of it.
    #[serde(default)]
    pub members: Vec<Member>,
    /// For the generic `Token` node: the named instantiations upstream generates
    /// (`AsteriskToken`, `QuestionToken`, …). Fields typed with one of these names
    /// are tokens, not distinct node types.
    #[serde(default, rename = "instantiationAliases")]
    pub instantiation_aliases: BTreeMap<String, String>,
}

/// A member of a node definition.
///
/// Mirrors the on-disk schema one-for-one, including flags the generator does not
/// currently branch on — deserializing the whole shape means a schema change shows
/// up as a parse error rather than being silently ignored.
#[derive(Debug, Deserialize)]
#[allow(clippy::struct_excessive_bools, dead_code)]
pub struct Member {
    /// Field name, as written upstream.
    pub name: String,
    /// Field type. Absent when `inherited` and not overridden.
    #[serde(default)]
    pub r#type: Option<TypeRef>,
    /// Whether this refers to a base field rather than introducing a new one.
    #[serde(default)]
    pub inherited: bool,
    /// Whether the field may be absent.
    #[serde(default)]
    pub optional: bool,
    /// The list type wrapping this field, if it is a list.
    #[serde(default)]
    pub list: Option<String>,
    /// Present only in the Go implementation — binder/checker state, which our
    /// design moves into side tables. Skipped during generation.
    #[serde(default, rename = "goOnly")]
    pub go_only: bool,
    /// Absent from the Go implementation.
    #[serde(default, rename = "noGo")]
    pub no_go: bool,
}

/// A field declared on a base.
#[derive(Debug, Deserialize)]
pub struct Field {
    /// Field type.
    #[serde(default)]
    pub r#type: Option<TypeRef>,
    /// Whether the field may be absent.
    #[serde(default)]
    pub optional: bool,
    /// The list type wrapping this field, if it is a list.
    #[serde(default)]
    pub list: Option<String>,
    /// See [`Member::go_only`].
    #[serde(default, rename = "goOnly")]
    pub go_only: bool,
    /// See [`Member::no_go`].
    #[serde(default, rename = "noGo")]
    pub no_go: bool,
}

/// A type reference: either a named type or an inline union of syntax kinds.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum TypeRef {
    /// A named node, alias, or primitive.
    Named(String),
    /// An inline union such as `["SyntaxKind.PlusToken", "SyntaxKind.MinusToken"]`,
    /// which constrains a token field to a specific set of kinds.
    KindUnion(Vec<String>),
}

impl Kinds {
    /// The syntax kind names, in discriminant order, with comments removed.
    pub fn kind_names(&self) -> Vec<&str> {
        self.elements.iter().filter_map(KindElement::kind_name).collect()
    }

    /// Resolve a marker name to the concrete kind it ultimately denotes.
    ///
    /// Markers may chain — upstream has `LastToken = LastKeyword = DeferKeyword` —
    /// so this follows the chain rather than resolving a single hop.
    pub fn resolve_marker<'a>(&'a self, name: &'a str) -> &'a str {
        let mut current = name;
        // Bounded by the marker count; a cycle would otherwise hang the generator.
        for _ in 0..=self.markers.len() {
            match self.markers.iter().find(|m| m.name == current) {
                Some(marker) => current = &marker.value,
                None => return current,
            }
        }
        current
    }
}

/// A union alias over node definitions.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum NodeAlias {
    /// Every node transitively extending the named base.
    Base {
        /// The base whose implementors form this union.
        base: String,
    },
    /// An explicit union of nodes and/or other aliases.
    Members(Vec<String>),
}

/// Members deserialize with camelCase keys upstream; `go_only` needs the rename
/// applied at the struct level rather than per-field for `Member`.
impl Member {
    /// Whether generation should skip this member entirely.
    pub fn is_skipped(&self) -> bool {
        self.go_only || self.no_go
    }
}

impl Field {
    /// Whether generation should skip this field entirely.
    pub fn is_skipped(&self) -> bool {
        self.go_only || self.no_go
    }
}
