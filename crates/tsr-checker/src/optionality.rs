//! `T` versus `T | undefined`: the optionality a `?` adds.
//!
//! Ported from `Checker.addOptionality` / `addOptionalityEx` / `getOptionalType`
//! (`checker.go:18629`–`:18647`) and `isOptionalDeclaration`
//! (`internal/checker/utilities.go:299`).
//!
//! # The forcing constraint
//!
//! Measured over the corpus by `tsr-conformance/examples/wrong_attribution.rs`
//! at `d27db6f`: **1,383 wrong `.types` lines are exactly `{ours} | undefined`**
//! — upstream's answer is ours with `| undefined` appended and nothing else
//! differs. That is 6.45% of every wrong `Identifier` line, and it is *one rule*
//! rather than a population, which is why it is worth a module of its own.
//!
//! It is also a **wrong answer rather than a gap**: we say `string` where
//! upstream says `string | undefined`, confidently and incorrectly. The
//! `errorType` discipline exists to keep this port from doing that
//! (`docs/architecture/checker.md`), and this is one of the places it was doing
//! it anyway — not by claiming something unported, but by applying a rule
//! incompletely.
//!
//! # Optionality is added only with `strictNullChecks`
//!
//! `addOptionalityEx` checks both the configured null-check mode and the written
//! `?`. With null checks off it preserves the input identity, including ordinary
//! nullable types and the distinct widening `undefined`. Calling `getOptionalType`
//! in that mode can erase a widening identity or turn ordinary `null` into an
//! error when the loose union constructor drops nullable constituents.
//!
//! # What is deliberately narrower than upstream
//!
//! Upstream calls `addOptionalityEx` from twenty-one sites. This ports the one
//! that pays: the **annotation** branch of `getTypeForVariableLikeDeclaration`
//! (`checker.go:16695`), reached through
//! `getWidenedTypeForVariableLikeDeclaration`, which is the only caller and
//! passes `includeOptionality = true` (`checker.go:16648`).
//!
//! The other twenty are unported and answer as they did before — binding
//! elements, mapped types, tuple members, index infos, JSX, and the
//! *initialiser* branch of the same function. Each is a miss rather than a wrong
//! answer, because this only ever adds `| undefined` where a `?` is written.
//!
//! # A parameter and its signature disagree, on purpose
//!
//! `testdata/baselines/reference/compiler/assertionWithNoArgument.types`, whose
//! case opens `// @strict: true`, settles both halves of this rule on adjacent
//! lines:
//!
//! ```text
//! export function assertWeird(value?: string): asserts value {
//! >assertWeird : (value?: string) => asserts value      <- signature: no `| undefined`
//! >value : string | undefined                           <- the parameter itself: has it
//! ```
//!
//! So an optional parameter's **symbol type** carries `| undefined` — which is
//! what this module produces — while its **printed form inside a signature** does
//! not. Upstream reconciles the two in the node builder rather than in the type:
//! `serializeTypeForDeclaration` (`nodebuilderimpl.go:2214`) reuses the *written
//! annotation node* for a parameter that has one, so the `?` carries the
//! optionality and the annotation prints as written.
//!
//! **That is a constraint on the signature printer, not on this rule.** A printer
//! that renders the symbol type will print `(x?: number | undefined)` and be
//! wrong; `crate::signatures` owns that.
//!
//! An earlier revision of this file said the parameter half was unverified,
//! because `callSignaturesWithOptionalParameters.types` shows `>x : number` and
//! its flags could not be found. That case is non-strict, which explains it
//! without contradicting anything: with `strictNullChecks` off, `addOptionalityEx`
//! adds nothing at all.
//!
//! # `isProperty` is computed and currently cannot be observed
//!
//! `getOptionalType` picks `undefinedOrMissingType` for a property and
//! `undefinedType` otherwise, and `undefinedOrMissingType` is
//! `exactOptionalPropertyTypes ? missingType : undefinedType`
//! (`checker.go:987`). That option is unported and defaults off, so the two are
//! the same type and the distinction changes no answer today.
//!
//! It is computed faithfully anyway rather than dropped, and said here rather
//! than left to be discovered: mutating [`Checker::is_property_for_optionality`]
//! to `false` turns no test red, and it starts to matter the moment
//! `exactOptionalPropertyTypes` and `missingType` exist.

use tsr_ast::{Node, NodeId, SyntaxKind};

use crate::{checker::Checker, types::TypeId};

impl Checker<'_, '_> {
    /// Add `| undefined` to `ty` if `declaration` is written with a `?`.
    ///
    /// The single entry point, so that the one call site in [`crate::symbols`]
    /// is one line. Upstream's decomposition — the `isProperty` and `isOptional`
    /// arguments of `addOptionalityEx` — is kept below rather than inlined,
    /// because those two are computed at the call site upstream and each has its
    /// own rule.
    pub(crate) fn add_optionality_for_declaration(
        &mut self,
        ty: TypeId,
        declaration: NodeId,
    ) -> TypeId {
        if crate::debug_env::is_set("TSR_DEBUG_832") {
            eprintln!(
                "832: add_optionality_for_declaration kind={:?} optional={}",
                self.nodes.kind(declaration),
                self.is_optional_declaration(declaration)
            );
        }
        let is_property = self.is_property_for_optionality(declaration);
        // `includeOptionality` is true on this path and is therefore not a
        // parameter here; see the module docs.
        let is_optional = self.is_optional_declaration(declaration);
        self.add_optionality_ex(ty, is_property, is_optional)
    }

    /// Ported from `Checker.addOptionalityEx` (`checker.go:18633`).
    ///
    /// Loose mode preserves the supplied identity; only strict optional
    /// declarations add a nullable constituent.
    fn add_optionality_ex(&mut self, ty: TypeId, is_property: bool, is_optional: bool) -> TypeId {
        if self.strict_null_checks && is_optional {
            self.get_optional_type(ty, is_property)
        } else {
            ty
        }
    }

    /// Ported from `Checker.getOptionalType` (`checker.go:18640`).
    ///
    /// # Both early returns are currently unobservable, and that was checked
    ///
    /// An earlier revision of this comment claimed they were load-bearing —
    /// that without them `undefined?` becomes `undefined | undefined` and an
    /// already-optional type gains a second constituent. **That is wrong**, and
    /// it is recorded rather than quietly corrected because it is exactly the
    /// kind of plausible reasoning this project's method exists to catch.
    /// [`Checker::get_union_type`] sorts and deduplicates its constituents and
    /// collapses a one-element union to that element, so both cases already come
    /// out right without either guard. Mutating each of them away turns no test
    /// red; the two tests written for them assert the *answers*, which are
    /// correct either way.
    ///
    /// They are kept because they are upstream's and because they stop being
    /// redundant the moment `undefinedOrMissingType` is `missingType` rather than
    /// `undefinedType` — that is, when `exactOptionalPropertyTypes` is ported —
    /// since `missingType` is a distinct type that would then need this test to
    /// avoid being unioned with itself under a different identity.
    pub(crate) fn get_optional_type(&mut self, ty: TypeId, is_property: bool) -> TypeId {
        // `undefinedOrMissingType` is `exactOptionalPropertyTypes ? missingType
        // : undefinedType` (`checker.go:987`) — §78: the option is now
        // plumbed and `missingType` exists, so upstream's `isProperty`
        // choice is live. `missing` prints `undefined`, and the
        // first-constituent early return below is what keeps `b?: string |
        // undefined` from carrying both spellings.
        let missing_or_undefined = if is_property && self.exact_optional_property_types {
            self.intrinsics.missing
        } else {
            self.intrinsics.undefined
        };
        if ty == missing_or_undefined {
            return ty;
        }
        // Upstream tests the *first* constituent specifically, because its union
        // constituents are ordered and `undefined` sorts to the front, so a type
        // that already carries it carries it there.
        if let crate::types::TypeData::Union { types, .. } = &self.store.get(ty).data
            && types
                .first()
                .is_some_and(|&t| t == missing_or_undefined || t == self.intrinsics.undefined)
        {
            return ty;
        }
        self.get_union_type(&[ty, missing_or_undefined])
    }

    /// [`Checker::get_optional_type`] for a consumer that never prints the
    /// result — see [`Checker::get_union_type_unprinted`].
    ///
    /// Only `T | undefined` differs, and only when `T` is a named union: the
    /// printing guard answers `errorType` there, which is what silenced TS2454
    /// on every enum-typed declaration in the corpus
    /// (`checker-notes-diag2.md` §42.1).
    pub(crate) fn get_optional_type_unprinted(&mut self, ty: TypeId) -> TypeId {
        let missing_or_undefined = self.intrinsics.undefined;
        if ty == missing_or_undefined {
            return ty;
        }
        if let crate::types::TypeData::Union { types, .. } = &self.store.get(ty).data
            && types.first() == Some(&missing_or_undefined)
        {
            return ty;
        }
        self.get_union_type_unprinted(&[ty, missing_or_undefined])
    }

    /// Ported from `Checker.removeMissingOrUndefinedType` (`checker.go:29099`).
    /// Exact optional properties distinguish an absent write from an explicit
    /// undefined value; ordinary optional properties use the same undefined type.
    pub(crate) fn remove_missing_or_undefined_type(&mut self, ty: TypeId) -> TypeId {
        if self.exact_optional_property_types {
            self.remove_missing_type(ty)
        } else {
            self.get_type_with_facts(ty, crate::flow::TypeFacts::NE_UNDEFINED)
        }
    }

    /// `removeMissingType` (`checker.go:14650`): drop `missingType` from a
    /// union — the write-position half of `exactOptionalPropertyTypes`.
    pub(crate) fn remove_missing_type(&mut self, ty: TypeId) -> TypeId {
        let missing = self.intrinsics.missing;
        if ty == missing {
            return self.intrinsics.never;
        }
        let crate::types::TypeData::Union { types, .. } = &self.store.get(ty).data else {
            return ty;
        };
        if !types.contains(&missing) {
            return ty;
        }
        let kept: Vec<TypeId> = types.iter().copied().filter(|&t| t != missing).collect();
        self.get_union_type(&kept)
    }

    /// Ported from `isOptionalDeclaration` (`utilities.go:299`), which is
    /// `ast.HasQuestionToken`.
    ///
    /// A parameter and a mapped type carry a dedicated `question_token`;
    /// everything else carries a `postfix_token` that is a `?` or a `!`, and only
    /// the `?` counts. **`p!: string` is not optional**, and reading the postfix
    /// token's presence rather than its kind would make it so.
    ///
    /// A parameter with an *initialiser* and no `?` is deliberately not optional
    /// here. It is optional to a *caller* — `isOptionalParameter`
    /// (`utilities.go:303`) says so — but its declared type is not
    /// `T | undefined`, because the initialiser supplies the value when the
    /// argument is absent. Conflating the two would append `| undefined` to
    /// every defaulted parameter in the corpus.
    pub(crate) fn is_optional_declaration(&self, declaration: NodeId) -> bool {
        let Some(node) = self.node_map.get(declaration) else { return false };
        let token = match node {
            Node::ParameterDeclaration(n) => n.question_token,
            Node::MappedTypeNode(n) => n.question_token,
            Node::NamedTupleMember(n) => n.question_token,
            Node::PropertyDeclaration(n) => n.postfix_token,
            Node::PropertySignatureDeclaration(n) => n.postfix_token,
            Node::MethodDeclaration(n) => n.postfix_token,
            Node::MethodSignatureDeclaration(n) => n.postfix_token,
            Node::JSDocParameterOrPropertyTag(n)
                if self.nodes.kind(declaration) == SyntaxKind::JSDocPropertyTag =>
            {
                // reparser.go::makeQuestionIfOptional supplies the synthetic ?.
                return n.is_bracketed
                    || matches!(n.type_expression,
                        Some(tsr_ast::TypeNode::JSDocTypeExpression(expression))
                            if matches!(expression.r#type,
                                Some(tsr_ast::TypeNode::JSDocOptionalType(_))));
            }
            _ => None,
        };
        token.is_some_and(|token| token.kind == SyntaxKind::QuestionToken)
    }

    /// Upstream's `isProperty` at `checker.go:16674`:
    ///
    /// ```go
    /// isProperty := ast.IsPropertyDeclaration(declaration) &&
    ///     !ast.HasAccessorModifier(declaration) ||
    ///     ast.IsPropertySignatureDeclaration(declaration)
    /// ```
    ///
    /// An `accessor` property is excluded because it is backed by a getter and a
    /// setter rather than by a field, so it is not a property for the purpose of
    /// `exactOptionalPropertyTypes`.
    ///
    /// Currently unobservable — see the module docs.
    fn is_property_for_optionality(&self, declaration: NodeId) -> bool {
        match self.node_map.get(declaration) {
            Some(Node::PropertyDeclaration(node)) => !node.modifiers.iter().any(|modifier| {
                matches!(modifier, tsr_ast::ModifierLike::Token(token)
                    if token.kind == SyntaxKind::AccessorKeyword)
            }),
            Some(Node::PropertySignatureDeclaration(_)) => true,
            Some(Node::JSDocParameterOrPropertyTag(_)) => {
                self.nodes.kind(declaration) == SyntaxKind::JSDocPropertyTag
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{checker::Checker, types::TypeData};

    #[test]
    fn optionality_preserves_identity_unless_strict_and_optional() {
        for strict in [false, true] {
            let arena = tsr_core::Arena::new();
            let parsed = tsr_parser::parse(&arena, "");
            let bound = tsr_binder::bind(
                &arena,
                parsed.source_file,
                &parsed.nodes,
                tsr_binder::FileInfo { name: "test.ts", text: "" },
            );
            let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
            checker.set_strict_null_checks(strict);
            let i = checker.intrinsics;
            for input in
                [i.undefined_widening, i.undefined, i.null, i.number, i.never, i.any, i.error]
            {
                assert_eq!(checker.add_optionality_ex(input, false, false), input);
                if !strict {
                    // Native addOptionalityEx returns the supplied identity;
                    // neither nullable flags nor printed spelling can replace it.
                    assert_eq!(checker.add_optionality_ex(input, false, true), input);
                }
            }
            if strict {
                for input in [i.null, i.number] {
                    let optional = checker.add_optionality_ex(input, false, true);
                    let TypeData::Union { types, .. } = &checker.store.get(optional).data else {
                        panic!("strict optional nullable/numeric input must gain undefined");
                    };
                    assert_eq!(types.as_slice(), &[i.undefined, input]);
                }
                assert_eq!(checker.add_optionality_ex(i.undefined, false, true), i.undefined);
                assert_eq!(checker.add_optionality_ex(i.never, false, true), i.undefined);
            }
        }
    }
}
