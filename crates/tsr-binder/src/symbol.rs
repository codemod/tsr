//! Symbols and symbol tables.
//!
//! Ported from `internal/ast/symbol.go` and `internal/ast/symbolflags.go` at the
//! pinned commit, with two deliberate differences forced by earlier decisions.
//!
//! **Symbols are handles, not pointers.** Upstream's `*ast.Symbol` becomes a
//! [`SymbolId`] into a contiguous `Vec`, per
//! [ADR-0013](../../../docs/adr/0013-checker-memoisation.md): the checker will ask
//! for symbol data through the store rather than holding references into it.
//!
//! **A node's symbol lives in a side table, not on the node.** Upstream stores it
//! in the node (`ast.go:239`); we cannot, because a mutable field on a shared node
//! is exactly what [ADR-0012](../../../docs/adr/0012-ast-is-sync.md) rules out —
//! and the binder's output has to be readable from every checker thread at once.

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use tsr_ast::NodeId;

/// Index of a symbol in a [`SymbolStore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolId(u32);

impl SymbolId {
    /// As a `usize`, for indexing.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

bitflags::bitflags! {
    /// What a symbol is.
    ///
    /// Values match `ast.SymbolFlags` exactly, because the checker's predicates
    /// are bit tests against these and a divergence would be silent.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct SymbolFlags: u32 {
        /// `var`, or a parameter.
        const FUNCTION_SCOPED_VARIABLE = 1 << 0;
        /// `let` or `const`.
        const BLOCK_SCOPED_VARIABLE = 1 << 1;
        /// Property, or enum member.
        const PROPERTY = 1 << 2;
        /// Enum member.
        const ENUM_MEMBER = 1 << 3;
        /// Function declaration.
        const FUNCTION = 1 << 4;
        /// Class declaration or expression.
        const CLASS = 1 << 5;
        /// Interface declaration.
        const INTERFACE = 1 << 6;
        /// `const enum`.
        const CONST_ENUM = 1 << 7;
        /// `enum`.
        const REGULAR_ENUM = 1 << 8;
        /// A namespace with a value side.
        const VALUE_MODULE = 1 << 9;
        /// A namespace with only types in it.
        const NAMESPACE_MODULE = 1 << 10;
        /// A type literal or mapped type.
        const TYPE_LITERAL = 1 << 11;
        /// An object literal.
        const OBJECT_LITERAL = 1 << 12;
        /// A method.
        const METHOD = 1 << 13;
        /// A constructor.
        const CONSTRUCTOR = 1 << 14;
        /// A `get` accessor.
        const GET_ACCESSOR = 1 << 15;
        /// A `set` accessor.
        const SET_ACCESSOR = 1 << 16;
        /// A call, construct, or index signature.
        const SIGNATURE = 1 << 17;
        /// A type parameter.
        const TYPE_PARAMETER = 1 << 18;
        /// A type alias.
        const TYPE_ALIAS = 1 << 19;
        /// Marks an exported value.
        const EXPORT_VALUE = 1 << 20;
        /// An import or export alias for another symbol.
        const ALIAS = 1 << 21;
        /// The synthetic `prototype` property.
        const PROTOTYPE = 1 << 22;
        /// `export *`.
        const EXPORT_STAR = 1 << 23;
        /// An optional property.
        const OPTIONAL = 1 << 24;
        /// Created by the checker rather than the binder.
        const TRANSIENT = 1 << 25;
        /// Declared by an assignment to a property (`f.x = 1`, `this.x = 1`)
        /// rather than by a declaration. JavaScript files only.
        const ASSIGNMENT = 1 << 26;
        /// The `module` of CommonJS's `module.exports`, and its `exports` member.
        const MODULE_EXPORTS = 1 << 27;
        /// A property declared by `this.x = …` in a constructor, which a real
        /// method or property of the same name replaces outright.
        const REPLACEABLE_BY_METHOD = 1 << 29;

        /// `enum` in either form.
        const ENUM = Self::REGULAR_ENUM.bits() | Self::CONST_ENUM.bits();
        /// `var`, `let` or `const`.
        const VARIABLE =
            Self::FUNCTION_SCOPED_VARIABLE.bits() | Self::BLOCK_SCOPED_VARIABLE.bits();
        /// Anything with a value at runtime.
        const VALUE = Self::VARIABLE.bits()
            | Self::PROPERTY.bits()
            | Self::ENUM_MEMBER.bits()
            | Self::OBJECT_LITERAL.bits()
            | Self::FUNCTION.bits()
            | Self::CLASS.bits()
            | Self::ENUM.bits()
            | Self::VALUE_MODULE.bits()
            | Self::METHOD.bits()
            | Self::GET_ACCESSOR.bits()
            | Self::SET_ACCESSOR.bits();
        /// Anything usable in type position.
        const TYPE = Self::CLASS.bits()
            | Self::INTERFACE.bits()
            | Self::ENUM.bits()
            | Self::ENUM_MEMBER.bits()
            | Self::TYPE_LITERAL.bits()
            | Self::TYPE_PARAMETER.bits()
            | Self::TYPE_ALIAS.bits();
        /// A namespace in either form.
        const NAMESPACE = Self::VALUE_MODULE.bits()
            | Self::NAMESPACE_MODULE.bits()
            | Self::ENUM.bits();
        /// `namespace` in either form.
        const MODULE = Self::VALUE_MODULE.bits() | Self::NAMESPACE_MODULE.bits();
        /// A `get` or `set` accessor.
        const ACCESSOR = Self::GET_ACCESSOR.bits() | Self::SET_ACCESSOR.bits();
    }
}

impl SymbolFlags {
    /// What a declaration with these flags may **not** merge with.
    ///
    /// Upstream spells these as a parallel set of `…Excludes` constants
    /// (`symbolflags.go:52` onward). Deriving them from the declaration's own
    /// flags keeps the two in one place: a new symbol kind that forgets to define
    /// its excludes silently permits every redeclaration, which is the failure
    /// mode a parallel list invites.
    #[must_use]
    pub fn excludes(self) -> Self {
        // `var` may be redeclared as `var`, but not as anything else with a value.
        if self.contains(Self::FUNCTION_SCOPED_VARIABLE) {
            return Self::VALUE & !Self::FUNCTION_SCOPED_VARIABLE;
        }
        // `let`/`const` may not be redeclared at all in value space.
        if self.contains(Self::BLOCK_SCOPED_VARIABLE) {
            return Self::VALUE;
        }
        if self.contains(Self::PROPERTY) {
            return Self::PROPERTY;
        }
        if self.contains(Self::ENUM_MEMBER) {
            return Self::VALUE | Self::TYPE;
        }
        // Overloads: two function declarations of the same name merge, and a
        // function merges with a namespace of the same name.
        if self.contains(Self::FUNCTION) {
            return Self::VALUE & !(Self::FUNCTION | Self::VALUE_MODULE | Self::CLASS);
        }
        // A class merges with an interface or a namespace, not with a value.
        if self.contains(Self::CLASS) {
            return (Self::VALUE | Self::TYPE) & !(Self::VALUE_MODULE | Self::INTERFACE);
        }
        // Interfaces merge with each other and with classes.
        if self.contains(Self::INTERFACE) {
            return Self::TYPE & !(Self::INTERFACE | Self::CLASS);
        }
        if self.intersects(Self::ENUM) {
            return (Self::VALUE | Self::TYPE) & !(Self::ENUM | Self::VALUE_MODULE);
        }
        // Namespaces merge with almost everything; that is what makes declaration
        // merging useful.
        if self.intersects(Self::MODULE) {
            return Self::empty();
        }
        if self.contains(Self::METHOD) {
            return Self::PROPERTY;
        }
        if self.intersects(Self::ACCESSOR) {
            // Upstream: `Value & ^SetAccessor` for a getter, and the mirror for a
            // setter. That is value space minus *the other* accessor: a getter
            // and a setter of the same name pair up, two getters collide.
            return Self::VALUE & !(Self::ACCESSOR & !self);
        }
        if self.contains(Self::TYPE_PARAMETER) {
            return Self::TYPE;
        }
        if self.contains(Self::TYPE_ALIAS) {
            return Self::TYPE;
        }
        if self.contains(Self::ALIAS) {
            return Self::ALIAS;
        }
        Self::empty()
    }
}

/// A named declaration, or a merged group of them.
#[derive(Debug)]
pub struct Symbol<'a> {
    /// What this symbol is; the union of every declaration that merged into it.
    pub flags: SymbolFlags,
    /// The declared name. `""` for the default export and for anonymous
    /// declarations, matching upstream's use of a well-known name.
    pub name: &'a str,
    /// Every declaration that contributed, in source order.
    ///
    /// Inline for one, which is what the overwhelming majority are: across the
    /// four benchmark fixtures 41,529 symbols are created and only a few hundred
    /// merge, so a `Vec` here meant ~41k heap allocations to hold a single
    /// four-byte id apiece.
    pub declarations: SmallVec<[NodeId; 1]>,
    /// The declaration that gives the symbol its value, if any.
    pub value_declaration: Option<NodeId>,
    /// Members, for a class, interface, enum, or type literal.
    pub members: SymbolTable<'a>,
    /// Exports, for a module or namespace.
    pub exports: SymbolTable<'a>,
    /// The symbol whose table this one lives in.
    pub parent: Option<SymbolId>,
}

/// Names to symbols, within one scope.
pub type SymbolTable<'a> = FxHashMap<&'a str, SymbolId>;

/// Every symbol the binder created, addressed by [`SymbolId`].
///
/// Contiguous storage rather than individually allocated symbols, for the reason
/// the checker spike measured: a handle into a `Vec` beats a reference into
/// scattered memory for this access pattern. See
/// [ADR-0013](../../../docs/adr/0013-checker-memoisation.md).
#[derive(Debug, Default)]
pub struct SymbolStore<'a> {
    symbols: Vec<Symbol<'a>>,
}

impl<'a> SymbolStore<'a> {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a symbol and return its id.
    pub fn create(&mut self, name: &'a str, flags: SymbolFlags) -> SymbolId {
        let id = SymbolId(u32::try_from(self.symbols.len()).expect("symbol count exceeds u32"));
        self.symbols.push(Symbol {
            flags,
            name,
            declarations: SmallVec::new(),
            value_declaration: None,
            members: SymbolTable::default(),
            exports: SymbolTable::default(),
            parent: None,
        });
        id
    }

    /// How many symbols exist.
    #[must_use]
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Whether no symbols exist.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Shared access to a symbol.
    #[must_use]
    pub fn get(&self, id: SymbolId) -> &Symbol<'a> {
        &self.symbols[id.index()]
    }

    /// Mutable access to a symbol.
    pub fn get_mut(&mut self, id: SymbolId) -> &mut Symbol<'a> {
        &mut self.symbols[id.index()]
    }

    /// Every symbol, in creation order.
    pub fn iter(&self) -> impl Iterator<Item = (SymbolId, &Symbol<'a>)> {
        self.symbols
            .iter()
            .enumerate()
            .map(|(i, s)| (SymbolId(u32::try_from(i).expect("symbol count exceeds u32")), s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables_and_functions_have_the_merge_rules_typescript_has() {
        // `var` twice is legal; `let` twice is not.
        assert!(
            !SymbolFlags::FUNCTION_SCOPED_VARIABLE
                .excludes()
                .contains(SymbolFlags::FUNCTION_SCOPED_VARIABLE)
        );
        assert!(
            SymbolFlags::BLOCK_SCOPED_VARIABLE
                .excludes()
                .contains(SymbolFlags::BLOCK_SCOPED_VARIABLE)
        );

        // Function overloads merge; a function and a `let` do not.
        assert!(!SymbolFlags::FUNCTION.excludes().contains(SymbolFlags::FUNCTION));
        assert!(SymbolFlags::FUNCTION.excludes().contains(SymbolFlags::BLOCK_SCOPED_VARIABLE));
    }

    #[test]
    fn declaration_merging_pairs_are_permitted() {
        // The combinations TypeScript documents as merging.
        assert!(!SymbolFlags::INTERFACE.excludes().contains(SymbolFlags::INTERFACE));
        assert!(!SymbolFlags::INTERFACE.excludes().contains(SymbolFlags::CLASS));
        assert!(!SymbolFlags::CLASS.excludes().contains(SymbolFlags::INTERFACE));
        assert!(!SymbolFlags::CLASS.excludes().contains(SymbolFlags::VALUE_MODULE));
        assert!(!SymbolFlags::FUNCTION.excludes().contains(SymbolFlags::VALUE_MODULE));
        // A namespace merges with anything.
        assert!(SymbolFlags::VALUE_MODULE.excludes().is_empty());
    }

    #[test]
    fn a_getter_and_a_setter_pair_but_two_getters_do_not() {
        assert!(!SymbolFlags::GET_ACCESSOR.excludes().contains(SymbolFlags::SET_ACCESSOR));
        assert!(SymbolFlags::GET_ACCESSOR.excludes().contains(SymbolFlags::GET_ACCESSOR));
        assert!(!SymbolFlags::SET_ACCESSOR.excludes().contains(SymbolFlags::GET_ACCESSOR));
        assert!(SymbolFlags::SET_ACCESSOR.excludes().contains(SymbolFlags::SET_ACCESSOR));
    }

    #[test]
    fn type_aliases_do_not_merge_with_anything_in_type_space() {
        assert!(SymbolFlags::TYPE_ALIAS.excludes().contains(SymbolFlags::TYPE_ALIAS));
        assert!(SymbolFlags::TYPE_ALIAS.excludes().contains(SymbolFlags::INTERFACE));
    }
}
