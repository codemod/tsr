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
