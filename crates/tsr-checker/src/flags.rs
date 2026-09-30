//! `TypeFlags`, ported from `internal/checker/types.go:418`.

bitflags::bitflags! {
    /// What kind of type this is.
    ///
    /// Ported one-for-one from typescript-go's `TypeFlags`
    /// (`internal/checker/types.go:427`), bit positions included.
    ///
    /// **The bit values are load-bearing, not arbitrary.** Upstream's comment at
    /// `types.go:420` is explicit: "the numeric values of TypeFlags determine the
    /// order computed by the `CompareTypes` function and therefore the order of
    /// constituent types in union types." Union constituent order is observable in
    /// every `.types` baseline, so renumbering these would silently reorder
    /// printed unions and fail conformance in a way that looks like a formatting
    /// bug. They are ordered by increasing potential complexity so that union
    /// processing can bail out early, with indexed-access and conditional types
    /// last because those are potentially infinite.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct TypeFlags: u32 {
        /// `any`.
        const ANY = 1 << 0;
        /// `unknown`.
        const UNKNOWN = 1 << 1;
        /// `undefined`.
        const UNDEFINED = 1 << 2;
        /// `null`.
        const NULL = 1 << 3;
        /// `void`.
        const VOID = 1 << 4;
        /// `string`.
        const STRING = 1 << 5;
        /// `number`.
        const NUMBER = 1 << 6;
        /// `bigint`.
        const BIG_INT = 1 << 7;
        /// `boolean`.
        const BOOLEAN = 1 << 8;
        /// The `symbol` primitive introduced in ES6.
        const ES_SYMBOL = 1 << 9;
        /// A string literal type, e.g. `"a"`.
        const STRING_LITERAL = 1 << 10;
        /// A numeric literal type, e.g. `1`.
        const NUMBER_LITERAL = 1 << 11;
        /// A bigint literal type, e.g. `1n`.
        const BIG_INT_LITERAL = 1 << 12;
        /// A boolean literal type, `true` or `false`.
        const BOOLEAN_LITERAL = 1 << 13;
        /// `unique symbol`.
        const UNIQUE_ES_SYMBOL = 1 << 14;
        /// Always combined with `STRING_LITERAL`, `NUMBER_LITERAL`, or `UNION`.
        const ENUM_LITERAL = 1 << 15;
        /// A numeric computed enum member value.
        ///
        /// Must stay immediately after [`Self::ENUM_LITERAL`]; upstream's
        /// `getSortOrderFlags` depends on the adjacency.
        const ENUM = 1 << 16;
        /// The intrinsic object type, spelled `object`.
        const NON_PRIMITIVE = 1 << 17;
        /// `never`.
        const NEVER = 1 << 18;
        /// A type parameter.
        const TYPE_PARAMETER = 1 << 19;
        /// An object type.
        const OBJECT = 1 << 20;
        /// `keyof T`.
        const INDEX = 1 << 21;
        /// A template literal type.
        const TEMPLATE_LITERAL = 1 << 22;
        /// `Uppercase`/`Lowercase`.
        const STRING_MAPPING = 1 << 23;
        /// A type parameter substitution.
        const SUBSTITUTION = 1 << 24;
        /// `T[K]`.
        const INDEXED_ACCESS = 1 << 25;
        /// `T extends U ? X : Y`.
        const CONDITIONAL = 1 << 26;
        /// `T | U`.
        const UNION = 1 << 27;
        /// `T & U`.
        const INTERSECTION = 1 << 28;
    }
}

impl TypeFlags {
    /// `TypeFlagsAnyOrUnknown`.
    pub const ANY_OR_UNKNOWN: Self = Self::ANY.union(Self::UNKNOWN);
    /// `TypeFlagsNullable`.
    pub const NULLABLE: Self = Self::UNDEFINED.union(Self::NULL);
    /// `TypeFlagsLiteral`.
    pub const LITERAL: Self = Self::STRING_LITERAL
        .union(Self::NUMBER_LITERAL)
        .union(Self::BIG_INT_LITERAL)
        .union(Self::BOOLEAN_LITERAL);
    /// `TypeFlagsFreshable` — a literal or enum type has a fresh and a widened form.
    pub const FRESHABLE: Self = Self::ENUM.union(Self::LITERAL);
    /// `TypeFlagsUnit`.
    pub const UNIT: Self =
        Self::ENUM.union(Self::LITERAL).union(Self::UNIQUE_ES_SYMBOL).union(Self::NULLABLE);
    /// `TypeFlagsStringLike`.
    pub const STRING_LIKE: Self = Self::STRING
        .union(Self::STRING_LITERAL)
        .union(Self::TEMPLATE_LITERAL)
        .union(Self::STRING_MAPPING);
    /// `TypeFlagsNumberLike`.
    pub const NUMBER_LIKE: Self = Self::NUMBER.union(Self::NUMBER_LITERAL).union(Self::ENUM);
    /// `TypeFlagsBigIntLike`.
    pub const BIG_INT_LIKE: Self = Self::BIG_INT.union(Self::BIG_INT_LITERAL);
    /// `TypeFlagsBooleanLike`.
    pub const BOOLEAN_LIKE: Self = Self::BOOLEAN.union(Self::BOOLEAN_LITERAL);
    /// `TypeFlagsEnumLike` — `types.go:475`.
    pub const ENUM_LIKE: Self = Self::ENUM.union(Self::ENUM_LITERAL);
    /// `TypeFlagsESSymbolLike` — `types.go:476`.
    pub const ES_SYMBOL_LIKE: Self = Self::ES_SYMBOL.union(Self::UNIQUE_ES_SYMBOL);
    /// `TypeFlagsDefinitelyNonNullable` — `types.go:479`.
    pub const DEFINITELY_NON_NULLABLE: Self = Self::STRING_LIKE
        .union(Self::NUMBER_LIKE)
        .union(Self::BIG_INT_LIKE)
        .union(Self::BOOLEAN_LIKE)
        .union(Self::ENUM_LIKE)
        .union(Self::ES_SYMBOL_LIKE)
        .union(Self::OBJECT)
        .union(Self::NON_PRIMITIVE);
    /// `TypeFlagsPrimitive` — `types.go:478`.
    ///
    /// Every domain a value can inhabit without being an object. Used by the
    /// intersection reductions, where a primitive beside a type from another
    /// domain is the empty set.
    pub const PRIMITIVE: Self = Self::STRING_LIKE
        .union(Self::NUMBER_LIKE)
        .union(Self::BIG_INT_LIKE)
        .union(Self::BOOLEAN_LIKE)
        .union(Self::ENUM_LIKE)
        .union(Self::ES_SYMBOL_LIKE)
        .union(Self::VOID_LIKE)
        .union(Self::NULL);
    /// `TypeFlagsVoidLike`.
    pub const VOID_LIKE: Self = Self::VOID.union(Self::UNDEFINED);
}
