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
//!   `default` arm). Two applicable signatures is a gap. **The base-type walk
//!   did not make this fall out for free**, and it is worth saying why rather
//!   than letting the absence pass: inheritance layers by *key type* and
//!   shadows on collision (`checker.go:19149`), so it can never hand
//!   `findApplicableIndexInfo` two signatures with the same key. The two
//!   applicable signatures that need merging come from one type declaring both
//!   `[k: string]` and `[k: number]`, which was already reachable before the
//!   walk existed. Untouched, still gapped.
//! - Index signatures on a **class** and on a mapped type.
//!
//! **Inherited** index signatures *are* ported — see
//! [`Checker::index_infos_of_symbol`].

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
    ///
    /// **`None` is a gap, `Some(vec![])` is "none declared".** The two are not
    /// the same claim: a type whose base this port cannot follow might have an
    /// index signature we cannot see, and answering "no index signatures" for it
    /// would turn a missing answer into a confident wrong one the moment a
    /// caller acts on the emptiness. Today's only caller collapses both to no
    /// answer, which is why the distinction has to be carried here rather than
    /// discovered later.
    pub(crate) fn get_index_infos_of_type(&mut self, id: TypeId) -> Option<Vec<IndexInfo>> {
        // §262. An ENUM's object type is `TypeData::Anonymous`, not `Named`, so
        // it returned empty here before the collector was ever asked — proven
        // by probe: `index_infos_of_symbol` is invoked ZERO times on
        // `compiler/indexIntoEnum`. §261 synthesised the signature in the
        // collector and measured a clean `+0` for exactly that reason; the
        // repair is routing, and the synthesis is its second half.
        //
        // Upstream's enum object carries an implicit numeric index signature
        // returning `string` — the REVERSE MAPPING, where `E[0]` is the member
        // NAME rather than a member.
        //
        //     namespace M { enum E { } var x = E[0]; }
        //     >E[0] : string
        //
        // Handled HERE rather than by widening the `Named`/`Anonymous` split,
        // which the type model keeps apart on purpose: `Named.members` is where
        // `getPropertyOfType` looks, and `Anonymous.symbol` carries the
        // declarations a call reads signatures from. Routing every anonymous
        // type into the members collector would make `typeof C` offer a class's
        // INSTANCE members, which is the wrong answer rather than a missing one
        // (see `TypeData`'s note on why the two fields are separate).
        //
        // `CONST_ENUM` is excluded: it has no runtime object, so upstream mints
        // no reverse mapping for it.
        if let TypeData::Anonymous { symbol, .. } = self.store.get(id).data
            && self
                .binder
                .symbols()
                .get(symbol)
                .flags
                .intersects(tsr_binder::SymbolFlags::REGULAR_ENUM)
        {
            return Some(vec![IndexInfo {
                key: self.intrinsics.number,
                value: self.intrinsics.string,
            }]);
        }
        // §539: an OBJECT LITERAL's index signature is minted at check time
        // and lives in a side table, because the literal's `__object` symbol
        // has no index-signature declaration for `index_infos_of_symbol` to
        // find. Consulted before the symbol road and never after it: a literal
        // that minted one has no declared signatures to merge with.
        if let Some(infos) = self.object_literal_index_infos.get(&id) {
            return Some(infos.clone());
        }
        // §785: the global `Record<K, V>` with a PRIMITIVE key carries an
        // index signature `[k: K]: V`.
        //
        // `Record<K, V>` is `{ [P in K]: V }`, and upstream reaches this
        // through `resolveMappedTypeMembers` (`checker.go`), which walks
        // `getLowerBoundOfKeyType(constraintType)` and — for a key that is not
        // usable as a property name, i.e. exactly `string`/`number`/`symbol` —
        // creates an INDEX INFO rather than a property. Mapped-type member
        // resolution is not ported (see this module's gap list above), so the
        // one alias the corpus actually leans on is special-cased here, the
        // same scoping decision §45 already made for the PROPERTY road in
        // `Checker::record_string_value` — and this is the half that road
        // could not cover, because `m[i]` never asks for a property.
        //
        // Head case `compiler/temporal`: four sites spelled
        // `monthsByDays[zdt.daysInMonth]` over
        // `Record<number, Temporal.ZonedDateTime[]>`, whose `any` cascades
        // into the `Array<T>` members read off each one — 339 wrong lines in a
        // case that is otherwise 6,258 RIGHT.
        //
        // Restricted to `string` and `number` on purpose. A literal-union key
        // (`Record<"a" | "b", V>`) must produce PROPERTIES, not an index
        // signature, and handing one back here would make `r.c` answer `V`
        // where upstream errors — a confident wrong answer in place of a
        // missing one.
        if let Some(info) = self.record_index_info(id) {
            return Some(vec![info]);
        }
        let TypeData::Named { members: Some(owner), .. } = self.store.get(id).data else {
            return Some(Vec::new());
        };
        let mut visiting = Vec::new();
        let infos = self.index_infos_of_symbol(owner, &mut visiting)?;
        // On an instantiated reference the declared value types are the
        // target's uninstantiated ones — `Array<string>`'s `[n: number]: T`
        // must answer `string`, not `T`. Same seam rule as
        // `get_type_of_property_of_type` (`crate::members`), for the same
        // reason, and a value that cannot be rebuilt becomes `errorType` — the
        // access stays a gap rather than answering the type parameter.
        // `bd tsr-4qx`.
        Some(
            infos
                .into_iter()
                .map(|info| IndexInfo {
                    key: info.key,
                    value: self.instantiate_for_reference(id, info.value),
                })
                .collect(),
        )
    }

    /// A symbol's own index signatures, then its base types', in that order.
    ///
    /// Ported from the base-type loop in `resolveObjectTypeMembers`
    /// (`checker.go:19149`), whose rule is a **shadow by key type**, not a
    /// merge:
    ///
    /// ```go
    /// indexInfos = core.Concatenate(indexInfos, core.Filter(inheritedIndexInfos,
    ///     func(info *IndexInfo) bool { return findIndexInfo(indexInfos, info.keyType) == nil }))
    /// ```
    ///
    /// So a derived `[k: string]: A` hides a base's `[k: string]: B` entirely —
    /// the two are never combined — while a base's `[k: number]` survives beside
    /// it. Same shape as `addInheritedMembers` for properties, and the same walk
    /// [`Checker::get_property_of_declared_symbol`] already does.
    ///
    /// # The gaps, and why each is `None` rather than an empty list
    ///
    /// [`Checker::base_symbols_of`] answers `None` when it cannot follow a base
    /// — a base with type arguments, a qualified name, an expression. That
    /// propagates here for the reason it was written in `members.rs`: an
    /// unfollowable base may declare an index signature, and reporting "none"
    /// would let `a[i]` confidently answer nothing when the real answer exists.
    ///
    /// A **cycle** (`interface A extends B {}` with `interface B extends A {}`)
    /// is likewise `None`. Upstream reports
    /// `Type_0_recursively_references_itself_as_a_base_type` and carries on with
    /// empty bases; this port has no diagnostics (`bd tsr-5e7.6`), so the honest
    /// reduction is a gap. The guard is the **path**, exactly as in
    /// `get_property_of_declared_symbol`, and for the same reason: there is no
    /// `resolvedBaseTypes` memo here to park a sentinel in.
    ///
    /// # This cannot desynchronise the printer from the lookup
    ///
    /// The trap this walk looks like it should spring — the lookup finding an
    /// inherited signature the printer never rendered — **cannot arise**, and
    /// the reason is worth stating because it is not obvious. `render_object_type`
    /// is reached only from [`Checker::get_type_from_type_literal`] and
    /// `check_object_literal`. An interface prints by *name* and never renders
    /// its members, and a type literal has no heritage clause, so no type that
    /// prints structurally can have an inherited index signature at all. If a
    /// structural printer for interfaces is ever added, this note is the one to
    /// re-read.
    fn index_infos_of_symbol(
        &mut self,
        owner: SymbolId,
        visiting: &mut Vec<SymbolId>,
    ) -> Option<Vec<IndexInfo>> {
        if visiting.contains(&owner) {
            return None;
        }
        visiting.push(owner);
        let declarations = self.binder.symbols().get(owner).declarations.clone();
        let mut infos = Vec::new();
        for declaration in declarations {
            // §252. A CLASS declares index signatures too, and this collector
            // read only the two `TypeElement` carriers. `getIndexInfosOfSymbol`
            // makes no such distinction — it walks the symbol's members, and a
            // class's `[x: string]: string` is one of them.
            //
            //     class C { foo!: string; [x: string]: string; }
            //     declare var c: C;
            //     var r2: string = c[''];
            //     >c[''] : string        <- this port answered `any`
            //
            // Witness `conformance/objectTypeWithStringIndexerHidingObjectIndexer`,
            // whose four sub-cases are a class, an interface, a type literal and
            // an `Object` augmentation — the interface and literal ones already
            // passed, which is exactly why the class gap was invisible: three of
            // four carriers worked.
            //
            // A class member is a `ClassElement`, not a `TypeElement`, so the
            // arm is separate rather than another line in the match. The
            // *element* is the only difference; the info is built by the same
            // `index_info_of`, which the AST makes possible because both
            // carriers wrap the identical `IndexSignatureDeclaration` node.
            let mut push_from = |checker: &mut Self, signature| {
                if let Some(info) = checker.index_info_of(signature) {
                    infos.push(info);
                }
            };
            match self.node_map.get(declaration) {
                Some(Node::InterfaceDeclaration(node)) => {
                    for member in node.members {
                        if let TypeElement::IndexSignatureDeclaration(signature) = member {
                            push_from(self, signature);
                        }
                    }
                }
                Some(Node::TypeLiteralNode(node)) => {
                    for member in node.members {
                        if let TypeElement::IndexSignatureDeclaration(signature) = member {
                            push_from(self, signature);
                        }
                    }
                }
                Some(Node::ClassDeclaration(node)) => {
                    for member in node.members {
                        if let tsr_ast::ClassElement::IndexSignatureDeclaration(signature) = member
                        {
                            push_from(self, signature);
                        }
                    }
                }
                // Any other declaration kind carries no index signature.
                // (`_ => continue` here is the same thing and clippy calls it
                // redundant, since the loop body ends immediately after.)
                _ => {}
            }
        }
        for base in self.base_symbols_of(owner)? {
            for inherited in self.index_infos_of_symbol(base, visiting)? {
                // `findIndexInfo(indexInfos, info.keyType) == nil` — an own
                // signature for this key hides the base's outright.
                if !infos.iter().any(|own| own.key == inherited.key) {
                    infos.push(inherited);
                }
            }
        }
        Some(infos)
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
        // §32 (`checker-notes-callres.md`): any computable NON-UNION key
        // prints as written; a union key is upstream TWO infos
        // (`getIndexInfosOfIndexSymbol` splits it), so it still declines.
        // The LOOKUP gate (`index_info_of`) deliberately stays narrower —
        // a print the lookup cannot serve gaps the access, never wrongs it.
        if key == self.intrinsics.error
            || self.store.get(key).flags.intersects(crate::flags::TypeFlags::UNION)
        {
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
    /// The `Record<K, V>` index signature, for `K` exactly `string` or
    /// `number`. §785 — see the call site in
    /// [`Checker::get_index_infos_of_type`] for why this alias alone is
    /// special-cased and why a literal-union key is excluded.
    fn record_index_info(&mut self, receiver: TypeId) -> Option<IndexInfo> {
        let (target, arguments) = self.type_reference_targets.get(&receiver)?.clone();
        if arguments.len() != 2 {
            return None;
        }
        let key = arguments[0];
        if key != self.intrinsics.string && key != self.intrinsics.number {
            return None;
        }
        let record = self.binder.global("Record")?;
        (self.binder.merged_symbol(target) == self.binder.merged_symbol(record))
            .then_some(IndexInfo { key, value: arguments[1] })
    }

    pub(crate) fn get_applicable_index_info(
        &mut self,
        id: TypeId,
        key: TypeId,
    ) -> Option<IndexInfo> {
        let infos = self.get_index_infos_of_type(id)?;
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
        // §567: a NUMERIC ENUM MEMBER is number-like. Upstream's enum member
        // type carries `NumberLiteral | EnumLiteral` together
        // (`checker.go`'s `getFreshTypeOfLiteralType` chain), so
        // `isTypeAssignableTo(E.A, numberType)` holds and `E[E.A]` reads the
        // reverse-mapping signature §262 synthesised. This port mints an enum
        // member as its own `TypeFlags::ENUM` type, so the literal test above
        // could not see it and `E[E.A]` gapped while the identical `E[0]`
        // answered `string`.
        // §694: `isApplicableIndexType` opens with
        // `isTypeAssignableTo(source, target)` (`checker.go:19057`), and `any`
        // is assignable to everything — so an `any`-typed key reads whichever
        // index signature the type has. `bar[id]++` on
        // `{ [id: string]: number }` with `id` used before its declaration
        // (`typeGuardNarrowsIndexedAccessOfKnownProperty10`) records `number`;
        // the structural tests below could not see it because `any` is neither
        // a literal nor `string`/`number`.
        if self.store.get(source).flags.intersects(crate::TypeFlags::ANY) {
            return true;
        }
        let source_is_number = source == number
            || matches!(self.store.get(source).data, TypeData::NumberLiteral(_))
            || self.store.get(source).flags.contains(crate::TypeFlags::ENUM);
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
