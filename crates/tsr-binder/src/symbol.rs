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
use std::hash::{Hash, Hasher};
use std::sync::Arc;
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
        /// What a `namespace`'s **exports** table may contribute to a name
        /// resolution — `SymbolFlagsModuleMember` (`ast/symbolflags.go:75`).
        ///
        /// Upstream masks the requested meaning with this before looking in a
        /// module symbol's exports, so a lookup for a *type* cannot be answered
        /// by an exported `const` and vice versa. `ENUM_MEMBER` is absent
        /// deliberately: upstream's list does not carry it, and an enum's
        /// members are reached through the `EnumDeclaration` arm with its own
        /// mask.
        const MODULE_MEMBER = Self::VARIABLE.bits()
            | Self::FUNCTION.bits()
            | Self::CLASS.bits()
            | Self::INTERFACE.bits()
            | Self::ENUM.bits()
            | Self::MODULE.bits()
            | Self::TYPE_ALIAS.bits()
            | Self::ALIAS.bits();
        /// A namespace in either form.
        const NAMESPACE = Self::VALUE_MODULE.bits()
            | Self::NAMESPACE_MODULE.bits()
            | Self::ENUM.bits();
        /// `namespace` in either form.
        const MODULE = Self::VALUE_MODULE.bits() | Self::NAMESPACE_MODULE.bits();
        /// A `get` or `set` accessor.
        const ACCESSOR = Self::GET_ACCESSOR.bits() | Self::SET_ACCESSOR.bits();
        /// Anything that is a member of a class.
        const CLASS_MEMBER =
            Self::METHOD.bits() | Self::ACCESSOR.bits() | Self::PROPERTY.bits();
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
        // A property does **not** exclude another property. Upstream:
        // `PropertyExcludes = Value & ^(Property | Accessor)`
        // (`symbolflags.go:59`). Two properties of the same name in one table —
        // `{ a: 1, a: 2 }`, `interface I { x: string; x: number }`, a class member
        // declared twice — merge in the binder, and the *checker* decides what to
        // say about them (`TS1117` for an object literal, `TS2717`/`TS2687` for a
        // member, `TS2300` only sometimes and from a different code path).
        // Reading this as `PROPERTY` instead made every such duplicate a binder
        // `TS2300`, which is both an unexpected diagnostic and, where upstream
        // does report something, one at the wrong code.
        if self.contains(Self::PROPERTY) {
            return Self::VALUE & !(Self::PROPERTY | Self::ACCESSOR);
        }
        if self.contains(Self::ENUM_MEMBER) {
            return Self::VALUE | Self::TYPE;
        }
        // Overloads: two function declarations of the same name merge, and a
        // function merges with a namespace of the same name.
        if self.contains(Self::FUNCTION) {
            return Self::VALUE & !(Self::FUNCTION | Self::VALUE_MODULE | Self::CLASS);
        }
        // A class merges with an interface, a namespace, or a *function*. The
        // function half was missing: upstream's `ClassExcludes` is
        // `(Value|Type) & ^(ValueModule|Interface|Function)`
        // (`symbolflags.go:63`), with the comment that class-interface mergability
        // is finished in the checker.
        if self.contains(Self::CLASS) {
            return (Self::VALUE | Self::TYPE)
                & !(Self::VALUE_MODULE | Self::INTERFACE | Self::FUNCTION);
        }
        // Interfaces merge with each other and with classes.
        if self.contains(Self::INTERFACE) {
            return Self::TYPE & !(Self::INTERFACE | Self::CLASS);
        }
        // `RegularEnumExcludes = (Value|Type) & ^(RegularEnum|ValueModule)` and
        // `ConstEnumExcludes = (Value|Type) & ^ConstEnum` (`symbolflags.go:64-65`)
        // — a const enum merges only with another const enum, not with a
        // namespace and not with a regular one. §100.
        if self.contains(Self::CONST_ENUM) {
            return (Self::VALUE | Self::TYPE) & !Self::CONST_ENUM;
        }
        if self.intersects(Self::ENUM) {
            return (Self::VALUE | Self::TYPE) & !(Self::REGULAR_ENUM | Self::VALUE_MODULE);
        }
        // Upstream has **two** module excludes (`symbolflags.go:66-67`), and
        // returning `NamespaceModuleExcludes` for both is what §94 measured and
        // refused: a namespace that emits JavaScript occupies value space and
        // does collide with a variable of the same name. §95 supplies the
        // missing half, and it only became correct once `classify` started
        // choosing the flag — the refusal's owner was the flag, not the mask.
        // A value module still merges with a function, a class, an enum and
        // another value module, which is what makes declaration merging useful.
        if self.contains(Self::VALUE_MODULE) {
            return Self::VALUE
                & !(Self::FUNCTION | Self::CLASS | Self::REGULAR_ENUM | Self::VALUE_MODULE);
        }
        // `NamespaceModuleExcludes = None` — a namespace that emits nothing
        // collides with nothing.
        if self.intersects(Self::MODULE) {
            return Self::empty();
        }
        // Upstream: `MethodExcludes = Value & ^Method` (`symbolflags.go:69`). Two
        // methods of the same name are overloads and merge; a method collides with
        // everything else in value space, including a property. `PROPERTY` had it
        // backwards on both counts.
        if self.contains(Self::METHOD) {
            return Self::VALUE & !Self::METHOD;
        }
        if self.intersects(Self::ACCESSOR) {
            // Upstream: `GetAccessorExcludes = Value & ^(SetAccessor | Property)`
            // and the mirror for a setter (`symbolflags.go:70`). Value space minus
            // *the other* accessor — a getter and a setter of the same name pair
            // up — and minus `Property`, which was missing: an accessor merges with
            // a property, and the checker decides whether that is legal
            // (`TS2717`/`TS1049`), not the binder.
            return Self::VALUE & !((Self::ACCESSOR & !self) | Self::PROPERTY);
        }
        // Upstream: `TypeParameterExcludes = Type & ^TypeParameter`
        // (`symbolflags.go:72`). Two type parameters of the same name in one list
        // do *not* collide in the binder — `class A<T, T>` is `TS2300` from the
        // checker, which knows it is looking at a type parameter list. Reading
        // this as all of `TYPE` made it a binder diagnostic instead.
        if self.contains(Self::TYPE_PARAMETER) {
            return Self::TYPE & !Self::TYPE_PARAMETER;
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
    pub members: SymbolTableField<'a>,
    /// Exports, for a module or namespace.
    pub exports: SymbolTableField<'a>,
    /// The symbol whose table this one lives in.
    pub parent: Option<SymbolId>,
    /// For an **export marker**, the export symbol it shadows.
    ///
    /// Ported from the export-symbol link on `ast.Symbol`
    /// (`internal/ast/symbol.go:20`), set by `declareModuleMember`
    /// (`internal/binder/binder.go:373`) at `internal/binder/binder.go:407`:
    ///
    /// ```go
    /// local.ExportSymbol = b.declareSymbol(ast.GetExports(container.Symbol()), …)
    /// ```
    ///
    /// # Why the checker cannot re-derive it
    ///
    /// `export var x` declares twice — a marker into the container's `locals`
    /// carrying only [`SymbolFlags::EXPORT_VALUE`], and the real symbol into the
    /// container's `exports`. An unqualified reference resolves to the marker,
    /// so every consumer that wants the *type* has to get from one to the other.
    ///
    /// Before this field, `Checker::export_symbol_of` reconstructed the link by
    /// walking the marker's declaration to its **source file** and reading that
    /// file's module symbol's `exports`. That is right only when the container
    /// *is* the file. For `namespace N { export enum E {} }` the export lives on
    /// `N`, and for a **script** file there is no module symbol to read at all —
    /// measured at **2,175 assertion lines** over the corpus, of which 988 fail
    /// at the missing-file-symbol step. See
    /// [`docs/architecture/checker-notes-nameres.md`](../../../docs/architecture/checker-notes-nameres.md).
    ///
    /// The binder is the only place that knows which container it declared into,
    /// which is why upstream records the answer here rather than recomputing it.
    ///
    /// `None` for every symbol that is not an export marker, which is almost all
    /// of them.
    pub export_symbol: Option<SymbolId>,
}

/// Names to symbols, within one scope.
pub type SymbolTable<'a> = FxHashMap<&'a str, SymbolId>;

/// A symbol's native nil-or-present member/export table.
/// Reads preserve absence. Initialization and insertion publish a present table;
/// clearing or removing entries never turns that table back into absence.
/// Scope tables remain ordinary `SymbolTable`s.
#[derive(Clone, Debug, Default)]
pub struct SymbolTableField<'a>(Option<SymbolTable<'a>>);

impl<'a> SymbolTableField<'a> {
    /// Whether the native table has been initialized, independently of completion.
    #[must_use]
    pub fn is_present(&self) -> bool {
        self.0.is_some()
    }

    /// Borrow a present table without initializing an absent one.
    #[must_use]
    pub fn as_ref(&self) -> Option<&SymbolTable<'a>> {
        self.0.as_ref()
    }

    /// Native `GetSymbolTable`: initialize even when the table remains empty.
    pub fn initialize(&mut self) -> &mut SymbolTable<'a> {
        self.0.get_or_insert_with(SymbolTable::default)
    }

    /// One existing edge; this read does not publish a table.
    #[must_use]
    pub fn get<Q>(&self, name: &Q) -> Option<&SymbolId>
    where
        &'a str: std::borrow::Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.0.as_ref()?.get(name)
    }

    /// One existing mutable edge, without initializing an absent table.
    pub fn get_mut<Q>(&mut self, name: &Q) -> Option<&mut SymbolId>
    where
        &'a str: std::borrow::Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.0.as_mut()?.get_mut(name)
    }

    /// Insert an edge, initializing its table first.
    pub fn insert(&mut self, name: &'a str, symbol: SymbolId) -> Option<SymbolId> {
        self.initialize().insert(name, symbol)
    }

    /// Remove an edge, preserving table presence.
    pub fn remove<Q>(&mut self, name: &Q) -> Option<SymbolId>
    where
        &'a str: std::borrow::Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.0.as_mut()?.remove(name)
    }

    /// Clear an existing table, preserving its presence and capacity.
    pub fn clear(&mut self) {
        if let Some(table) = &mut self.0 {
            table.clear();
        }
    }

    /// Whether an edge exists, independently of table completion.
    #[must_use]
    pub fn contains_key<Q>(&self, name: &Q) -> bool
    where
        &'a str: std::borrow::Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.get(name).is_some()
    }

    /// The number of existing edges. Absence and a present empty table both have zero.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.as_ref().map_or(0, SymbolTable::len)
    }

    /// Whether there are no edges; this does not answer whether a table is present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Retained edge capacity, without initializing an absent table.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.0.as_ref().map_or(0, SymbolTable::capacity)
    }

    /// Existing edges in the unchanged underlying table's iteration order.
    pub fn iter(&self) -> SymbolTableFieldIter<'_, 'a> {
        self.0.as_ref().map(SymbolTable::iter).into_iter().flatten()
    }

    /// Existing names, without publishing a table.
    pub fn keys(&self) -> impl Iterator<Item = &&'a str> + std::fmt::Debug {
        self.iter().map(|(name, _)| name)
    }

    /// Existing bound ids, without publishing a table.
    pub fn values(&self) -> impl Iterator<Item = &SymbolId> {
        self.iter().map(|(_, id)| id)
    }
}

/// Borrowed iteration over a possibly absent symbol table.
pub type SymbolTableFieldIter<'s, 'a> = std::iter::Flatten<
    std::option::IntoIter<std::collections::hash_map::Iter<'s, &'a str, SymbolId>>,
>;

impl<'s, 'a> IntoIterator for &'s SymbolTableField<'a> {
    type Item = (&'s &'a str, &'s SymbolId);
    type IntoIter = SymbolTableFieldIter<'s, 'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, Q> std::ops::Index<&Q> for SymbolTableField<'a>
where
    &'a str: std::borrow::Borrow<Q>,
    Q: ?Sized + Hash + Eq,
{
    type Output = SymbolId;
    fn index(&self, name: &Q) -> &Self::Output {
        &self.0.as_ref().expect("no entry found for key")[name]
    }
}

/// Allocation identity of one bound Program symbol store.
/// Retained handles keep only this stamp alive, never its symbols or AST.
#[derive(Clone, Debug)]
pub struct SymbolStoreIdentity(Arc<u8>);

impl PartialEq for SymbolStoreIdentity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for SymbolStoreIdentity {}
impl Hash for SymbolStoreIdentity {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

/// Every symbol the binder created, addressed by [`SymbolId`].
///
/// Contiguous storage rather than individually allocated symbols, for the reason
/// the checker spike measured: a handle into a `Vec` beats a reference into
/// scattered memory for this access pattern. See
/// [ADR-0013](../../../docs/adr/0013-checker-memoisation.md).
#[derive(Debug)]
pub struct SymbolStore<'a> {
    symbols: Vec<Symbol<'a>>,
    identity: SymbolStoreIdentity,
}

impl Default for SymbolStore<'_> {
    fn default() -> Self {
        Self { symbols: Vec::new(), identity: SymbolStoreIdentity(Arc::new(1)) }
    }
}

impl<'a> SymbolStore<'a> {
    /// Shared identity for Checkers reading this immutable bound store.
    #[must_use]
    pub fn identity(&self) -> &SymbolStoreIdentity {
        &self.identity
    }

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
            members: SymbolTableField::default(),
            exports: SymbolTableField::default(),
            parent: None,
            export_symbol: None,
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
    fn absent_table_reads_and_mutable_misses_do_not_initialize() {
        let mut table = SymbolTableField::default();
        assert!(!table.is_present());
        assert!(table.get("missing").is_none());
        assert!(table.get_mut("missing").is_none());
        assert!(table.remove("missing").is_none());
        assert!(table.is_empty());
        assert_eq!(table.capacity(), 0);
        assert_eq!(table.iter().count(), 0);
        table.clear();
        assert!(!table.is_present());
        assert!(!table.clone().is_present());
    }

    #[test]
    fn initialized_empty_clone_and_clear_keep_native_presence() {
        let mut symbols = SymbolStore::new();
        let edge = symbols.create("edge", SymbolFlags::PROPERTY);
        let mut table = SymbolTableField::default();
        table.initialize();
        let mut clone = table.clone();
        assert!(table.is_present() && clone.is_present());
        assert!(table.is_empty() && clone.is_empty());
        clone.insert("edge", edge);
        assert_eq!(clone["edge"], edge);
        assert!(table.get("edge").is_none());
        clone.clear();
        assert!(clone.is_present() && clone.is_empty());
        assert!(table.is_present() && table.is_empty());
    }

    #[test]
    fn removing_last_edge_keeps_table_present() {
        let mut symbols = SymbolStore::new();
        let edge = symbols.create("edge", SymbolFlags::PROPERTY);
        let mut table = SymbolTableField::default();
        table.insert("edge", edge);
        assert!(table.is_present());
        assert_eq!(table.remove("edge"), Some(edge));
        assert!(table.is_present() && table.is_empty());
        assert!(table.as_ref().is_some());
    }

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
        // A NON-instantiated namespace merges with anything — it emits nothing,
        // so it occupies no space to collide in. This assertion used to be made
        // of `VALUE_MODULE`, which encoded the divergence §95 fixed: upstream
        // has two module excludes (`symbolflags.go:66-67`) and `classify` picks
        // between them with `GetModuleInstanceState`.
        assert!(SymbolFlags::NAMESPACE_MODULE.excludes().is_empty());
        // An instantiated one occupies value space. It still merges with a
        // function, a class, an enum and another namespace — those are the
        // declaration-merging pairs — and collides with a variable.
        let value_module = SymbolFlags::VALUE_MODULE.excludes();
        assert!(!value_module.contains(SymbolFlags::FUNCTION));
        assert!(!value_module.contains(SymbolFlags::CLASS));
        assert!(!value_module.contains(SymbolFlags::VALUE_MODULE));
        assert!(value_module.contains(SymbolFlags::BLOCK_SCOPED_VARIABLE));
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
