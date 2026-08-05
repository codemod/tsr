//! Element access: `a["b"]`, `a[0]`, `a[k]`.
//!
//! Ported from `Checker.checkIndexedAccess` into `checkElementAccessExpression`
//! (`checker.go:8133`, `:8146`) and the part of `getPropertyTypeForIndexType`
//! (`checker.go:21902`) that a literal index reaches.
//!
//! # The whole slice is one observation from upstream
//!
//! `a["b"]` is not a second kind of lookup. Upstream derives a **property name
//! from the index's *type*** — `getPropertyNameFromIndex` (`checker.go:21786`)
//! into `getPropertyNameFromType` — and then calls the same `getPropertyOfType`
//! that property access calls. So an element access with a literal index *is* a
//! property access, and this module is that observation plus the cases where the
//! name cannot be derived.
//!
//! Taking the name from the index's **type** rather than from its syntax is
//! upstream's choice and it is worth more than it looks:
//!
//! ```text
//! const k = "b";
//! a[k]            // k's type is the literal "b", so this resolves
//! ```
//!
//! A syntactic reading would see an identifier and gap. Because a `const`
//! initialised with a string literal keeps its literal type (the freshness rule
//! this crate already implements), the type-directed reading answers it.
//!
//! # What is a gap
//!
//! - **An index whose type is not a literal** — `a[i]` for a `number` `i`. That
//!   needs index signatures (`getIndexInfoOfType`), which no type here has.
//! - **An optional chain**, `a?.[b]`.
//! - **A `unique symbol` index**, the third arm of `getPropertyNameFromType`.
//! - **A name that is not a property of the receiver.** Upstream reports
//!   "Property 0 does not exist" and answers `errorType`; so does this, for the
//!   same reason property access does — including every receiver whose members
//!   this port cannot reach, such as an array or a tuple, which resolve through
//!   `lib.d.ts` (`bd tsr-9or.1`).

use tsr_ast::ElementAccessExpression;

use crate::{
    checker::Checker,
    types::{TypeData, TypeId},
};

impl Checker<'_, '_> {
    /// The type of an element access expression.
    ///
    /// Ported from `Checker.checkElementAccessExpression` (`checker.go:8146`).
    ///
    /// Upstream's `checkNonNullExpression` on the receiver, the widening for an
    /// assignment target, the `const` enum diagnostic and the `for…in` numeric
    /// special case are all absent: each either reports (`bd tsr-5e7.6`) or needs
    /// machinery this port does not have, and **none of them changes the type**
    /// for the shapes answered here.
    pub fn check_element_access_expression(
        &mut self,
        node: &ElementAccessExpression<'_>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        if node.question_dot_token.is_some() {
            return error;
        }
        let (Some(receiver), Some(index)) = (node.expression, node.argument_expression) else {
            return error;
        };
        let object_type = self.check_expression(receiver);
        // Upstream returns the object type when it is `errorType`
        // (`checker.go:8154`), which is the same answer by identity — an
        // unreachable receiver takes the access with it.
        if object_type == error {
            return error;
        }
        let index_type = self.check_expression(index);
        let Some(name) = self.property_name_from_index(index_type) else {
            return error;
        };
        match self.get_property_of_type(object_type, &name) {
            Some(property) => self.get_type_of_symbol(property),
            None => error,
        }
    }

    /// The property name an index type names, if it names one.
    ///
    /// Ported from `getPropertyNameFromIndex` into `getPropertyNameFromType`
    /// (`checker.go:21786`), restricted to the two literal arms. Upstream's
    /// fallback — reading the name off the *node* when the type is unusable — is
    /// for index signatures in type position and is not reachable from an element
    /// access expression.
    ///
    /// The numeric arm takes the literal's **normalised** text, which is the same
    /// string `Number::toString` gives upstream: `a[1.0]` and `a[1]` name the same
    /// property `1`, and the payload is already normalised for exactly this
    /// reason (see [`crate::printing::normalise_number`]).
    fn property_name_from_index(&self, index: TypeId) -> Option<String> {
        match &self.store.get(index).data {
            TypeData::StringLiteral(value) => Some(value.clone()),
            TypeData::NumberLiteral(text) => Some(text.clone()),
            _ => None,
        }
    }
}
