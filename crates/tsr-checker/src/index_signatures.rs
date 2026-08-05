//! Index signatures: `{ [k: string]: number }`, and what `a[i]` means when `i`
//! is not a literal.
//!
//! Ported from `Checker.getIndexInfosOfType` (`checker.go:18974`),
//! `getApplicableIndexInfo` (`:19008`) and `findApplicableIndexInfo`, with the
//! `indexInfos` themselves read off the declarations that `resolveDeclaredMembers`
//! reads them off upstream.
//!
//! # The asymmetry, taken from upstream rather than guessed
//!
//! `isApplicableIndexType` (`checker.go:19040`) is **not symmetric**, and getting
//! it backwards looks right on half the corpus:
//!
//! - a **string** index signature applies to a `string` key *and to a `number`
//!   key* — `{ [k: string]: T }` answers `a[0]`;
//! - a **number** index signature applies only to a `number` key, and to a
//!   string literal that is a numeric name (`a["0"]`), never to `string`.
//!
//! And `findApplicableIndexInfo` adds a precedence rule on top: *"index
//! signatures for type `string` are considered only when no other index
//! signature applies"*, so a type with both answers a numeric access from the
//! **number** signature.
//!
//! # What is not ported
//!
//! - **`noUncheckedIndexedAccess`.** An index signature does not make a property
//!   optional and does not add `| undefined`; that is a compiler option this port
//!   does not read (`bd tsr-y5a`). The two are deliberately not blended.
//! - **Applicability by assignability.** Upstream asks `isTypeAssignableTo`, and
//!   there is no relation here. The key shapes this port can *produce* are
//!   decided structurally instead — see [`Checker::is_applicable_index_type`] —
//!   and any other key is a gap rather than a guess.
//! - **Merging several applicable signatures** into a synthetic `IndexInfo` over
//!   the intersection of their value types (`findApplicableIndexInfo`'s
//!   `default` arm). Two applicable signatures is a gap.
//! - Index signatures on a **class**, on a mapped type, and inherited ones.

use tsr_ast::{Node, TypeElement};
use tsr_binder::SymbolId;

use crate::{
    checker::Checker,
    types::{TypeData, TypeId},
};

/// One index signature, reduced to what a lookup needs.
///
/// Upstream's `IndexInfo` (`types.go`) also carries `isReadonly` and the
/// declaration; neither changes the type an access yields, and `readonly` is only
/// consulted by assignment checking, which does not exist here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexInfo {
    /// `keyType` — `stringType` or `numberType`. Upstream permits `symbol` and
    /// pattern literal keys too; both are gaps here.
    pub key: TypeId,
    /// `valueType`, what an applicable access yields.
    pub value: TypeId,
}

impl<'a> Checker<'a, '_> {
    /// The index signatures a type declares.
    ///
    /// Ported from `getIndexInfosOfType` (`checker.go:18974`) into
    /// `resolveStructuredTypeMembers`, reduced to reading the
    /// `IndexSignatureDeclaration` members off the declarations of the symbol the
    /// type is named by.
    ///
    /// Only a type with a members table has any — that is
    /// [`TypeData::Named`]'s `members`, the same field property access reads, and
    /// for the same reason: it is the one table the binder already built. An
    /// intrinsic, a literal and an anonymous function type have none.
    pub(crate) fn get_index_infos_of_type(&mut self, id: TypeId) -> Vec<IndexInfo> {
        let TypeData::Named { members: Some(owner), .. } = self.store.get(id).data else {
            return Vec::new();
        };
        self.index_infos_of_symbol(owner)
    }

    /// The index signatures declared on a symbol's own declarations.
    ///
    /// Base types are **not** followed. Upstream's `resolveObjectTypeMembers`
    /// layers a base's index signatures under the derived type's, so an
    /// inherited one is a real answer this port misses — a gap, not a wrong
    /// answer, and it belongs with whoever owns base-type walking in
    /// `members.rs`.
    fn index_infos_of_symbol(&mut self, owner: SymbolId) -> Vec<IndexInfo> {
        let declarations = self.binder.symbols().get(owner).declarations.clone();
        let mut infos = Vec::new();
        for declaration in declarations {
            let members: &[TypeElement<'a>] = match self.node_map.get(declaration) {
                Some(Node::InterfaceDeclaration(node)) => node.members,
                Some(Node::TypeLiteralNode(node)) => node.members,
                _ => continue,
            };
            for member in members {
                let TypeElement::IndexSignatureDeclaration(signature) = member else { continue };
                let Some(info) = self.index_info_of(signature) else { continue };
                infos.push(info);
            }
        }
        infos
    }

    /// One `[k: K]: V` member, or `None` when either half is a gap.
    fn index_info_of(
        &mut self,
        signature: &tsr_ast::IndexSignatureDeclaration<'a>,
    ) -> Option<IndexInfo> {
        let [parameter] = signature.parameters else { return None };
        let key = self.get_type_from_type_node(parameter.r#type?);
        let value = self.get_type_from_type_node(signature.r#type?);
        // Upstream permits a `symbol` key and pattern literal keys; this port
        // answers only the two the corpus is overwhelmingly made of, because
        // applicability for the others needs the relation.
        if key != self.intrinsics.string && key != self.intrinsics.number {
            return None;
        }
        (value != self.intrinsics.error).then_some(IndexInfo { key, value })
    }

    /// One `[k: K]: V` member rendered for printing, or `None` when it is a gap.
    ///
    /// Ported from `indexInfoToIndexSignatureDeclarationHelper`
    /// (`nodebuilderimpl.go:2138`), which takes the bracketed parameter's name
    /// from the *declaration* — upstream's `IndexInfo` does not carry one — and
    /// that is why the printed name is whatever was written: baselines record
    /// both `[key: string]: string` and `[x: string]: unknown`.
    ///
    /// The gaps are deliberately the **same** ones [`Checker::index_info_of`]
    /// takes, so a literal cannot print an index signature that a subsequent
    /// `a[i]` lookup then fails to find:
    ///
    /// - a key that is not the `string` or `number` intrinsic. `[k: string | number]`
    ///   is upstream *two* index infos rather than one printed with a union key
    ///   (`getIndexInfosOfIndexSymbol` splits it), so printing it whole would be
    ///   a confident wrong answer;
    /// - a value type this port cannot compute;
    /// - a binding pattern where the parameter name should be, which the grammar
    ///   forbids but the AST permits.
    pub(crate) fn index_signature_member(
        &mut self,
        signature: &tsr_ast::IndexSignatureDeclaration<'a>,
    ) -> Option<crate::objects::Member> {
        let [parameter] = signature.parameters else { return None };
        let Some(tsr_ast::BindingName::Identifier(name)) = parameter.name else { return None };
        let key = self.get_type_from_type_node(parameter.r#type?);
        if key != self.intrinsics.string && key != self.intrinsics.number {
            return None;
        }
        let value = self.get_type_from_type_node(signature.r#type?);
        if value == self.intrinsics.error {
            return None;
        }
        let readonly = signature.modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token)
                if token.kind == tsr_ast::SyntaxKind::ReadonlyKeyword)
        });
        Some(crate::objects::Member::Index {
            readonly,
            name: name.text.to_string(),
            key: self.type_to_string(key),
            value: self.type_to_string(value),
        })
    }

    /// The index signature that applies to `key`, if exactly one does.
    ///
    /// Ported from `findApplicableIndexInfo` (`checker.go:19019`), including its
    /// precedence rule: **a `string` index signature is considered only when no
    /// other one applies**, so `{ [k: string]: A; [k: number]: B }` answers a
    /// numeric access with `B`.
    ///
    /// Upstream's `default` arm merges several applicable signatures into a
    /// synthetic `IndexInfo` over the intersection of their value types. That
    /// needs intersections, so two applicable signatures is a gap here.
    pub(crate) fn get_applicable_index_info(
        &mut self,
        id: TypeId,
        key: TypeId,
    ) -> Option<IndexInfo> {
        let infos = self.get_index_infos_of_type(id);
        let string_info = infos.iter().find(|info| info.key == self.intrinsics.string).copied();
        let applicable: Vec<IndexInfo> = infos
            .iter()
            .filter(|info| info.key != self.intrinsics.string)
            .filter(|info| self.is_applicable_index_type(key, info.key))
            .copied()
            .collect();
        match applicable.as_slice() {
            [] => {
                string_info.filter(|_| self.is_applicable_index_type(key, self.intrinsics.string))
            }
            [info] => Some(*info),
            _ => None,
        }
    }

    /// Ported from `isApplicableIndexType` (`checker.go:19040`), decided
    /// structurally because there is no assignability relation here.
    ///
    /// Upstream asks `isTypeAssignableTo(source, target)` and then adds two
    /// special cases. For the key shapes this port can produce — the `string` and
    /// `number` intrinsics and their literal types — assignability is decidable
    /// by inspection, and **every other key is a gap**: `None` from the caller
    /// rather than a guess, because a wrong index type yields a confident wrong
    /// value type.
    fn is_applicable_index_type(&self, source: TypeId, target: TypeId) -> bool {
        let (string, number) = (self.intrinsics.string, self.intrinsics.number);
        let source_is_number =
            source == number || matches!(self.store.get(source).data, TypeData::NumberLiteral(_));
        let source_is_string =
            source == string || matches!(self.store.get(source).data, TypeData::StringLiteral(_));
        if target == string {
            // *"A `string` index signature applies to types assignable to
            // `string` **or `number`**"* — the half that is easy to drop, and
            // dropping it loses every `a[0]` on a string-indexed type.
            return source_is_string || source_is_number;
        }
        if target == number {
            // The reverse does **not** hold: a `number` index signature never
            // applies to a `string` key. It does apply to a string *literal*
            // that spells a number, which is upstream's `isNumericLiteralName`.
            if source_is_number {
                return true;
            }
            return match &self.store.get(source).data {
                TypeData::StringLiteral(value) => is_numeric_literal_name(value),
                _ => false,
            };
        }
        false
    }
}

/// Ported from `isNumericLiteralName` (`checker.go`): a name is numeric when
/// converting it to a number and back gives the same string.
///
/// That round-trip is the definition rather than a shortcut — it is what makes
/// `"0"` numeric and `"00"`, `"1.0"` and `" 1"` not, all of which are distinct
/// property names.
fn is_numeric_literal_name(name: &str) -> bool {
    let Ok(value) = name.parse::<f64>() else { return false };
    crate::printing::normalise_number(name) == name && value.is_finite()
}
