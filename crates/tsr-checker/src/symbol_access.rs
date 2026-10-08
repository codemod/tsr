//! Validated Program and Checker symbol ownership.
//!
//! Native private/transient symbols never become binder indexes. See
//! docs/architecture/checker-symbol-completion-contract.md. This store supplies
//! ownership and record operations; alias resolution and node completion are
//! separate consumers and do not publish caches here.

use crate::TypeId;
use rustc_hash::FxHashMap;
use std::borrow::Cow;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tsr_ast::NodeId;
use tsr_binder::{Symbol, SymbolFlags, SymbolId, SymbolStore, SymbolStoreIdentity};

#[derive(Clone, Debug)]
struct CheckerIdentity(Arc<u8>);
impl PartialEq for CheckerIdentity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for CheckerIdentity {}
impl Hash for CheckerIdentity {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

/// An owned symbol identity. Constructors and private indexes stay in this module.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SymbolRef(Identity);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Identity {
    Bound(SymbolStoreIdentity, SymbolId),
    Private(CheckerIdentity, u32),
}

bitflags::bitflags! {
    /// Native ast.CheckFlags, independent of SymbolFlagsTransient ownership.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct CheckFlags: u32 {
        /// Instantiated symbol.
        const INSTANTIATED = 1 << 0;
        /// Property in union or intersection type.
        const SYNTHETIC_PROPERTY = 1 << 1;
        /// Method in union or intersection type.
        const SYNTHETIC_METHOD = 1 << 2;
        /// Readonly transient symbol.
        const READONLY = 1 << 3;
        /// Synthetic property present in some but not all constituents.
        const READ_PARTIAL = 1 << 4;
        /// Synthetic property present in some but only satisfied by an index signature in others.
        const WRITE_PARTIAL = 1 << 5;
        /// Synthetic property with non-uniform type in constituents.
        const HAS_NON_UNIFORM_TYPE = 1 << 6;
        /// Synthetic property with at least one literal type in constituents.
        const HAS_LITERAL_TYPE = 1 << 7;
        /// Synthetic property with public constituent(s).
        const CONTAINS_PUBLIC = 1 << 8;
        /// Synthetic property with protected constituent(s).
        const CONTAINS_PROTECTED = 1 << 9;
        /// Synthetic property with private constituent(s).
        const CONTAINS_PRIVATE = 1 << 10;
        /// Synthetic property with static constituent(s).
        const CONTAINS_STATIC = 1 << 11;
        /// Late-bound symbol for a computed property with a dynamic name.
        const LATE = 1 << 12;
        /// Property of reverse-inferred homomorphic mapped type.
        const REVERSE_MAPPED = 1 << 13;
        /// Optional parameter.
        const OPTIONAL_PARAMETER = 1 << 14;
        /// Rest parameter.
        const REST_PARAMETER = 1 << 15;
        /// Calculation of the type of this symbol is deferred due to processing costs, should be fetched with `getTypeOfSymbolWithDeferredType`.
        const DEFERRED_TYPE = 1 << 16;
        /// Synthetic property with at least one never type in constituents.
        const HAS_NEVER_TYPE = 1 << 17;
        /// Property of mapped type.
        const MAPPED = 1 << 18;
        /// Strip optionality in mapped property.
        const STRIP_OPTIONAL = 1 << 19;
        /// Unresolved type alias symbol.
        const UNRESOLVED = 1 << 20;
        /// IsDiscriminant flags has been computed.
        const IS_DISCRIMINANT_COMPUTED = 1 << 21;
        /// Discriminant property.
        const IS_DISCRIMINANT = 1 << 22;
        /// Synthetic property created from index signature.
        const INDEX_SYMBOL = 1 << 23;
        /// Either synthetic property or synthetic method.
        const SYNTHETIC = Self::SYNTHETIC_PROPERTY.bits() | Self::SYNTHETIC_METHOD.bits();
        /// Nonuniform or literal constituent type.
        const NON_UNIFORM_AND_LITERAL = Self::HAS_NON_UNIFORM_TYPE.bits() | Self::HAS_LITERAL_TYPE.bits();
        /// Read or write partial property.
        const PARTIAL = Self::READ_PARTIAL.bits() | Self::WRITE_PARTIAL.bits();
    }
}

/// Ownership violations must not be recovered as missing-name results.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolAccessError {
    /// The bound handle belongs to another Program store.
    ForeignProgram,
    /// The private handle belongs to another Checker.
    ForeignChecker,
    /// The validated domain has no record at this index.
    InvalidIndex,
    /// A mutation requested a bound Program record.
    ImmutableProgram,
}

type PrivateTable<'a> = FxHashMap<Cow<'a, str>, SymbolRef>;

#[derive(Debug)]
struct PrivateSymbol<'a> {
    flags: SymbolFlags,
    check_flags: CheckFlags,
    name: Cow<'a, str>,
    declarations: Vec<NodeId>,
    value_declaration: Option<NodeId>,
    parent: Option<SymbolRef>,
    export_symbol: Option<SymbolRef>,
    members: Option<PrivateTable<'a>>,
    exports: Option<PrivateTable<'a>>,
    origin: Option<SymbolRef>,
    declared_type: Option<TypeId>,
}

/// A short-lived read view; drop it before recursively mutating the Checker.
pub struct SymbolView<'s, 'a>(View<'s, 'a>);

enum View<'s, 'a> {
    Bound(&'s Symbol<'a>, &'s SymbolStoreIdentity),
    Private(&'s PrivateSymbol<'a>),
}

impl SymbolView<'_, '_> {
    /// Declaration flags including private Transient ownership.
    #[must_use]
    pub fn flags(&self) -> SymbolFlags {
        match self.0 {
            View::Bound(s, _) => s.flags,
            View::Private(s) => s.flags,
        }
    }
    /// Native transient metadata; zero for a bound record.
    #[must_use]
    pub fn check_flags(&self) -> CheckFlags {
        match self.0 {
            View::Bound(_, _) => CheckFlags::empty(),
            View::Private(s) => s.check_flags,
        }
    }
    /// The record's leaf name; use `symbol_path` for a qualified path.
    #[must_use]
    pub fn name(&self) -> &str {
        match self.0 {
            View::Bound(s, _) => s.name,
            View::Private(s) => &s.name,
        }
    }
    /// Declaration ids in native append order.
    #[must_use]
    pub fn declarations(&self) -> &[NodeId] {
        match self.0 {
            View::Bound(s, _) => &s.declarations,
            View::Private(s) => &s.declarations,
        }
    }
    /// The declaration providing this symbol's value.
    #[must_use]
    pub fn value_declaration(&self) -> Option<NodeId> {
        match self.0 {
            View::Bound(s, _) => s.value_declaration,
            View::Private(s) => s.value_declaration,
        }
    }
    /// Raw parent identity, before late-bound or merged redirection.
    #[must_use]
    pub fn parent(&self) -> Option<SymbolRef> {
        match &self.0 {
            View::Bound(s, domain) => {
                s.parent.map(|id| SymbolRef(Identity::Bound((*domain).clone(), id)))
            }
            View::Private(s) => s.parent.clone(),
        }
    }
    /// Explicit export-marker edge, without alias resolution.
    #[must_use]
    pub fn export_symbol(&self) -> Option<SymbolRef> {
        match &self.0 {
            View::Bound(s, domain) => {
                s.export_symbol.map(|id| SymbolRef(Identity::Bound((*domain).clone(), id)))
            }
            View::Private(s) => s.export_symbol.clone(),
        }
    }
    /// One existing member edge; absence is not completed member resolution.
    #[must_use]
    pub fn member(&self, name: &str) -> Option<SymbolRef> {
        match &self.0 {
            View::Bound(s, domain) => {
                s.members.get(name).map(|&id| SymbolRef(Identity::Bound((*domain).clone(), id)))
            }
            View::Private(s) => s.members.as_ref()?.get(name).cloned(),
        }
    }
    /// One existing export edge, without alias or merge forcing.
    #[must_use]
    pub fn export(&self, name: &str) -> Option<SymbolRef> {
        match &self.0 {
            View::Bound(s, domain) => {
                s.exports.get(name).map(|&id| SymbolRef(Identity::Bound((*domain).clone(), id)))
            }
            View::Private(s) => s.exports.as_ref()?.get(name).cloned(),
        }
    }
    /// Whether the Rust record has a member table, independently of completion.
    #[must_use]
    pub fn has_member_table(&self) -> bool {
        match &self.0 {
            View::Bound(s, _) => s.members.is_present(),
            View::Private(s) => s.members.is_some(),
        }
    }
    /// Whether the Rust record has an export table, independently of completion.
    #[must_use]
    pub fn has_export_table(&self) -> bool {
        match &self.0 {
            View::Bound(s, _) => s.exports.is_present(),
            View::Private(s) => s.exports.is_some(),
        }
    }
    /// Original identity of a private clone; may itself be private.
    #[must_use]
    pub fn origin(&self) -> Option<SymbolRef> {
        match &self.0 {
            View::Bound(_, _) => None,
            View::Private(s) => s.origin.clone(),
        }
    }
    /// Private unresolved alias linkage; ordinary declared types are separate consumers.
    #[must_use]
    pub fn declared_type(&self) -> Option<TypeId> {
        match &self.0 {
            View::Bound(_, _) => None,
            View::Private(s) => s.declared_type,
        }
    }
}

/// A snapshot of private retained storage, not allocation counts or process RSS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SymbolStorageUsage {
    /// Initialized records, including the unknown sentinel.
    pub records: usize,
    /// Slots reserved in the private record vector.
    pub record_capacity: usize,
    /// Reserved inline record bytes; excludes pointed-to allocations.
    pub inline_record_bytes: usize,
    /// Capacity of owned leaf names, in bytes.
    pub owned_name_bytes: usize,
    /// Reserved declaration-buffer bytes.
    pub declaration_bytes: usize,
    /// Unresolved full-path entries and table capacity.
    pub unresolved_entries: usize,
    /// Unresolved table capacity, without estimating hash bucket overhead.
    pub unresolved_capacity: usize,
    /// Capacity of owned full-path keys, in bytes.
    pub path_key_bytes: usize,
    /// Merged redirect entries.
    pub merged_entries: usize,
    /// Merged redirect table capacity.
    pub merged_capacity: usize,
    /// Combined capacities of allocated private member/export tables.
    pub edge_capacity: usize,
    /// Live strong references to the private stamp, including this store.
    pub stamp_references: usize,
}

/// Private records and redirects for one Checker reading one frozen bound store.
pub struct CheckerSymbols<'a> {
    program: &'a SymbolStore<'a>,
    domain: CheckerIdentity,
    records: Vec<PrivateSymbol<'a>>,
    unknown: SymbolRef,
    unresolved: FxHashMap<String, SymbolRef>,
    merged: FxHashMap<SymbolRef, SymbolRef>,
    unresolved_type: TypeId,
}

impl<'a> CheckerSymbols<'a> {
    /// Construct once per Checker. The type argument must belong to its `TypeStore`.
    #[must_use]
    pub fn new(program: &'a SymbolStore<'a>, unresolved_type: TypeId) -> Self {
        let domain = CheckerIdentity(Arc::new(1));
        let unknown = SymbolRef(Identity::Private(domain.clone(), 0));
        Self {
            program,
            domain,
            unknown,
            records: vec![PrivateSymbol {
                flags: SymbolFlags::PROPERTY | SymbolFlags::TRANSIENT,
                check_flags: CheckFlags::empty(),
                name: Cow::Borrowed("unknown"),
                declarations: Vec::new(),
                value_declaration: None,
                parent: None,
                export_symbol: None,
                members: None,
                exports: None,
                origin: None,
                declared_type: None,
            }],
            unresolved: FxHashMap::default(),
            merged: FxHashMap::default(),
            unresolved_type,
        }
    }

    /// The private unknown sentinel; distinct from an uncomputed node link.
    #[must_use]
    pub fn unknown(&self) -> SymbolRef {
        self.unknown.clone()
    }

    /// Lift a bound id at the Program boundary; private ids have no such conversion.
    pub fn bound(&self, id: SymbolId) -> Result<SymbolRef, SymbolAccessError> {
        if id.index() >= self.program.len() {
            return Err(SymbolAccessError::InvalidIndex);
        }
        Ok(SymbolRef(Identity::Bound(self.program.identity().clone(), id)))
    }

    /// Validate the domain before indexing either store.
    pub fn view(&self, symbol: &SymbolRef) -> Result<SymbolView<'_, 'a>, SymbolAccessError> {
        match &symbol.0 {
            Identity::Bound(domain, id) => {
                if domain != self.program.identity() {
                    return Err(SymbolAccessError::ForeignProgram);
                }
                if id.index() >= self.program.len() {
                    return Err(SymbolAccessError::InvalidIndex);
                }
                Ok(SymbolView(View::Bound(self.program.get(*id), self.program.identity())))
            }
            Identity::Private(domain, index) => {
                if domain != &self.domain {
                    return Err(SymbolAccessError::ForeignChecker);
                }
                let record =
                    self.records.get(*index as usize).ok_or(SymbolAccessError::InvalidIndex)?;
                Ok(SymbolView(View::Private(record)))
            }
        }
    }

    fn private_mut(
        &mut self,
        symbol: &SymbolRef,
    ) -> Result<&mut PrivateSymbol<'a>, SymbolAccessError> {
        self.view(symbol)?;
        let Identity::Private(_, index) = symbol.0 else {
            return Err(SymbolAccessError::ImmutableProgram);
        };
        self.records.get_mut(index as usize).ok_or(SymbolAccessError::InvalidIndex)
    }

    fn add(&mut self, record: PrivateSymbol<'a>) -> SymbolRef {
        let index = u32::try_from(self.records.len()).expect("private symbol count exceeds u32");
        self.records.push(record);
        SymbolRef(Identity::Private(self.domain.clone(), index))
    }

    /// Inspect retained capacities without changing symbol or completion state.
    #[must_use]
    pub fn storage_usage(&self) -> SymbolStorageUsage {
        SymbolStorageUsage {
            records: self.records.len(),
            record_capacity: self.records.capacity(),
            inline_record_bytes: self.records.capacity() * std::mem::size_of::<PrivateSymbol<'a>>(),
            owned_name_bytes: self
                .records
                .iter()
                .map(|r| match &r.name {
                    Cow::Owned(name) => name.capacity(),
                    Cow::Borrowed(_) => 0,
                })
                .sum(),
            declaration_bytes: self
                .records
                .iter()
                .map(|r| r.declarations.capacity() * std::mem::size_of::<NodeId>())
                .sum(),
            unresolved_entries: self.unresolved.len(),
            unresolved_capacity: self.unresolved.capacity(),
            path_key_bytes: self.unresolved.keys().map(String::capacity).sum(),
            merged_entries: self.merged.len(),
            merged_capacity: self.merged.capacity(),
            edge_capacity: self
                .records
                .iter()
                .map(|r| {
                    r.members.as_ref().map_or(0, FxHashMap::capacity)
                        + r.exports.as_ref().map_or(0, FxHashMap::capacity)
                })
                .sum(),
            stamp_references: Arc::strong_count(&self.domain.0),
        }
    }

    /// Native newSymbolEx: all new checker symbols are Transient, even with zero `CheckFlags`.
    pub fn new_symbol(
        &mut self,
        flags: SymbolFlags,
        name: Cow<'a, str>,
        check_flags: CheckFlags,
    ) -> SymbolRef {
        self.add(PrivateSymbol {
            flags: flags | SymbolFlags::TRANSIENT,
            name,
            check_flags,
            declarations: Vec::new(),
            value_declaration: None,
            parent: None,
            export_symbol: None,
            members: None,
            exports: None,
            origin: None,
            declared_type: None,
        })
    }

    /// Intern one native entity-name segment with its already resolved raw parent.
    /// The full path is the key; leaf text need not itself be a qualified path.
    pub fn unresolved_symbol(
        &mut self,
        name: &str,
        parent: Option<&SymbolRef>,
    ) -> Result<SymbolRef, SymbolAccessError> {
        if let Some(parent) = parent {
            self.view(parent)?;
        }
        if name.is_empty() {
            return Ok(self.unknown());
        }
        let key = match parent {
            Some(parent) => format!("{}.{name}", self.symbol_path(parent)?),
            None => name.to_owned(),
        };
        if let Some(result) = self.unresolved.get(&key) {
            return Ok(result.clone());
        }
        let result = self.add(PrivateSymbol {
            flags: SymbolFlags::TYPE_ALIAS | SymbolFlags::TRANSIENT,
            check_flags: CheckFlags::UNRESOLVED,
            name: Cow::Owned(name.to_owned()),
            declarations: Vec::new(),
            value_declaration: None,
            parent: parent.cloned(),
            export_symbol: None,
            members: None,
            exports: None,
            origin: None,
            declared_type: Some(self.unresolved_type),
        });
        self.unresolved.insert(key, result.clone());
        Ok(result)
    }

    /// Query a synthetic qualified path. Production entity names supply AST segments.
    pub fn unresolved_path(&mut self, path: &str) -> SymbolRef {
        let (left, name) = match path.rsplit_once('.') {
            Some((left, right)) => (Some(left), right),
            None => (None, path),
        };
        // Native skips parent recursion when the right identifier is empty.
        if name.is_empty() {
            return self.unknown();
        }
        let parent = left.map(|left| self.unresolved_path(left));
        self.unresolved_symbol(name, parent.as_ref()).expect("own unresolved parent")
    }

    /// Build the native raw parent/name path. This does not resolve aliases or merges.
    pub fn symbol_path(&self, symbol: &SymbolRef) -> Result<String, SymbolAccessError> {
        let view = self.view(symbol)?;
        let mut path = match view.parent() {
            // Native getSymbolPath tests parent presence, not parent-name length.
            Some(parent) => format!("{}.", self.symbol_path(&parent)?),
            None => String::new(),
        };
        path.push_str(view.name());
        Ok(path)
    }

    /// Native cloneSymbol: independent tables/declarations, shallow symbol edges.
    pub fn clone_symbol(&mut self, symbol: &SymbolRef) -> Result<SymbolRef, SymbolAccessError> {
        self.view(symbol)?;
        let record = match &symbol.0 {
            Identity::Bound(_, id) => {
                let source = self.program.get(*id);
                let table = |entries: &tsr_binder::SymbolTable<'a>| -> Result<PrivateTable<'a>, SymbolAccessError> {
                    entries.iter().map(|(&name, &id)| Ok((Cow::Borrowed(name), self.bound(id)?))).collect()
                };
                PrivateSymbol {
                    flags: source.flags | SymbolFlags::TRANSIENT,
                    check_flags: CheckFlags::empty(),
                    name: Cow::Borrowed(source.name),
                    declarations: source.declarations.to_vec(),
                    value_declaration: source.value_declaration,
                    parent: source.parent.map(|id| self.bound(id)).transpose()?,
                    export_symbol: None,
                    members: source.members.as_ref().map(table).transpose()?,
                    exports: source.exports.as_ref().map(table).transpose()?,
                    origin: Some(symbol.clone()),
                    declared_type: None,
                }
            }
            Identity::Private(_, index) => {
                let source = &self.records[*index as usize];
                PrivateSymbol {
                    flags: source.flags | SymbolFlags::TRANSIENT,
                    check_flags: CheckFlags::empty(),
                    name: source.name.clone(),
                    declarations: source.declarations.clone(),
                    value_declaration: source.value_declaration,
                    parent: source.parent.clone(),
                    export_symbol: None,
                    members: source.members.clone(),
                    exports: source.exports.clone(),
                    origin: Some(symbol.clone()),
                    declared_type: None,
                }
            }
        };
        let result = self.add(record);
        self.record_merged(&result, symbol)?;
        Ok(result)
    }

    /// Native recordMergedSymbol: checker-local redirect, independent of Binder redirects.
    pub fn record_merged(
        &mut self,
        target: &SymbolRef,
        source: &SymbolRef,
    ) -> Result<(), SymbolAccessError> {
        self.view(target)?;
        self.view(source)?;
        self.merged.insert(source.clone(), target.clone());
        Ok(())
    }

    /// Native getMergedSymbol is a single redirect, not the Binder's bounded chain walk.
    pub fn merged_symbol(&self, symbol: &SymbolRef) -> Result<SymbolRef, SymbolAccessError> {
        self.view(symbol)?;
        Ok(self.merged.get(symbol).unwrap_or(symbol).clone())
    }

    /// Append only to a private declaration sequence.
    pub fn append_declaration(
        &mut self,
        symbol: &SymbolRef,
        declaration: NodeId,
    ) -> Result<(), SymbolAccessError> {
        self.private_mut(symbol)?.declarations.push(declaration);
        Ok(())
    }

    /// Set a validated raw parent. Late-bound/merged parent queries are a separate consumer.
    pub fn set_parent(
        &mut self,
        symbol: &SymbolRef,
        parent: Option<&SymbolRef>,
    ) -> Result<(), SymbolAccessError> {
        if let Some(parent) = parent {
            self.view(parent)?;
        }
        self.private_mut(symbol)?.parent = parent.cloned();
        Ok(())
    }

    /// Set a validated export marker edge.
    pub fn set_export_symbol(
        &mut self,
        symbol: &SymbolRef,
        export: Option<&SymbolRef>,
    ) -> Result<(), SymbolAccessError> {
        if let Some(export) = export {
            self.view(export)?;
        }
        self.private_mut(symbol)?.export_symbol = export.cloned();
        Ok(())
    }

    /// Initialize or extend a private member table without mutating its clone's source.
    pub fn set_member(
        &mut self,
        symbol: &SymbolRef,
        name: Cow<'a, str>,
        member: &SymbolRef,
    ) -> Result<(), SymbolAccessError> {
        self.view(member)?;
        self.private_mut(symbol)?
            .members
            .get_or_insert_with(FxHashMap::default)
            .insert(name, member.clone());
        Ok(())
    }

    /// Initialize or extend a private export table.
    pub fn set_export(
        &mut self,
        symbol: &SymbolRef,
        name: Cow<'a, str>,
        export: &SymbolRef,
    ) -> Result<(), SymbolAccessError> {
        self.view(export)?;
        self.private_mut(symbol)?
            .exports
            .get_or_insert_with(FxHashMap::default)
            .insert(name, export.clone());
        Ok(())
    }

    /// Change native `CheckFlags` without changing Transient ownership.
    pub fn set_check_flags(
        &mut self,
        symbol: &SymbolRef,
        flags: CheckFlags,
    ) -> Result<(), SymbolAccessError> {
        self.private_mut(symbol)?.check_flags = flags;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_handle_keeps_only_stamp_alive() {
        let program = SymbolStore::new();
        let mut types = crate::TypeStore::new();
        let unresolved = crate::Intrinsics::create(&mut types).unresolved;
        let checker = CheckerSymbols::new(&program, unresolved);
        let weak = Arc::downgrade(&checker.domain.0);
        let handle = checker.unknown();
        drop(checker);
        assert!(weak.upgrade().is_some());
        drop(handle);
        assert!(weak.upgrade().is_none());
    }
}

// ---------------------------------------------------------------------------
// Declaration-emit symbol accessibility (`EmitResolver`).
// ---------------------------------------------------------------------------

/// `printer.SymbolAccessibility` (`printer/emitresolver.go`), restricted to
/// the answers [`DeclarationEmitResolver::is_entity_name_visible`] produces.
/// `CannotBeNamed` comes only from `isSymbolAccessible`'s module-specifier
/// arm, which nothing here ports yet (`docs/parity/notes/r4-declemit.md` §3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EmitAccessibility {
    /// `SymbolAccessibilityAccessible`, with `AliasesToMakeVisible`: the
    /// statements `hasVisibleDeclarations` painted visible on the way.
    Accessible {
        /// The aliasing statements to emit after all.
        aliases_to_make_visible: Vec<NodeId>,
    },
    /// `SymbolAccessibilityNotAccessible`, with `ErrorSymbolName` and
    /// `ErrorNode` (the entity name's first identifier).
    NotAccessible {
        /// The first identifier's text.
        error_symbol_name: String,
        /// The first identifier.
        error_node: NodeId,
    },
    /// `SymbolAccessibilityNotResolved`: the checker reports unresolvable
    /// names itself, so declaration emit stays silent.
    NotResolved,
}

/// A `SymbolTracker` call the node builder makes while serializing an
/// inferred type (`checker/symboltracker.go`), as
/// [`DeclarationEmitResolver::inferred_type_reports`] answers it. The
/// declaration transform turns each into its diagnostic
/// (`transformers/declarations/tracker.go`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrackerReport {
    /// `ReportPrivateInBaseOfClassExpression(propertyName)`.
    PrivateInBaseOfClassExpression(String),
    /// `ReportLikelyUnsafeImportRequiredError(specifier, symbolName)`
    /// (`nodebuilderimpl.go:709`).
    LikelyUnsafeImportRequired {
        /// The specifier that dives into `node_modules`.
        specifier: String,
        /// `symbol.Name`; empty selects the two-argument message.
        symbol_name: String,
    },
}

/// Pinned tsgo 5b1047d: `EmitResolver` (`checker/emitresolver.go`), the
/// accessibility half: `isDeclarationVisible`, `determineIfDeclarationIsVisible`,
/// `PrecalculateDeclarationEmitVisibility`/`markLinkedAliases`,
/// `isEntityNameVisible` and `hasVisibleDeclarations`.
///
/// Checker port convention record (`docs/conventions.md`):
///
/// - **Native operation**: `EmitResolver.declarationLinks.isVisible` and
///   `declarationFileLinks.aliasesMarked`.
/// - **Key identity and owner**: declaration `NodeId` / source-file `NodeId`;
///   owned by this resolver value, which plays native's per-program
///   `EmitResolver` (its links are the resolver's, not the checker's — native
///   keeps them on `EmitResolver`, so the checker's own declaration
///   visibility used by type printing is untouched).
/// - **Publication states**: absent = unknown (`TSUnknown`); `true`/`false`
///   once `determineIfDeclarationIsVisible` ran. `hasVisibleDeclarations`'
///   `addVisibleAlias` and `markLinkedAliases` *paint* an entry `true`, which
///   is native's mutation and is what makes a referenced non-exported
///   statement emittable.
/// - **Receiver/alias context**: none; names resolve at the enclosing
///   declaration through the checker's `resolve_name_with_export_alias`.
/// - **Expensive work**: name resolution and alias resolution, both the
///   checker's; the walk itself is one ancestor chain per query.
pub struct DeclarationEmitResolver<'c, 'a, 'n> {
    checker: &'c mut crate::checker::Checker<'a, 'n>,
    is_visible: FxHashMap<NodeId, bool>,
    aliases_marked: rustc_hash::FxHashSet<NodeId>,
}

impl<'c, 'a, 'n> DeclarationEmitResolver<'c, 'a, 'n> {
    /// A resolver with no visibility decided yet.
    pub fn new(checker: &'c mut crate::checker::Checker<'a, 'n>) -> Self {
        Self {
            checker,
            is_visible: FxHashMap::default(),
            aliases_marked: rustc_hash::FxHashSet::default(),
        }
    }

    fn kind(&self, node: NodeId) -> tsr_ast::SyntaxKind {
        self.checker.nodes.kind(node)
    }

    fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.checker.nodes.parent(node)
    }

    fn modifiers(&self, node: NodeId) -> &'a [tsr_ast::ModifierLike<'a>] {
        use tsr_ast::Node;
        match self.checker.node_map.get(node) {
            Some(Node::PropertySignatureDeclaration(n)) => n.modifiers,
            Some(Node::MethodSignatureDeclaration(n)) => n.modifiers,
            Some(node) => crate::check::modifiers_of(node).unwrap_or(&[]),
            None => &[],
        }
    }

    /// `ast.HasSyntacticModifier(node, flag)` for one modifier keyword.
    fn has_modifier(&self, node: NodeId, kind: tsr_ast::SyntaxKind) -> bool {
        self.modifiers(node)
            .iter()
            .any(|m| matches!(m, tsr_ast::ModifierLike::Token(token) if token.kind == kind))
    }

    /// `getCombinedModifierFlagsCached(node) & ModifierFlagsExport`
    /// (`ast.GetCombinedModifierFlags`): a variable declaration also carries
    /// its list's and statement's modifiers.
    fn has_combined_export(&self, node: NodeId) -> bool {
        use tsr_ast::SyntaxKind as K;
        let mut node = self.root_declaration(node);
        if self.has_modifier(node, K::ExportKeyword) {
            return true;
        }
        if self.kind(node) == K::VariableDeclaration
            && let Some(list) = self.parent(node)
        {
            node = list;
        }
        if self.kind(node) == K::VariableDeclarationList
            && let Some(statement) = self.parent(node)
        {
            return self.kind(statement) == K::VariableStatement
                && self.has_modifier(statement, K::ExportKeyword);
        }
        false
    }

    /// `ast.GetRootDeclaration`: up through binding elements and patterns.
    fn root_declaration(&self, mut node: NodeId) -> NodeId {
        use tsr_ast::SyntaxKind as K;
        while self.kind(node) == K::BindingElement {
            match self.parent(node).and_then(|pattern| self.parent(pattern)) {
                Some(owner) => node = owner,
                None => break,
            }
        }
        node
    }

    /// `ast.GetDeclarationContainer`.
    fn declaration_container(&self, node: NodeId) -> Option<NodeId> {
        use tsr_ast::SyntaxKind as K;
        let mut current = self.root_declaration(node);
        while matches!(
            self.kind(current),
            K::VariableDeclaration
                | K::VariableDeclarationList
                | K::ImportSpecifier
                | K::NamedImports
                | K::NamespaceImport
                | K::ImportClause
        ) {
            current = self.parent(current)?;
        }
        self.parent(current)
    }

    fn source_file(&self, node: NodeId) -> Option<&'a tsr_ast::SourceFile<'a>> {
        match self.checker.node_map.get(node) {
            Some(tsr_ast::Node::SourceFile(file)) => Some(file),
            _ => None,
        }
    }

    /// `ast.IsGlobalSourceFile`.
    fn is_global_source_file(&self, node: NodeId) -> bool {
        self.source_file(node).is_some_and(|file| !tsr_binder::is_external_module(file))
    }

    /// `ast.IsAmbientModule`: a string-named module or `declare global`.
    fn is_ambient_module(&self, node: NodeId) -> bool {
        matches!(self.checker.node_map.get(node), Some(tsr_ast::Node::ModuleDeclaration(module))
            if matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
                || module.keyword.kind == tsr_ast::SyntaxKind::GlobalKeyword)
    }

    /// `ast.IsExternalModuleAugmentation` (`ast/utilities.go:3567`).
    fn is_external_module_augmentation(&self, node: NodeId) -> bool {
        use tsr_ast::SyntaxKind as K;
        if !self.is_ambient_module(node) {
            return false;
        }
        let Some(parent) = self.parent(node) else { return false };
        match self.kind(parent) {
            K::SourceFile => self.source_file(parent).is_some_and(tsr_binder::is_external_module),
            K::ModuleBlock => self.parent(parent).is_some_and(|grand| {
                self.is_ambient_module(grand)
                    && self.parent(grand).is_some_and(|file| self.is_global_source_file(file))
            }),
            _ => false,
        }
    }

    /// `EmitResolver.isDeclarationVisible` (`emitresolver.go:111`).
    pub fn is_declaration_visible(&mut self, node: NodeId) -> bool {
        if self.checker.nodes.flags(node).contains(tsr_ast::NodeFlags::SYNTHESIZED) {
            return false;
        }
        if let Some(&visible) = self.is_visible.get(&node) {
            return visible;
        }
        let visible = self.determine_if_declaration_is_visible(node);
        self.is_visible.insert(node, visible);
        visible
    }

    /// `EmitResolver.determineIfDeclarationIsVisible` (`emitresolver.go:131`).
    /// The JSDoc typedef arms are not reached: JavaScript files are not
    /// walked (`tsr_dts::accessibility`).
    fn determine_if_declaration_is_visible(&mut self, node: NodeId) -> bool {
        use tsr_ast::SyntaxKind as K;
        match self.kind(node) {
            K::BindingElement => self
                .parent(node)
                .and_then(|pattern| self.parent(pattern))
                .is_some_and(|owner| self.is_declaration_visible(owner)),
            K::VariableDeclaration
            | K::ModuleDeclaration
            | K::ClassDeclaration
            | K::InterfaceDeclaration
            | K::TypeAliasDeclaration
            | K::FunctionDeclaration
            | K::EnumDeclaration
            | K::ImportEqualsDeclaration => {
                if let Some(tsr_ast::Node::VariableDeclaration(declaration)) =
                    self.checker.node_map.get(node)
                    && let Some(tsr_ast::BindingName::BindingPattern(pattern)) = declaration.name
                    && pattern.elements.is_empty()
                {
                    return false;
                }
                if self.is_external_module_augmentation(node) {
                    return true;
                }
                let Some(parent) = self.declaration_container(node) else { return false };
                // Not exported, and not an ambient module element (an import
                // declaration excepted).
                let ambient_member = self.kind(node) != K::ImportEqualsDeclaration
                    && self.kind(parent) != K::SourceFile
                    && self.checker.nodes.flags(parent).contains(tsr_ast::NodeFlags::AMBIENT);
                if !self.has_combined_export(node) && !ambient_member {
                    return self.is_global_source_file(parent);
                }
                self.is_declaration_visible(parent)
            }
            K::PropertyDeclaration
            | K::PropertySignature
            | K::GetAccessor
            | K::SetAccessor
            | K::MethodDeclaration
            | K::MethodSignature => {
                if self.has_modifier(node, K::PrivateKeyword)
                    || self.has_modifier(node, K::ProtectedKeyword)
                {
                    return false;
                }
                self.parent(node).is_some_and(|parent| self.is_declaration_visible(parent))
            }
            K::Constructor
            | K::ConstructSignature
            | K::CallSignature
            | K::IndexSignature
            | K::Parameter
            | K::ModuleBlock
            | K::FunctionType
            | K::ConstructorType
            | K::TypeLiteral
            | K::TypeReference
            | K::ArrayType
            | K::TupleType
            | K::UnionType
            | K::IntersectionType
            | K::ParenthesizedType
            | K::NamedTupleMember => {
                self.parent(node).is_some_and(|parent| self.is_declaration_visible(parent))
            }
            K::TypeParameter | K::SourceFile | K::NamespaceExportDeclaration => true,
            K::ExportSpecifier => {
                let export = self.parent(node).and_then(|named| self.parent(named));
                match export.map(|id| (id, self.checker.node_map.get(id))) {
                    Some((id, Some(tsr_ast::Node::ExportDeclaration(declaration))))
                        if declaration.module_specifier.is_none() =>
                    {
                        self.parent(id).is_some_and(|parent| self.is_declaration_visible(parent))
                    }
                    _ => false,
                }
            }
            // Import clauses, namespace imports and specifiers are visible
            // only on demand; export assignments bind nothing outside.
            _ => false,
        }
    }

    /// `EmitResolver.PrecalculateDeclarationEmitVisibility`
    /// (`emitresolver.go:236`): mark what `export =`, `export default` and
    /// `export { … }` name as visible before the transform runs.
    pub fn precalculate_declaration_emit_visibility(&mut self, file: NodeId) {
        if !self.aliases_marked.insert(file) {
            return;
        }
        let mut stack = vec![file];
        while let Some(node) = stack.pop() {
            match self.checker.node_map.get(node) {
                Some(tsr_ast::Node::ExportAssignment(assignment)) => {
                    if let Some(tsr_ast::Expression::Identifier(identifier)) = assignment.expression
                        && let Some(id) = identifier.node_id
                    {
                        self.mark_linked_aliases(id, Some(identifier.text));
                    }
                }
                Some(tsr_ast::Node::ExportSpecifier(_)) => self.mark_linked_aliases(node, None),
                _ => {}
            }
            if let Some(typed) = self.checker.node_map.get(node) {
                tsr_ast::for_each_child_id(typed, |child| stack.push(child));
            }
        }
    }

    /// `EmitResolver.markLinkedAliases` (`emitresolver.go:278`). For an
    /// export specifier the target is `getTargetOfExportSpecifier`, here the
    /// specifier symbol's resolved alias. The `CommonJS` `module.exports =`
    /// arm is not reached (JavaScript files are not walked).
    fn mark_linked_aliases(&mut self, node: NodeId, export_assignment_name: Option<&str>) {
        let all =
            SymbolFlags::VALUE | SymbolFlags::TYPE | SymbolFlags::NAMESPACE | SymbolFlags::ALIAS;
        let mut export_symbol = match export_assignment_name {
            Some(name) => self.checker.resolve_name_with_export_alias(node, name, all),
            None => self
                .checker
                .binder
                .symbol_of(node)
                .and_then(|alias| self.checker.resolve_alias(alias)),
        };
        let mut visited = rustc_hash::FxHashSet::default();
        while let Some(symbol) = export_symbol {
            let symbol = self.checker.binder.merged_symbol(symbol);
            if !visited.insert(symbol) {
                break;
            }
            let mut next = None;
            let declarations = self.checker.binder.symbols().get(symbol).declarations.clone();
            for declaration in declarations {
                self.is_visible.insert(declaration, true);
                // `IsInternalModuleImportEqualsDeclaration`.
                if let Some(tsr_ast::Node::ImportEqualsDeclaration(import)) =
                    self.checker.node_map.get(declaration)
                    && let Some(reference) = import.module_reference
                    && !matches!(reference, tsr_ast::ModuleReference::ExternalModuleReference(_))
                {
                    let first = first_identifier_of_module_reference(reference);
                    next = first.and_then(|(_, text)| {
                        self.checker.resolve_name_with_export_alias(declaration, text, all)
                    });
                }
            }
            export_symbol = next;
        }
    }

    /// `ast.GetFirstIdentifier` over an entity name or entity-name
    /// expression, with its text.
    fn first_identifier(&self, mut node: NodeId) -> Option<(NodeId, &'a str)> {
        use tsr_ast::Node;
        loop {
            match self.checker.node_map.get(node)? {
                Node::Identifier(identifier) => return Some((node, identifier.text)),
                Node::QualifiedName(name) => node = name.left?.node_id()?,
                Node::PropertyAccessExpression(access) => node = access.expression?.node_id()?,
                _ => return None,
            }
        }
    }

    /// `getMeaningOfEntityNameReference` (`emitresolver.go:311`).
    fn meaning_of_entity_name_reference(&self, entity_name: NodeId) -> SymbolFlags {
        use tsr_ast::{Node, SyntaxKind as K};
        let parent = self.parent(entity_name);
        let parent_kind = parent.map(|p| self.kind(p));
        let is_value = match parent_kind {
            Some(K::TypeQuery | K::ComputedPropertyName | K::BinaryExpression) => true,
            Some(K::ExpressionWithTypeArguments) => {
                let tree = tsr_ast::Tree { nodes: self.checker.nodes, map: self.checker.node_map };
                !tsr_ast::predicates::is_part_of_type_node(parent.unwrap_or(entity_name), tree)
            }
            Some(K::TypePredicate) => matches!(
                parent.and_then(|p| self.checker.node_map.get(p)),
                Some(Node::TypePredicateNode(predicate))
                    if predicate.parameter_name.and_then(|n| n.node_id()) == Some(entity_name)
            ),
            _ => false,
        };
        if is_value {
            return SymbolFlags::VALUE | SymbolFlags::EXPORT_VALUE;
        }
        let left_of = |p: NodeId| match self.checker.node_map.get(p) {
            Some(Node::QualifiedName(name)) => {
                name.left.and_then(|n| n.node_id()) == Some(entity_name)
            }
            Some(Node::PropertyAccessExpression(access)) => {
                access.expression.and_then(|n| n.node_id()) == Some(entity_name)
            }
            Some(Node::ElementAccessExpression(access)) => {
                access.expression.and_then(|n| n.node_id()) == Some(entity_name)
            }
            _ => false,
        };
        if matches!(self.kind(entity_name), K::QualifiedName | K::PropertyAccessExpression)
            || parent_kind == Some(K::ImportEqualsDeclaration)
            || parent.is_some_and(left_of)
        {
            return SymbolFlags::NAMESPACE;
        }
        SymbolFlags::TYPE
    }

    /// `EmitResolver.isEntityNameVisible` (`emitresolver.go:340`) with
    /// `shouldComputeAliasToMakeVisible`.
    ///
    /// The `this` arm (`typeof this` resolved through the `this` container's
    /// symbol) answers `NotResolved` here; native answers `Accessible` or
    /// falls through to the same `NotResolved`, and neither reports.
    pub fn is_entity_name_visible(
        &mut self,
        entity_name: NodeId,
        enclosing_declaration: NodeId,
    ) -> EmitAccessibility {
        if self.checker.nodes.flags(entity_name).contains(tsr_ast::NodeFlags::SYNTHESIZED) {
            return EmitAccessibility::NotResolved;
        }
        let meaning = self.meaning_of_entity_name_reference(entity_name);
        let Some((first, text)) = self.first_identifier(entity_name) else {
            return EmitAccessibility::NotResolved;
        };
        let Some(symbol) =
            self.checker.resolve_name_with_export_alias(enclosing_declaration, text, meaning)
        else {
            return EmitAccessibility::NotResolved;
        };
        let symbol = self.checker.binder.merged_symbol(symbol);
        let flags = self.checker.binder.symbols().get(symbol).flags;
        if flags.contains(SymbolFlags::TYPE_PARAMETER) && meaning.intersects(SymbolFlags::TYPE) {
            return EmitAccessibility::Accessible { aliases_to_make_visible: Vec::new() };
        }
        match self.has_visible_declarations(symbol) {
            Some(aliases) => EmitAccessibility::Accessible { aliases_to_make_visible: aliases },
            None => EmitAccessibility::NotAccessible {
                error_symbol_name: text.to_string(),
                error_node: first,
            },
        }
    }

    /// `EmitResolver.hasVisibleDeclarations` (`emitresolver.go:384`) with
    /// `shouldComputeAliasToMakeVisible`: `None` is native's `nil`; the
    /// aliases come back in first-painted order (native collects a map).
    fn has_visible_declarations(&mut self, symbol: SymbolId) -> Option<Vec<NodeId>> {
        use tsr_ast::SyntaxKind as K;
        let record = self.checker.binder.symbols().get(symbol);
        let flags = record.flags;
        let declarations = record.declarations.clone();
        let mut aliases: Vec<(NodeId, NodeId)> = Vec::new();
        let mut add_visible_alias = |this: &mut Self, declaration: NodeId, statement: NodeId| {
            this.is_visible.insert(declaration, true);
            match aliases.iter_mut().find(|(d, _)| *d == declaration) {
                Some(entry) => entry.1 = statement,
                None => aliases.push((declaration, statement)),
            }
        };
        for declaration in declarations {
            let kind = self.kind(declaration);
            if kind == K::Identifier || self.is_declaration_visible(declaration) {
                continue;
            }
            // `getAnyImportSyntax` (`checker/utilities.go:1602`).
            let import = match kind {
                K::ImportEqualsDeclaration => Some(declaration),
                K::ImportClause => self.parent(declaration),
                K::NamespaceImport => self.parent(declaration).and_then(|p| self.parent(p)),
                K::ImportSpecifier => self
                    .parent(declaration)
                    .and_then(|p| self.parent(p))
                    .and_then(|p| self.parent(p)),
                _ => None,
            };
            if let Some(import) = import
                && !self.has_modifier(import, K::ExportKeyword)
                && self.parent(import).is_some_and(|parent| self.is_declaration_visible(parent))
            {
                add_visible_alias(self, declaration, import);
                continue;
            }
            if kind == K::VariableDeclaration
                && let Some(statement) = self.parent(declaration).and_then(|list| self.parent(list))
                && self.kind(statement) == K::VariableStatement
                && !self.has_modifier(statement, K::ExportKeyword)
                && self.parent(statement).is_some_and(|parent| self.is_declaration_visible(parent))
            {
                add_visible_alias(self, declaration, statement);
                continue;
            }
            // `ast.IsLateVisibilityPaintedStatement` (`ast/utilities.go:3548`).
            if matches!(
                kind,
                K::ImportDeclaration
                    | K::ImportEqualsDeclaration
                    | K::VariableStatement
                    | K::ClassDeclaration
                    | K::FunctionDeclaration
                    | K::ModuleDeclaration
                    | K::TypeAliasDeclaration
                    | K::InterfaceDeclaration
                    | K::EnumDeclaration
            ) && !self.has_modifier(declaration, K::ExportKeyword)
                && self
                    .parent(declaration)
                    .is_some_and(|parent| self.is_declaration_visible(parent))
            {
                add_visible_alias(self, declaration, declaration);
                continue;
            }
            if kind == K::BindingElement && flags.contains(SymbolFlags::BLOCK_SCOPED_VARIABLE) {
                // The JavaScript `require` arm is not reached: JavaScript
                // files are not walked.
                let root = self.root_declaration(declaration);
                if self.kind(root) == K::Parameter {
                    return None;
                }
                let statement = self.parent(root).and_then(|list| self.parent(list))?;
                if self.kind(statement) != K::VariableStatement {
                    return None;
                }
                if self.has_modifier(statement, K::ExportKeyword) {
                    continue;
                }
                if !self.parent(statement).is_some_and(|parent| self.is_declaration_visible(parent))
                {
                    return None;
                }
                add_visible_alias(self, declaration, statement);
                continue;
            }
            return None;
        }
        Some(aliases.into_iter().map(|(_, statement)| statement).collect())
    }

    /// `EmitResolver.IsImplementationOfOverload` (`emitresolver.go:463`):
    /// a body-carrying function-like declaration whose symbol has more than
    /// one signature, or one that is not this declaration.
    /// `getSignaturesOfSymbol` is read as the symbol's function-like
    /// declarations, one signature each, which is what it computes for the
    /// TypeScript files this resolver is asked about.
    pub fn is_implementation_of_overload(&mut self, node: NodeId) -> bool {
        use tsr_ast::{Node, SyntaxKind as K};
        let has_body = match self.checker.node_map.get(node) {
            Some(Node::FunctionDeclaration(n)) => n.body.is_some(),
            Some(Node::MethodDeclaration(n)) => n.body.is_some(),
            Some(Node::ConstructorDeclaration(n)) => n.body.is_some(),
            _ => return false,
        };
        if !has_body {
            return false;
        }
        let Some(symbol) = self.checker.binder.symbol_of(node) else { return false };
        let symbol = self.checker.binder.merged_symbol(symbol);
        let signatures: Vec<NodeId> = self
            .checker
            .binder
            .symbols()
            .get(symbol)
            .declarations
            .iter()
            .copied()
            .filter(|&d| {
                matches!(
                    self.kind(d),
                    K::FunctionDeclaration
                        | K::MethodDeclaration
                        | K::MethodSignature
                        | K::Constructor
                        | K::FunctionExpression
                        | K::ArrowFunction
                )
            })
            .collect();
        signatures.len() > 1 || (signatures.len() == 1 && signatures[0] != node)
    }

    /// The `SymbolTracker` reports the node builder makes while serializing
    /// the **inferred** type of `node` for declaration emit, for the arms this
    /// port reaches (`docs/parity/notes/r5-declemit2.md` §2).
    ///
    /// `node` is what the transform hands the node builder:
    /// `CreateTypeOfDeclaration` for a variable declaration or an
    /// `export default <expr>` (`transform.go:1667`), or
    /// `CreateTypeOfExpression` for a class's `extends <expr>`
    /// (`transform.go:2018`), whose type is
    /// `getWidenedType(getRegularTypeOfExpression(expr))`
    /// (`emitresolver.go`). An object type is its own widened and regular
    /// form, and the one arm below only reads object types.
    ///
    /// # The arm: a class expression written as a type literal
    ///
    /// Declaration emit builds with `FlagsWriteClassExpressionAsTypeLiteral`
    /// (`transform.go:216`). `createAnonymousTypeNode`
    /// (`nodebuilderimpl.go:2805`) then expands the static side of a class
    /// whose value declaration is a class *expression* — unconditionally, the
    /// `!IsClassDeclaration` disjunct — and `createTypeNodesFromResolvedType`
    /// (`:2660`) reports each property that is private, protected or
    /// `#private` (`ReportPrivateInBaseOfClassExpression`). The static side's
    /// construct signature returns the instance type, which `typeToTypeNode`
    /// (`:3047`) expands too because a class expression's symbol is never
    /// value-accessible outside its own body, so its properties report as well.
    ///
    /// Only the **top-level** type is read: a class expression nested inside a
    /// union, a property type or a signature is reached by native's
    /// recursion and not here, so those errors are missed, never invented.
    /// The same holds for a class *declaration* whose name is not accessible
    /// (the `IsSymbolAccessible` disjunct), which needs
    /// `getAccessibleSymbolChain`.
    pub fn inferred_type_reports(&mut self, node: NodeId) -> Vec<TrackerReport> {
        use tsr_ast::Node;
        let ty = match self.checker.node_map.get(node) {
            Some(Node::VariableDeclaration(_)) => {
                let Some(symbol) = self.checker.binder.symbol_of(node) else { return Vec::new() };
                self.checker.get_type_of_symbol(symbol)
            }
            Some(Node::ExportAssignment(assignment)) => {
                let Some(expression) = assignment.expression else { return Vec::new() };
                self.checker.check_expression(expression)
            }
            Some(Node::ExpressionWithTypeArguments(heritage)) => {
                let Some(expression) = heritage.expression else { return Vec::new() };
                self.checker.check_expression(expression)
            }
            _ => return Vec::new(),
        };
        let mut reports = Vec::new();
        self.unsafe_import_reports(ty, node, &mut reports);
        let crate::types::TypeData::Anonymous { symbol: class, .. } =
            self.checker.store.get(ty).data
        else {
            return reports;
        };
        let symbols = self.checker.binder.symbols().get(class);
        // `symbol.ValueDeclaration`. This port's binder leaves it unset on a
        // class expression's symbol, whose one declaration is the expression.
        let value_declaration =
            symbols.value_declaration.or_else(|| symbols.declarations.first().copied());
        let is_class_expression =
            value_declaration.is_some_and(|d| self.kind(d) == tsr_ast::SyntaxKind::ClassExpression);
        if !symbols.flags.contains(SymbolFlags::CLASS) || !is_class_expression {
            return reports;
        }
        // The instance side first: the construct signature precedes the
        // static properties in `createTypeNodesFromResolvedType`.
        let instance = self.checker.get_declared_type_of_symbol(class);
        self.report_private_properties(instance, &mut reports);
        self.report_private_properties(ty, &mut reports);
        reports
    }

    /// `ReportLikelyUnsafeImportRequiredError` (`nodebuilderimpl.go:681`-`:710`)
    /// over the inferred type `ty` of `node`.
    ///
    /// The type is printed with the checker's specifier tracker on
    /// (`Checker::unsafe_import_tracker`, filled where `symbol_chain`
    /// generates an `import("…")` specifier into `node_modules`); each entry
    /// is then put through the node builder's two escapes under `node16` /
    /// `nodenext` resolution. A target emitted as ESM from a file of another
    /// format already carries a `resolution-mode` attribute and is no error;
    /// otherwise the specifier is generated again in the swapped mode, and a
    /// result outside `node_modules` is written with the attribute instead.
    /// What remains is reported, once per (specifier, symbol). Only the
    /// qualified `import("…").T` route is tracked: the module-object route
    /// (`typeof import("…")`, `symbol` = the module itself) is not, so its
    /// reports are missed, never invented. r5-modules §6.
    fn unsafe_import_reports(
        &mut self,
        ty: TypeId,
        node: NodeId,
        reports: &mut Vec<TrackerReport>,
    ) {
        use tsr_core::ModuleKind;
        // A program with no `node_modules` file generates no such specifier;
        // skip the serialization (r5-modules §6).
        if self.checker.module_host.is_none_or(|host| !host.has_node_modules_files()) {
            return;
        }
        let previous = self.checker.unsafe_import_tracker.replace(Vec::new());
        let _ = self.checker.type_to_string_at(ty, node);
        let found = std::mem::replace(&mut self.checker.unsafe_import_tracker, previous)
            .unwrap_or_default();
        if found.is_empty() {
            return;
        }
        let Some(host) = self.checker.module_host else { return };
        let node_next = host.specifier_options(ModuleKind::None).module_resolution_is_node_next;
        let context_file = self.checker.source_file_of(node);
        for entry in found {
            if node_next && let Some(context_file) = context_file {
                let format = |file: NodeId| host.implied_node_format_for_emit(file);
                let target_file = self
                    .checker
                    .binder
                    .symbols()
                    .get(entry.module)
                    .declarations
                    .iter()
                    .copied()
                    .find(|&declaration| self.kind(declaration) == tsr_ast::SyntaxKind::SourceFile);
                if target_file.is_some_and(|target| {
                    format(target) == ModuleKind::ESNext && format(target) != format(context_file)
                }) {
                    continue;
                }
                let swapped = if format(context_file) == ModuleKind::ESNext {
                    ModuleKind::CommonJS
                } else {
                    ModuleKind::ESNext
                };
                if self
                    .checker
                    .module_specifier_for_symbol_in_mode(entry.module, node, swapped)
                    .is_some_and(|specifier| !specifier.contains("/node_modules/"))
                {
                    continue;
                }
            }
            reports.push(TrackerReport::LikelyUnsafeImportRequired {
                specifier: entry.specifier,
                symbol_name: entry.symbol_name,
            });
        }
    }

    /// The `FlagsWriteClassExpressionAsTypeLiteral` arm of
    /// `createTypeNodesFromResolvedType` (`nodebuilderimpl.go:2660`) over
    /// `ty`'s resolved properties. An unenumerable property list declines.
    fn report_private_properties(&mut self, ty: TypeId, reports: &mut Vec<TrackerReport>) {
        use tsr_ast::SyntaxKind as K;
        let Some(names) = self.checker.get_property_names_of_type(ty) else { return };
        for name in names {
            let Some(property) = self.checker.get_property_of_type(ty, &name) else { continue };
            let symbol = self.checker.binder.symbols().get(property);
            if symbol.flags.contains(SymbolFlags::PROTOTYPE) {
                continue;
            }
            // `getDeclarationModifierFlagsFromSymbol`: the value
            // declaration's combined modifier flags.
            let declaration =
                symbol.value_declaration.or_else(|| symbol.declarations.first().copied());
            if let Some(declaration) = declaration {
                if self.has_modifier(declaration, K::PrivateKeyword)
                    || self.has_modifier(declaration, K::ProtectedKeyword)
                {
                    reports.push(TrackerReport::PrivateInBaseOfClassExpression(name.clone()));
                }
                // `IsPrivateIdentifierSymbol`, reported under `SymbolName`.
                if self.checker.is_private_identifier_class_element_declaration(declaration) {
                    reports.push(TrackerReport::PrivateInBaseOfClassExpression(name));
                }
            }
        }
    }

    /// `EmitResolver.IsImportRequiredByAugmentation` (`emitresolver.go:504`):
    /// the imported file augments one of this file's own exports.
    ///
    /// Native asks whether `getMergedSymbol(s) != s` for each symbol of the
    /// *parse-tree* module symbol's `getExportsOfModule`, then whether the
    /// merged symbol has a declaration in the import's target file. This
    /// port's augmentation merge (`merge_module_augmentations`) unions into
    /// the target symbol in place, so "merged into" reads as: an export
    /// declared in this file that also carries a declaration from the target.
    /// The "declared in this file" half is what native's original table
    /// guarantees: a name the augmentation *adds* lives only in native's
    /// merged clone, never in `file.Symbol.exports`. `export *` re-exports
    /// are not walked; upstream reaches them through `getExportsOfModule`
    /// but they are only "merged" when augmented, so this declines rather
    /// than invents.
    pub fn is_import_required_by_augmentation(&mut self, node: NodeId) -> bool {
        let Some(tsr_ast::Node::ImportDeclaration(import)) = self.checker.node_map.get(node) else {
            return false;
        };
        let Some(file) = self.checker.source_file_of(node) else { return false };
        // A script file has no module symbol.
        let Some(module) = self.checker.binder.symbol_of(file) else { return false };
        // `GetExternalModuleFileFromDeclaration`: the resolved module's
        // source-file declaration.
        let Some(specifier) = import.module_specifier.and_then(|s| s.node_id()) else {
            return false;
        };
        let Some(target) = self.checker.resolve_external_module_name(node, specifier) else {
            return false;
        };
        let target = self.checker.binder.merged_symbol(target);
        let Some(target_file) = self
            .checker
            .binder
            .symbols()
            .get(target)
            .declarations
            .iter()
            .copied()
            .find(|&d| self.kind(d) == tsr_ast::SyntaxKind::SourceFile)
        else {
            return false;
        };
        if target_file == file {
            return false;
        }
        let symbols = self.checker.binder.symbols();
        symbols.get(module).exports.values().any(|&export| {
            let declarations = &symbols.get(export).declarations;
            let file_of = |d: NodeId| self.checker.source_file_of(d);
            declarations.iter().any(|&d| file_of(d) == Some(file))
                && declarations.iter().any(|&d| file_of(d) == Some(target_file))
        })
    }
}

fn first_identifier_of_module_reference(
    reference: tsr_ast::ModuleReference<'_>,
) -> Option<(NodeId, &str)> {
    let mut name = match reference {
        tsr_ast::ModuleReference::Identifier(identifier) => {
            return Some((identifier.node_id?, identifier.text));
        }
        tsr_ast::ModuleReference::QualifiedName(name) => name,
        tsr_ast::ModuleReference::ExternalModuleReference(_) => return None,
    };
    loop {
        match name.left? {
            tsr_ast::EntityName::Identifier(identifier) => {
                return Some((identifier.node_id?, identifier.text));
            }
            tsr_ast::EntityName::QualifiedName(left) => name = left,
        }
    }
}
