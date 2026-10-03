//! Interned types retain freshness, constituent order and nominal declaration identity.

use tsr_binder::{SymbolFlags, SymbolStore};
use tsr_checker::flags::TypeFlags;
use tsr_checker::types::{EnumLiteralValue, TypeData, TypeStore};

#[test]
fn literal_reuse_preserves_freshness_value_and_type_creation_order() {
    let mut store = TypeStore::new();
    let regular =
        store.intern_literal(TypeFlags::STRING_LITERAL, TypeData::StringLiteral("x".into()), false);
    let fresh =
        store.intern_literal(TypeFlags::STRING_LITERAL, TypeData::StringLiteral("x".into()), true);
    let other =
        store.intern_literal(TypeFlags::STRING_LITERAL, TypeData::StringLiteral("y".into()), false);
    assert_ne!(regular, fresh);
    assert_ne!(regular, other);
    assert_eq!(store.len(), 3);
    for _ in 0..3 {
        assert_eq!(
            store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral("x".into()),
                false
            ),
            regular
        );
        assert_eq!(
            store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral("x".into()),
                true
            ),
            fresh
        );
    }
    assert_eq!(store.len(), 3);
    assert!(!store.get(regular).fresh);
    assert!(store.get(fresh).fresh);
    let next = store.new_intrinsic(TypeFlags::ANY, "any");
    assert_eq!(next.index(), 3, "hits must not reserve unused type identities");
}

#[test]
fn composite_reuse_keeps_variant_constituent_order_and_alias_identity() {
    let mut store = TypeStore::new();
    let string = store.new_intrinsic(TypeFlags::STRING, "string");
    let number = store.new_intrinsic(TypeFlags::NUMBER, "number");
    let mut symbols = SymbolStore::new();
    let first_alias = symbols.create("Alias", SymbolFlags::TYPE_ALIAS);
    let second_alias = symbols.create("Alias", SymbolFlags::TYPE_ALIAS);
    let union = TypeData::Union {
        text: "Alias".into(),
        types: vec![string, number],
        symbol: Some(first_alias),
    };
    let id = store.intern_union(TypeFlags::UNION, union.clone());
    assert_eq!(store.intern_union(TypeFlags::UNION, union), id);
    let other_alias = store.intern_union(
        TypeFlags::UNION,
        TypeData::Union {
            text: "Alias".into(),
            types: vec![string, number],
            symbol: Some(second_alias),
        },
    );
    assert_ne!(id, other_alias, "same printed alias is not the same declaration");
    let intersection = store.intern_intersection(
        TypeFlags::INTERSECTION,
        TypeData::Intersection {
            text: "Alias".into(),
            types: vec![string, number],
            symbol: Some(first_alias),
        },
    );
    let reversed = store.intern_intersection(
        TypeFlags::INTERSECTION,
        TypeData::Intersection {
            text: "Alias".into(),
            types: vec![number, string],
            symbol: Some(first_alias),
        },
    );
    assert_ne!(intersection, id);
    assert_ne!(intersection, reversed, "intersection constituent order remains significant");
    let ids_before = store.len();
    assert_eq!(
        store.intern_intersection(TypeFlags::INTERSECTION, store.get(intersection).data.clone()),
        intersection
    );
    assert_eq!(store.len(), ids_before);
}

#[test]
fn enum_literal_reuse_keeps_nominal_owner_identity() {
    let mut store = TypeStore::new();
    let mut symbols = SymbolStore::new();
    let first = symbols.create("Enum", SymbolFlags::REGULAR_ENUM);
    let second = symbols.create("Enum", SymbolFlags::REGULAR_ENUM);
    let first_member = symbols.create("Value", SymbolFlags::ENUM_MEMBER);
    let second_member = symbols.create("Value", SymbolFlags::ENUM_MEMBER);
    let literal = TypeData::EnumLiteral {
        value: EnumLiteralValue::Number("1".into()),
        owner: first,
        member: first_member,
        text: "Enum.Value".into(),
    };
    let flags = TypeFlags::ENUM_LITERAL | TypeFlags::NUMBER_LITERAL;
    let id = store.intern_literal(flags, literal.clone(), false);
    assert_eq!(store.intern_literal(flags, literal, false), id);
    let other = store.intern_literal(
        flags,
        TypeData::EnumLiteral {
            value: EnumLiteralValue::Number("1".into()),
            owner: second,
            member: second_member,
            text: "Enum.Value".into(),
        },
        false,
    );
    assert_ne!(id, other);
}
