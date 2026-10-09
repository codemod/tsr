//! TS2300 for late-bound members: the two steps of
//! `getResolvedMembersOrExportsOfSymbol` (`checker.go:15930`) that
//! `check_object_type_for_duplicate_declarations` (`check.rs`) replays over
//! one declaration's members, `lateBindMember`'s conflict report and
//! `combineSymbolTables`' merge of the late-bound table into the early one.
//!
//! `docs/parity/notes/r5-smallcodes2.md` §2.3.

use rustc_hash::FxHashMap;
use tsr_ast::NodeId;
use tsr_binder::{SymbolFlags, SymbolId};

use crate::check::DuplicateMemberKey;
use crate::checker::Checker;

impl Checker<'_, '_> {
    /// TS2300 from `lateBindMember` (`checker.go:16005`): a late-bound member
    /// whose flags the late-bound symbol of its name already excludes
    /// (`getExcludedSymbolFlags`, `:16035`) is reported on every declaration
    /// of that symbol, on the early-bound member of the same name if there is
    /// one, and on itself. The new declaration then goes to a fresh symbol that
    /// is not entered in the table, so a third conflicting declaration is
    /// reported against the first again. The name printed is the property
    /// name, or for a unique symbol the conflicting declaration's written name.
    ///
    /// `getResolvedMembersOrExportsOfSymbol` (`:15930`) binds instance members
    /// into one table and static members into the exports table; `entries`
    /// and `keys` are `check_object_type_for_duplicate_declarations`'s, one
    /// declaration's members in source order. Upstream walks every declaration
    /// of the merged symbol; a conflict between two merged declarations is
    /// not seen here. `addDeclarationToLateBoundSymbol`'s replaceable-by-method
    /// arm only applies to JavaScript assignment declarations, which
    /// `entries` never holds. `docs/parity/notes/r5-smallcodes2.md` §2.3.
    pub(crate) fn late_bound_member_conflicts(
        &self,
        entries: &[(NodeId, SymbolId, u8, bool)],
        keys: &[DuplicateMemberKey],
    ) -> Vec<(NodeId, String)> {
        // (late-bound symbol flags, its declarations' name nodes), per table.
        let mut tables: [FxHashMap<&DuplicateMemberKey, (SymbolFlags, Vec<NodeId>)>; 2] =
            Default::default();
        let mut reports = Vec::new();
        for (&(name_node, _, _, is_static), key) in entries.iter().zip(keys) {
            let DuplicateMemberKey::Late(member_name, unique) = key else { continue };
            let Some(member) = self.nodes.parent(name_node) else { continue };
            let Some(own) = self.binder.symbol_of(member) else { continue };
            let flags = self.binder.symbols().get(own).flags;
            let late = tables[usize::from(is_static)].entry(key).or_default();
            if !late.0.intersects(flags.excludes()) {
                late.0 |= flags;
                late.1.push(name_node);
                continue;
            }
            let name = if unique.is_some() {
                let Some(written) = self.overload_name_to_string(name_node) else { continue };
                written
            } else {
                member_name.clone()
            };
            // `earlySymbols[memberName]`: a unique symbol's member name is an
            // internal `__@…` name no early member has.
            if unique.is_none() {
                for (&(_, symbol, _, early_static), early_key) in entries.iter().zip(keys) {
                    if early_static != is_static
                        || !matches!(early_key, DuplicateMemberKey::Early(_))
                    {
                        continue;
                    }
                    let early = self.binder.symbols().get(symbol);
                    if early.name != member_name.as_str() {
                        continue;
                    }
                    for &declaration in &early.declarations {
                        let at = self.declaration_name_of(declaration).unwrap_or(declaration);
                        reports.push((at, name.clone()));
                    }
                    break;
                }
            }
            for &declared in &late.1 {
                reports.push((declared, name.clone()));
            }
            reports.push((name_node, name));
            if late.0.intersects(SymbolFlags::ACCESSOR)
                && (late.0 & SymbolFlags::ACCESSOR) != (flags & SymbolFlags::ACCESSOR)
            {
                late.0 |= SymbolFlags::ACCESSOR;
            }
        }
        reports
    }

    /// `combineSymbolTables(earlySymbols, lateSymbols)` (`checker.go:15974`)
    /// merges a late-bound symbol into the early-bound one of the same name,
    /// so `[c0]` with `c0 = "1"` and a member `1` are one symbol of two
    /// declarations, named `1`. Rewrites each such late key to the early
    /// symbol's and answers how many late declarations each early symbol (per
    /// static-ness) gained.
    pub(crate) fn merge_late_bound_into_early(
        &self,
        entries: &[(NodeId, SymbolId, u8, bool)],
        keys: &mut [DuplicateMemberKey],
    ) -> FxHashMap<(SymbolId, bool), usize> {
        let mut merged_late: FxHashMap<(SymbolId, bool), usize> = FxHashMap::default();
        for index in 0..keys.len() {
            let DuplicateMemberKey::Late(name, None) = &keys[index] else { continue };
            let is_static = entries[index].3;
            let early =
                entries.iter().zip(keys.iter()).find_map(|(&(_, symbol, _, early_static), key)| {
                    (early_static == is_static
                        && matches!(key, DuplicateMemberKey::Early(_))
                        && self.binder.symbols().get(symbol).name == name.as_str())
                    .then_some(symbol)
                });
            if let Some(symbol) = early {
                *merged_late.entry((symbol, is_static)).or_default() += 1;
                keys[index] = DuplicateMemberKey::Early(symbol);
            }
        }
        merged_late
    }
}
