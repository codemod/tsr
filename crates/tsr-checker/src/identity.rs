//! `isTypeIdenticalTo` — the identity relation, as a three-valued walk.
//!
//! Pinned tsgo `5b1047d`, `internal/checker/relater.go`: `isTypeRelatedTo`
//! (`:170`) and `isRelatedTo` (`:2625`) under `identityRelation`, then
//! `structuredTypeRelatedToWorker`'s identity arm (`:3309`) and the four
//! structural comparisons it reaches for object types —
//! `propertiesIdenticalTo` (`:4101` → `compareProperties`, `checker.go:27672`),
//! `signaturesIdenticalTo` (`:4442` → `compareSignaturesIdentical`, `:2167`) for
//! call and construct signatures, and `indexSignaturesIdenticalTo` (`:4579`).
//!
//! # Why a walk of its own rather than a `Relation::Identity`
//!
//! The relater (`crate::relater`) carries assignability, subtype, strict
//! subtype and comparability over one structural engine whose answers are
//! `Unknown` wherever this port cannot enumerate a side. Identity is the one
//! relation whose structural arm is *symmetric and exhaustive*: equal property
//! counts, equal signature counts, every pair identical. None of the
//! assignability machinery (apparent types, optionality relaxations, excess
//! properties, discriminated targets, the `any` index rule) applies, so a
//! separate walk over the same member queries is both smaller and safer than
//! threading a fifth relation through every arm of the relater, which another
//! lane owns. `docs/parity/notes/decls.md` §2 records the decision.
//!
//! # Port boundary (`docs/conventions.md`, checker ports)
//!
//! - **Native operation:** `isTypeIdenticalTo`; first consumer
//!   `checkVariableLikeDeclaration`'s secondary-declaration arm (TS2403).
//! - **Identity and owner:** no cache and no side table. Upstream caches
//!   identity pairs in `identityRelation`; here every call is a fresh walk,
//!   bounded by [`MAX_DEPTH`] and by the assumption stack, which plays
//!   `recursiveTypeRelatedTo`'s `maybeKeys` role: a pair already being compared
//!   is assumed identical (upstream's `TernaryMaybe`).
//! - **Publication:** nothing is published; a `Ternary::Unknown` answer is a
//!   decline and callers report only `NotRelated`.
//! - **Work boundary:** member enumeration through the existing member queries
//!   (`get_property_names_of_type`, `get_type_of_property_of_type`,
//!   `signatures_of_type_kind`, `get_index_infos_of_type`). The only consumer
//!   runs once per *secondary* `var` declaration, which is rare enough that no
//!   cache is warranted; a hot consumer would need one first.

use crate::{
    checker::Checker,
    flags::TypeFlags,
    relater::{Relation, Ternary},
    signatures::{Signature, SignatureKind},
    types::{TypeData, TypeId},
};

/// The nesting bound past which a comparison declines. Upstream's limiter is
/// `isDeeplyNestedType` at depth 3 per recursion identity; a flat bound is
/// coarser but only ever turns an answer into `Unknown`.
const MAX_DEPTH: usize = 40;

/// `TypeFlagsSingleton` (`types.go`): two distinct types with these flags and
/// equal flags are identical.
const SINGLETON: TypeFlags = TypeFlags::ANY
    .union(TypeFlags::UNKNOWN)
    .union(TypeFlags::STRING)
    .union(TypeFlags::NUMBER)
    .union(TypeFlags::BOOLEAN)
    .union(TypeFlags::BIG_INT)
    .union(TypeFlags::ES_SYMBOL)
    .union(TypeFlags::VOID)
    .union(TypeFlags::UNDEFINED)
    .union(TypeFlags::NULL)
    .union(TypeFlags::NEVER)
    .union(TypeFlags::NON_PRIMITIVE);

/// The flags that `isTypeRelatedTo` leaves to normalization before testing
/// flag equality.
const SIMPLIFIABLE: TypeFlags = TypeFlags::UNION
    .union(TypeFlags::INTERSECTION)
    .union(TypeFlags::INDEXED_ACCESS)
    .union(TypeFlags::CONDITIONAL)
    .union(TypeFlags::SUBSTITUTION);

impl Checker<'_, '_> {
    /// `isTypeIdenticalTo(source, target)`, three-valued: `Unknown` where this
    /// port cannot decide a part of the comparison.
    pub(crate) fn is_type_identical_to(&mut self, source: TypeId, target: TypeId) -> Ternary {
        let mut walk = Walk { assumed: Vec::new(), structural: true };
        self.identical_to(source, target, &mut walk)
    }

    /// [`Self::is_type_identical_to`] with the object arm reduced to mutual
    /// assignability: two object types are non-identical only when one is
    /// not assignable to the other. For operands this port may have *built*
    /// wrong (an inferred initializer type), where the structural arm's
    /// precision would turn a near-miss in another subsystem into a report.
    /// `docs/parity/notes/decls.md` §2.
    pub(crate) fn is_type_identical_to_by_assignability(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> Ternary {
        let mut walk = Walk { assumed: Vec::new(), structural: false };
        self.identical_to(source, target, &mut walk)
    }

    fn identical_to(&mut self, source: TypeId, target: TypeId, walk: &mut Walk) -> Ternary {
        let source = self.get_regular_type_of_literal_type(source);
        let target = self.get_regular_type_of_literal_type(target);
        if source == target {
            return Ternary::Related;
        }
        if self.is_gap(source)
            || self.is_gap(target)
            || source == self.intrinsics.unresolved
            || target == self.intrinsics.unresolved
        {
            return Ternary::Unknown;
        }
        let source_flags = self.type_of(source).flags;
        let target_flags = self.type_of(target).flags;
        if source_flags != target_flags {
            // **Enum types are declined.** This port builds one enum under two
            // representations (a declared enum type and its literal union) and
            // a narrowed enum union can come back as `number`; a flags
            // difference involving an enum is not yet evidence of non-identity.
            // Even an object type against an enum declines: a qualified
            // `M3.Color` annotation can resolve to an object-flagged type
            // (`instantiatedModule`, measured; notes §2).
            let enum_like = TypeFlags::ENUM | TypeFlags::ENUM_LITERAL;
            if (source_flags | target_flags).intersects(enum_like) {
                return Ternary::Unknown;
            }
            // `isTypeRelatedTo` tests flags only outside the simplifiable
            // kinds; `isRelatedTo` tests them again after `getNormalizedType`.
            // This port's types reach here already simplified, so the second
            // test is applied directly.
            return Ternary::NotRelated;
        }
        if !source_flags.intersects(SIMPLIFIABLE) && source_flags.intersects(SINGLETON) {
            return Ternary::Related;
        }
        if walk.assumed.contains(&(source, target)) {
            // `recursiveTypeRelatedTo`'s `maybeKeys`: a pair already on the
            // stack is assumed to hold.
            return Ternary::Related;
        }
        if walk.assumed.len() >= MAX_DEPTH {
            return Ternary::Unknown;
        }
        walk.assumed.push((source, target));
        let result = self.structured_identical_to(source, target, source_flags, walk);
        walk.assumed.pop();
        result
    }

    /// `structuredTypeRelatedToWorker`'s identity arm (`relater.go:3309`) and,
    /// for object types, the structural comparison below it (`:3862`).
    fn structured_identical_to(
        &mut self,
        source: TypeId,
        target: TypeId,
        flags: TypeFlags,
        walk: &mut Walk,
    ) -> Ternary {
        if flags.intersects(TypeFlags::UNION | TypeFlags::INTERSECTION) {
            let (TypeData::Union { types: sources, .. }
            | TypeData::Intersection { types: sources, .. }) = self.type_of(source).data.clone()
            else {
                return Ternary::Unknown;
            };
            let (TypeData::Union { types: targets, .. }
            | TypeData::Intersection { types: targets, .. }) = self.type_of(target).data.clone()
            else {
                return Ternary::Unknown;
            };
            let forward = self.each_identical_to_some(&sources, &targets, walk);
            if forward == Ternary::NotRelated {
                return forward;
            }
            let backward = self.each_identical_to_some(&targets, &sources, walk);
            return and(forward, backward);
        }
        if flags.intersects(TypeFlags::TYPE_PARAMETER) {
            // Two distinct type parameters reach `if source.flags&TypeFlagsObject
            // == 0 { return TernaryFalse }`. This port can mint a second id for
            // one parameter (`instantiate_signature_with_fresh_parameters`), so
            // a pair sharing a declaring symbol, or lacking one, declines.
            let source_symbol = self.type_parameter_symbols.get(&source).copied();
            let target_symbol = self.type_parameter_symbols.get(&target).copied();
            let fresh = |checker: &Self, id| checker.instantiated_type_parameters.contains_key(&id);
            if source_symbol.is_none()
                || target_symbol.is_none()
                || source_symbol == target_symbol
                || fresh(self, source)
                || fresh(self, target)
            {
                return Ternary::Unknown;
            }
            return Ternary::NotRelated;
        }
        if !flags.contains(TypeFlags::OBJECT) || flags.intersects(TypeFlags::INSTANTIABLE) {
            // Index, template-literal and string-mapping identities have
            // structural arms upstream that are not ported here.
            return Ternary::Unknown;
        }
        if !walk.structural {
            // Identity implies mutual assignability, so either direction
            // deciding `NotRelated` is non-identity.
            if self.pair_is_reportable(source, target)
                && !self.is_mapped_for_identity(source)
                && !self.is_mapped_for_identity(target)
                && (self.relate_ternary(source, target, Relation::Assignable)
                    == Ternary::NotRelated
                    || self.relate_ternary(target, source, Relation::Assignable)
                        == Ternary::NotRelated)
            {
                return Ternary::NotRelated;
            }
            return Ternary::Unknown;
        }
        // `isGenericMappedType(source)` is `False` under identity
        // (`relater.go:3817`); a resolved mapped type is compared by its
        // members upstream, but this port's mapped members are not complete
        // enough to prove a negative.
        if self.is_mapped_for_identity(source) || self.is_mapped_for_identity(target) {
            return Ternary::Unknown;
        }
        // Type references to one generic target: the type arguments are
        // related under identity whatever their variance, except an
        // independent parameter, which is never witnessed. A success settles
        // it; a failure falls through to the structural comparison, which
        // decides the same pairs upstream's `relateVariances` would reject
        // and keeps the `Unmeasurable` fallback without needing its flags.
        let references = (
            self.type_reference_targets.get(&source).cloned(),
            self.type_reference_targets.get(&target).cloned(),
        );
        if let (Some((source_symbol, source_arguments)), Some((target_symbol, target_arguments))) =
            references
            && self.binder.merged_symbol(source_symbol) == self.binder.merged_symbol(target_symbol)
            && source_arguments.len() == target_arguments.len()
        {
            let variances = self.inference_variances(source_symbol).unwrap_or_default();
            let mut result = Ternary::Related;
            for (index, (&s, &t)) in source_arguments.iter().zip(&target_arguments).enumerate() {
                if variances.get(index) == Some(&crate::variances::Variance::Independent) {
                    continue;
                }
                result = and(result, self.identical_to(s, t, walk));
                if result == Ternary::NotRelated {
                    break;
                }
            }
            if result == Ternary::Related {
                return result;
            }
        }
        let properties = self.properties_identical_to(source, target, walk);
        if properties == Ternary::NotRelated {
            return properties;
        }
        let mut result = properties;
        for kind in [SignatureKind::Call, SignatureKind::Construct] {
            let signatures = self.signatures_identical_to(source, target, kind, walk);
            if signatures == Ternary::NotRelated {
                return signatures;
            }
            result = and(result, signatures);
        }
        let indexes = self.index_signatures_identical_to(source, target, walk);
        if indexes == Ternary::NotRelated {
            return indexes;
        }
        and(result, indexes)
    }

    /// `eachTypeRelatedToSomeType` under identity.
    fn each_identical_to_some(
        &mut self,
        sources: &[TypeId],
        targets: &[TypeId],
        walk: &mut Walk,
    ) -> Ternary {
        let mut result = Ternary::Related;
        for &source in sources {
            let mut best = Ternary::NotRelated;
            for &target in targets {
                match self.identical_to(source, target, walk) {
                    Ternary::Related => {
                        best = Ternary::Related;
                        break;
                    }
                    Ternary::NotRelated => {}
                    Ternary::Unknown => best = Ternary::Unknown,
                }
            }
            match best {
                Ternary::NotRelated => return Ternary::NotRelated,
                Ternary::Related => {}
                Ternary::Unknown => result = Ternary::Unknown,
            }
        }
        result
    }

    /// `propertiesIdenticalTo` (`relater.go:4101`) with `compareProperties`
    /// (`checker.go:27672`) inlined.
    fn properties_identical_to(
        &mut self,
        source: TypeId,
        target: TypeId,
        walk: &mut Walk,
    ) -> Ternary {
        let (Some(source_names), Some(target_names)) =
            (self.get_property_names_of_type(source), self.get_property_names_of_type(target))
        else {
            return Ternary::Unknown;
        };
        if source_names.len() != target_names.len() {
            return Ternary::NotRelated;
        }
        if source_names.iter().any(|name| !target_names.contains(name)) {
            return Ternary::NotRelated;
        }
        let mut result = Ternary::Related;
        for name in source_names {
            let related = self.property_identical_to(source, target, &name, walk);
            if related == Ternary::NotRelated {
                return related;
            }
            result = and(result, related);
        }
        result
    }

    /// `compareProperties` for one name present on both sides.
    fn property_identical_to(
        &mut self,
        source: TypeId,
        target: TypeId,
        name: &str,
        walk: &mut Walk,
    ) -> Ternary {
        let Some(source_metadata) = self.identity_property_metadata(source, name) else {
            return Ternary::Unknown;
        };
        let Some(target_metadata) = self.identity_property_metadata(target, name) else {
            return Ternary::Unknown;
        };
        if source_metadata.symbol.is_some() && source_metadata.symbol == target_metadata.symbol {
            // `sourceProp == targetProp`.
            return Ternary::Related;
        }
        if source_metadata.accessibility != target_metadata.accessibility {
            return Ternary::NotRelated;
        }
        if source_metadata.accessibility != Accessibility::Public {
            // `getTargetSymbol(sourceProp) != getTargetSymbol(targetProp)`: a
            // non-public member is identical only to itself, read through the
            // declaration it originates in.
            if source_metadata.declaration.is_none()
                || source_metadata.declaration != target_metadata.declaration
            {
                return Ternary::NotRelated;
            }
        } else if source_metadata.optional != target_metadata.optional {
            return Ternary::NotRelated;
        }
        if source_metadata.readonly != target_metadata.readonly {
            return Ternary::NotRelated;
        }
        let (Some(source_type), Some(target_type)) = (
            self.get_type_of_property_of_type(source, name),
            self.get_type_of_property_of_type(target, name),
        ) else {
            return Ternary::Unknown;
        };
        // `getNonMissingTypeOfSymbol`.
        let (source_type, target_type) = if self.exact_optional_property_types {
            (self.remove_missing_type(source_type), self.remove_missing_type(target_type))
        } else {
            (source_type, target_type)
        };
        self.identical_to(source_type, target_type, walk)
    }

    /// The `compareProperties` inputs for one property of a receiver: the
    /// captured anonymous-member flags where the receiver has them, else the
    /// declaring symbol's syntax — the same two sources the relater's
    /// property-flag reader uses.
    fn identity_property_metadata(
        &mut self,
        receiver: TypeId,
        name: &str,
    ) -> Option<PropertyMetadata> {
        if let Some((properties, _)) = self.anonymous_properties.get(&receiver)
            && let Some(property) = properties.iter().find(|property| property.name == name)
        {
            let (optional, readonly, origin) =
                (property.optional, property.readonly, property.origin);
            let accessibility = match origin {
                Some(symbol) => self.symbol_accessibility(symbol),
                None => Accessibility::Public,
            };
            let declaration =
                origin.and_then(|symbol| self.binder.symbols().get(symbol).value_declaration);
            return Some(PropertyMetadata {
                symbol: None,
                declaration,
                accessibility,
                optional,
                readonly,
            });
        }
        let symbol = self.get_property_of_type(receiver, name)?;
        Some(PropertyMetadata {
            symbol: Some(symbol),
            declaration: self.binder.symbols().get(symbol).value_declaration,
            accessibility: self.symbol_accessibility(symbol),
            optional: self.property_is_optional(symbol),
            readonly: self.is_readonly_property(symbol),
        })
    }

    /// `getDeclarationModifierFlagsFromSymbol(prop) &
    /// ModifierFlagsNonPublicAccessibilityModifier`.
    fn symbol_accessibility(&self, symbol: tsr_binder::SymbolId) -> Accessibility {
        if self.property_has_modifier(symbol, tsr_ast::SyntaxKind::PrivateKeyword) {
            Accessibility::Private
        } else if self.property_has_modifier(symbol, tsr_ast::SyntaxKind::ProtectedKeyword) {
            Accessibility::Protected
        } else {
            Accessibility::Public
        }
    }

    /// `signaturesIdenticalTo` (`relater.go:4442`).
    fn signatures_identical_to(
        &mut self,
        source: TypeId,
        target: TypeId,
        kind: SignatureKind,
        walk: &mut Walk,
    ) -> Ternary {
        let (Some(sources), Some(targets)) = (
            self.signatures_of_type_kind(source, kind),
            self.signatures_of_type_kind(target, kind),
        ) else {
            return Ternary::Unknown;
        };
        if sources.len() != targets.len() {
            return Ternary::NotRelated;
        }
        let mut result = Ternary::Related;
        for (source, target) in sources.iter().zip(&targets) {
            let related = self.compare_signatures_identical(source, target, walk);
            if related == Ternary::NotRelated {
                return related;
            }
            result = and(result, related);
        }
        result
    }

    /// `compareSignaturesIdentical(source, target, partialMatch=false,
    /// ignoreThisTypes=false, ignoreReturnTypes=false, isRelatedTo)`
    /// (`relater.go:2167`).
    fn compare_signatures_identical(
        &mut self,
        source: &Signature,
        target: &Signature,
        walk: &mut Walk,
    ) -> Ternary {
        // `isMatchingSignature` (`relater.go:2227`) without the partial arm.
        if self.signature_parameter_count(source) != self.signature_parameter_count(target)
            || self.signature_min_argument_count(source)
                != self.signature_min_argument_count(target)
            || self.signature_has_effective_rest(source)
                != self.signature_has_effective_rest(target)
        {
            return Ternary::NotRelated;
        }
        if source.type_parameters.len() != target.type_parameters.len() {
            return Ternary::NotRelated;
        }
        // Instantiate the source with the target's type parameters, after
        // checking constraints and defaults agree under that mapping.
        let map: Vec<(TypeId, TypeId)> = if target.type_parameters.is_empty() {
            Vec::new()
        } else {
            let (Some(source_parameters), Some(target_parameters)) =
                (self.type_parameter_types(source), self.type_parameter_types(target))
            else {
                return Ternary::Unknown;
            };
            source_parameters.into_iter().zip(target_parameters).collect()
        };
        let mut result = Ternary::Related;
        let unknown = self.intrinsics.unknown;
        for (index, &(s, t)) in map.iter().enumerate() {
            if s == t {
                continue;
            }
            let pairs = [
                (
                    source.type_parameters[index].constraint,
                    target.type_parameters[index].constraint,
                ),
                (source.type_parameters[index].default, target.type_parameters[index].default),
            ];
            for (source_part, target_part) in pairs {
                let source_part = source_part.unwrap_or(unknown);
                let source_part = self.instantiate_identity_part(source_part, &map);
                let related = self.identical_to(source_part, target_part.unwrap_or(unknown), walk);
                if related == Ternary::NotRelated {
                    return related;
                }
                result = and(result, related);
            }
        }
        if let (Some(source_this), Some(target_this)) =
            (&source.this_parameter, &target.this_parameter)
        {
            let source_this = self.parameter_type(source_this);
            let source_this = self.instantiate_identity_part(source_this, &map);
            let target_this = self.parameter_type(target_this);
            let related = self.identical_to(source_this, target_this, walk);
            if related == Ternary::NotRelated {
                return related;
            }
            result = and(result, related);
        }
        for position in 0..self.signature_parameter_count(target) {
            let (Some(s), Some(t)) = (
                self.signature_type_at_position(source, position),
                self.signature_type_at_position(target, position),
            ) else {
                return Ternary::Unknown;
            };
            let s = self.instantiate_identity_part(s, &map);
            let related = self.identical_to(t, s, walk);
            if related == Ternary::NotRelated {
                return related;
            }
            result = and(result, related);
        }
        if source.predicate.is_some() || target.predicate.is_some() {
            // `compareTypePredicatesIdentical` is not ported.
            return and(result, Ternary::Unknown);
        }
        let (Some(source_return), Some(target_return)) =
            (self.get_return_type_of_signature(source), self.get_return_type_of_signature(target))
        else {
            return Ternary::Unknown;
        };
        let source_return = self.instantiate_identity_part(source_return, &map);
        and(result, self.identical_to(source_return, target_return, walk))
    }

    /// One type of a source signature under the source-to-target type
    /// parameter mapping (`instantiateSignatureEx(source, mapper, true)`).
    fn instantiate_identity_part(&mut self, ty: TypeId, map: &[(TypeId, TypeId)]) -> TypeId {
        if map.is_empty() {
            return ty;
        }
        let parameters: Vec<TypeId> = map.iter().map(|&(_, target)| target).collect();
        let names: Vec<String> = parameters.iter().map(|&id| self.type_to_string(id)).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        self.instantiate_type(ty, map, &parameters, &names)
    }

    /// `indexSignaturesIdenticalTo` (`relater.go:4579`).
    fn index_signatures_identical_to(
        &mut self,
        source: TypeId,
        target: TypeId,
        walk: &mut Walk,
    ) -> Ternary {
        let (Some(sources), Some(targets)) =
            (self.get_index_infos_of_type(source), self.get_index_infos_of_type(target))
        else {
            return Ternary::Unknown;
        };
        if sources.len() != targets.len() {
            return Ternary::NotRelated;
        }
        let mut result = Ternary::Related;
        for target_info in &targets {
            // `getIndexInfoOfType(source, targetInfo.keyType)` — an exact key.
            let Some(source_info) = sources.iter().find(|info| info.key == target_info.key) else {
                return Ternary::NotRelated;
            };
            if source_info.readonly != target_info.readonly {
                return Ternary::NotRelated;
            }
            let related = self.identical_to(source_info.value, target_info.value, walk);
            if related == Ternary::NotRelated {
                return related;
            }
            result = and(result, related);
        }
        result
    }
}

/// One identity comparison's state: the assumption stack and the object-arm
/// mode.
struct Walk {
    /// Pairs under comparison (`maybeKeys`).
    assumed: Vec<(TypeId, TypeId)>,
    /// Whether object types are compared structurally (upstream) or by mutual
    /// assignability (a necessary condition only).
    structural: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Accessibility {
    Public,
    Private,
    Protected,
}

struct PropertyMetadata {
    symbol: Option<tsr_binder::SymbolId>,
    declaration: Option<tsr_ast::NodeId>,
    accessibility: Accessibility,
    optional: bool,
    readonly: bool,
}

/// Ternary conjunction: `NotRelated` dominates, then `Unknown`.
fn and(left: Ternary, right: Ternary) -> Ternary {
    match (left, right) {
        (Ternary::NotRelated, _) | (_, Ternary::NotRelated) => Ternary::NotRelated,
        (Ternary::Related, Ternary::Related) => Ternary::Related,
        _ => Ternary::Unknown,
    }
}
