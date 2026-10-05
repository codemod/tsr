//! TS2339 — `Property '{0}' does not exist on type '{1}'.`
//!
//! `reportNonexistentProperty` (`checker.go:11530`), error node the property
//! name.
//!
//! # This is the second attempt, and the first one is why it exists
//!
//! `docs/architecture/checker-notes-diag2.md` §9 built this rule against the
//! bound *"the receiver carries `members: Some(_)`"*, measured **2 conversions
//! against 254 wrong lines**, and refused it. The diagnosis it left behind is
//! the design of this file:
//!
//! > An absent property and an unbuilt members table are the same `None`, and no
//! > predicate over the *type* separates them.
//!
//! [`crate::member_completeness`] is the separation, and it works by asking the
//! **walk** rather than the type. Everything else here is the four declines
//! §9 correctly said were *not* what refused it — they were never the problem,
//! but each is a wrong code at a right position and the position is what the
//! suite compares.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::SymbolFlags;
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

/// `ast.SymbolFlagsBlockScoped` (`internal/ast/symbolflags.go:77`) — the names
/// that live in the global *scope* without being properties of the global
/// *object*.
const BLOCK_SCOPED: SymbolFlags =
    SymbolFlags::BLOCK_SCOPED_VARIABLE.union(SymbolFlags::CLASS).union(SymbolFlags::ENUM);

impl Checker<'_, '_> {
    /// The nonexistent-property check for one property access.
    ///
    /// **No `.js` decline**, and §37's audit is why. Every other rule this
    /// session declines JS because JSDoc supplies types this port does not
    /// parse — but that argument is about *annotations*, and this rule reports
    /// on a member's **absence** from a table the binder built from real
    /// declarations. JSDoc adds no members. Removing the decline here and in
    /// `crate::type_argument_arity`, which declined for the same borrowed
    /// reason, is worth 3 cases.
    pub(crate) fn check_nonexistent_property(&mut self, node: NodeId, ambient: bool) {
        // Pinned `checkPropertyAccessExpressionOrQualifiedName` reports this
        // semantic error even with parse diagnostics. Existing receiver-image
        // completeness and unsupported-access declines still own the work.
        if ambient {
            return;
        }
        // **`a["nope"]` is the same question.** Eight of upstream's twelve
        // `checker.go` sites for this code are the element-access cluster
        // (`checker.go:27074`–`:27196`), and this rule saw none of them. Only a
        // **string-literal** argument is admitted: a computed index names no
        // particular property, which is the bound §326 took for the readonly
        // rule and `declared_members_are_complete` takes for index signatures.
        // §401.
        let (receiver, name_text, name_id, optional) = match self.node_map.get(node) {
            Some(Node::PropertyAccessExpression(access)) => {
                let (Some(receiver), Some(member)) = (access.expression, access.name) else {
                    return;
                };
                // A private name has its own diagnostics (TS2339 is reported for
                // one, but so are TS18013 and TS18016 from `crate::members`' own
                // note), and the corpus's private-name cases are a row of their
                // own.
                let tsr_ast::MemberName::Identifier(name) = member else { return };
                // Native `right.Text() != ""` (`checker.go:11345`) declines a
                // missing recovery name, not all accesses in a malformed file.
                if name.text.is_empty() {
                    return;
                }
                let Some(name_id) = name.node_id else { return };
                // Native `IsPartOfTypeNode` excludes interface extends and
                // implements from checked value property accesses. Use the
                // name slot so nested `typeof` type arguments stay distinct.
                if self.identifier_in_non_emitting_heritage_clause(name_id) {
                    return;
                }
                (receiver, name.text, name_id, access.question_dot_token.is_some())
            }
            Some(Node::ElementAccessExpression(access)) => {
                // Malformed element access is not part of this proven property
                // recovery boundary; retain its existing parse-error decline.
                if self.file_has_parse_errors {
                    return;
                }
                let (Some(receiver), Some(argument)) =
                    (access.expression, access.argument_expression)
                else {
                    return;
                };
                let Some(id) = argument.node_id() else { return };
                let Some(Node::StringLiteral(literal)) = self.node_map.get(id) else { return };
                (receiver, literal.text, id, access.question_dot_token.is_some())
            }
            _ => return,
        };
        // An optional chain strips `null`/`undefined` before the lookup, and
        // this rule reads the receiver's type directly — so the two disagree
        // exactly where `?.` is written.
        if optional {
            return;
        }

        let Some(receiver_id) = receiver.node_id() else { return };
        if self.file_has_parse_errors {
            // Constructor recovery can leave `this.x = ...` as a statement
            // where native ended the body and owns class property declarations.
            // This port cannot certify that recovered write's receiver image.
            if self.nodes.kind(receiver_id) == SyntaxKind::ThisKeyword
                && self.is_write_only_access(node)
                && self
                    .get_this_container(node, false)
                    .is_some_and(|container| self.nodes.kind(container) == SyntaxKind::Constructor)
            {
                return;
            }
            if let Some(Node::Identifier(identifier)) = self.node_map.get(receiver_id) {
                let Some(symbol) = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    receiver_id,
                    identifier.text,
                    SymbolFlags::VALUE,
                ) else {
                    return;
                };
                // `getExplicitTypeOfSymbol` (pinned flow.go:2155) is also the
                // assertion-effect supplier. Its unsupported for-of/mapped
                // origins cannot certify a miss merely by flowed == declared.
                if self.get_explicit_type_of_symbol(symbol).is_none() {
                    return;
                }
            }
        }
        let receiver_type = self.check_expression(receiver);
        if self.global_this_member_is_not_reported(receiver_type, name_text) {
            return;
        }
        if !self.receiver_type_is_the_declared_one(receiver_id, receiver_type) {
            return;
        }
        let apparent_receiver =
            if matches!(self.node_map.get(node), Some(Node::ElementAccessExpression(_))) {
                // getPropertyTypeForIndexType (checker.go:27001) reports a literal
                // miss on an apparent primitive or union only through its
                // noImplicitAny 7053 family, never as this dotted-name TS2339.
                // Element access keeps the declared-table certification only.
                if !self.declared_members_are_complete(receiver_type)
                    || self.get_property_of_type(receiver_type, name_text).is_some()
                    || self.no_index_signature_admits(receiver_type, name_text) != Some(true)
                {
                    return;
                }
                receiver_type
            } else {
                // A union receiver that is a narrowable reference is the one
                // shape whose flow type this port cannot certify: the narrowing
                // arms decline to the declared union silently, and a declined
                // narrowing reads exactly like no narrowing (the flowed == declared
                // test above). `boolean` is a union only by representation.
                if matches!(
                    self.store.get(receiver_type).data,
                    crate::types::TypeData::Union { .. }
                ) && !self
                    .store
                    .get(receiver_type)
                    .flags
                    .contains(crate::flags::TypeFlags::BOOLEAN)
                    && matches!(
                        self.nodes.kind(receiver_id),
                        SyntaxKind::Identifier
                            | SyntaxKind::ThisKeyword
                            | SyntaxKind::PropertyAccessExpression
                            | SyntaxKind::ElementAccessExpression
                            | SyntaxKind::ParenthesizedExpression
                    )
                {
                    return;
                }
                let Some(apparent) = self.property_is_known_absent(node, receiver_type, name_text)
                else {
                    return;
                };
                apparent
            };
        if let crate::types::TypeData::Anonymous { symbol, .. } =
            &self.store.get(receiver_type).data
        {
            // `getPropertyOfType`/`reportNonexistentProperty`, native pin
            // 5b1047d10d32e7d5b446be4de56b126ff42f82bb: absence is usable only
            // after the receiver's declaration/export ownership is complete.
            // Read only this Program symbol's declarations and dotted module
            // bodies, after a miss; no member forcing, graph cache or new image.
            // Missing names/bodies are unsupported recovery, not completion.
            for &declaration in &self.binder.symbols().get(*symbol).declarations {
                let mut declaration = self.node_map.get(declaration);
                while let Some(Node::ModuleDeclaration(module)) = declaration {
                    if module.name.is_none_or(|name| {
                        matches!(name, tsr_ast::ModuleName::Identifier(name) if name.text.is_empty())
                    }) || module.body.is_none()
                    {
                        return;
                    }
                    declaration = module.body.map(Node::from);
                }
            }
        }
        // getPropertyTypeForIndexType (5b1047d1 checker.go:27129-27182).
        // Original source-module SymbolId/TypeId publication, not a class,
        // object/JS literal, clone image or active module construction. Literal
        // indices have no union constituent's SuppressNoImplicitAnyError state
        // (26979); an applicable index is answered before this miss policy.
        let module_element_miss =
            matches!(self.node_map.get(node), Some(Node::ElementAccessExpression(_)))
                && match self.store.get(receiver_type).data {
                    crate::types::TypeData::Anonymous { symbol, .. } => {
                        self.symbol_types.get(&symbol) == Some(&receiver_type)
                            && !self
                                .resolutions
                                .on_stack(symbol, crate::resolution::PropertyName::Type)
                            && !self.module_value_clones.contains_key(&receiver_type)
                            && !self.js_literal_types.contains(&receiver_type)
                            && !self.is_object_literal_type(receiver_type)
                            && self
                                .binder
                                .symbols()
                                .get(symbol)
                                .flags
                                .contains(SymbolFlags::VALUE_MODULE)
                            && !self
                                .binder
                                .symbols()
                                .get(symbol)
                                .flags
                                .intersects(SymbolFlags::MODULE_EXPORTS | SymbolFlags::CLASS)
                            && self.binder.symbols().get(symbol).value_declaration.is_some_and(
                                |declaration| {
                                    self.nodes.kind(declaration) == tsr_ast::SyntaxKind::SourceFile
                                },
                            )
                            && self
                                .get_index_infos_of_type(receiver_type)
                                .is_some_and(|infos| infos.is_empty())
                    }
                    _ => false,
                };
        // Native 27147 guards static and spelling suggestions as well as the
        // final implicit-any index error. Dot property policy stays unchanged.
        if module_element_miss && !self.no_implicit_any {
            return;
        }
        // **The other side of the class is not silence, it is TS2576.**
        // `other_side_of_class_has`'s own doc names the code; the caller used
        // the answer only to suppress TS2339. `this.Foo()` on a static `Foo` is
        // upstream's suggestion form, and the instance→static direction is the
        // one the corpus writes. §447.
        if self.other_side_of_class_has(receiver_type, name_text) {
            let statically_declared = self.owning_symbol_of(receiver_type).is_some_and(|symbol| {
                let entry = self.binder.symbols().get(symbol);
                entry.exports.contains_key(name_text) && !entry.members.contains_key(name_text)
            });
            if !statically_declared {
                return;
            }
            let class_name = self
                .owning_symbol_of(receiver_type)
                .map(|symbol| self.binder.symbols().get(symbol).name.to_string())
                .unwrap_or_default();
            let Some(file) = self.source_file_of_for_diagnostics(name_id) else { return };
            let span = self.error_span(name_id);
            let printed = self.type_to_string(receiver_type);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::PROPERTY_0_DOES_NOT_EXIST_ON_TYPE_1_DID_YOU_MEAN_TO_ACCESS_THE_STATIC_MEMBER_2_INSTEAD,
                    span,
                    [name_text.to_string(), printed, format!("{class_name}.{name_text}")],
                ),
            );
            return;
        }
        // `getSuggestedLibForNonExistentProperty` (`checker.go:11593`) is asked
        // before the spelling suggestion, keyed by the apparent type's symbol.
        if !module_element_miss
            && let Some(lib) =
                self.suggested_lib_for_nonexistent_property(name_text, apparent_receiver)
        {
            let Some(file) = self.source_file_of_for_diagnostics(name_id) else { return };
            let span = self.error_span(name_id);
            let printed = self.type_to_string(receiver_type);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::PROPERTY_0_DOES_NOT_EXIST_ON_TYPE_1_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_COMPILER_OPTION_TO_2_OR_LATER,
                    span,
                    [name_text.to_string(), printed, lib.to_string()],
                ),
            );
            return;
        }
        // `reportNonexistentProperty`'s suggestion arm: a near-miss member name
        // is **TS2551**, not TS2339. §33 made the same move for TS2304/TS2552 —
        // the spelling algorithm is already exact, so reporting the code it
        // selects costs one message.
        let candidates = if module_element_miss {
            self.get_property_names_of_type(receiver_type).unwrap_or_default()
        } else {
            self.apparent_property_names(receiver_type, apparent_receiver)
        };
        if let Some(suggestion) = crate::check::spelling_suggestion(
            name_text,
            &candidates.iter().map(String::as_str).collect::<Vec<_>>(),
        ) {
            let suggestion = suggestion.to_string();
            let Some(file) = self.source_file_of_for_diagnostics(name_id) else { return };
            let span = self.error_span(name_id);
            let printed = self.type_to_string(receiver_type);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::PROPERTY_0_DOES_NOT_EXIST_ON_TYPE_1_DID_YOU_MEAN_2,
                    span,
                    [name_text.to_string(), printed, suggestion],
                ),
            );
            return;
        }
        // Native's outer 7053 chain is at the whole expression; 27196's
        // argument-level 2339 fallback has no accessExpression and is not this
        // original-source-module expression branch.
        if module_element_miss
            && let Some(Node::ElementAccessExpression(access)) = self.node_map.get(node)
        {
            let Some(index) = access.argument_expression else { return };
            let index_type = self.check_expression(index);
            let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::ELEMENT_IMPLICITLY_HAS_AN_ANY_TYPE_BECAUSE_EXPRESSION_OF_TYPE_0_CAN_T_BE_USED_TO_INDEX_TYPE_1,
                    self.error_span(node),
                    [self.type_to_string(index_type), self.type_to_string(receiver_type)],
                ),
            );
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(name_id) else { return };
        let span = self.error_span(name_id);
        let printed = self.type_to_string(receiver_type);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::PROPERTY_0_DOES_NOT_EXIST_ON_TYPE_1,
                span,
                [name_text.to_string(), printed],
            ),
        );
    }

    /// Is `name` certainly absent from `getPropertyOfType(getApparentType(
    /// receiver))`, as `checkPropertyAccessExpressionOrQualifiedName`
    /// (`checker.go:11258`) asks it? `Some(apparent)` is the certified miss and
    /// the apparent type whose names feed the spelling suggestion; `None` is a
    /// present property or a table this port cannot certify.
    ///
    /// Three receiver shapes, each read through the existing completeness walk:
    ///
    /// - a **primitive** is read through its global apparent interface
    ///   (`getApparentType`, `checker.go:21745-21751`, ported as
    ///   [`Checker::apparent_type`]);
    /// - a **union** is `getUnionOrIntersectionProperty` →
    ///   `createUnionOrIntersectionProperty`: each constituent's apparent type is
    ///   asked, and one constituent without the name makes the property partial,
    ///   which `getPropertyOfUnionOrIntersectionType` answers `nil`. An object
    ///   literal constituent makes it write-partial instead (a readable
    ///   `undefined`), and a nullable constituent belongs to
    ///   `checkNonNullExpression`'s diagnostics, so both decline;
    /// - anything else is the declared table the walk certifies.
    ///
    /// No cache: the walk and lookups are the existing members subsystem's, run
    /// once per checked access after the type answer, as before.
    fn property_is_known_absent(
        &mut self,
        access: NodeId,
        receiver: TypeId,
        name: &str,
    ) -> Option<TypeId> {
        use crate::flags::TypeFlags;
        let flags = self.store.get(receiver).flags;
        if let crate::types::TypeData::Union { types, .. } = &self.store.get(receiver).data {
            let types = types.clone();
            let mut missing = false;
            for constituent in types {
                let constituent_flags = self.store.get(constituent).flags;
                if constituent_flags.intersects(TypeFlags::NEVER) {
                    continue;
                }
                if constituent_flags
                    .intersects(TypeFlags::NULLABLE | TypeFlags::ANY | TypeFlags::UNKNOWN)
                    || self.is_object_literal_type(constituent)
                {
                    return None;
                }
                let apparent = self.primitive_apparent_type(constituent);
                if apparent == self.intrinsics.error {
                    return None;
                }
                missing |= self.apparent_type_lacks(apparent, name)?;
            }
            return missing.then_some(receiver);
        }
        let apparent = if flags.intersects(TypeFlags::PRIMITIVE) {
            self.primitive_apparent_type(receiver)
        } else if let Some(&symbol) = self.type_parameter_symbols.get(&receiver) {
            // A declared type parameter (the polymorphic `this` keeps the
            // class-table road below). Certify the receiver: declared by a
            // node enclosing this access. One that escaped its declaration is
            // an uninstantiated inference result here, not upstream's type.
            let declared_here =
                self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
                    self.nodes
                        .parent(declaration)
                        .is_some_and(|owner| self.nodes.ancestors(access).any(|node| node == owner))
                });
            if !declared_here {
                return None;
            }
            // getApparentType's head: a type parameter reads through its base
            // constraint, `unknown` when it has none.
            let mut apparent = self.apparent_type(receiver);
            if self.store.get(apparent).flags.intersects(TypeFlags::TYPE_PARAMETER) {
                // No (or a circular) base constraint: `unknownType`, which is
                // `{}` outside strictNullChecks (checker.go:21754-21759).
                apparent = if self.strict_null_checks {
                    self.intrinsics.unknown
                } else {
                    self.intrinsics.empty_object
                };
            }
            if apparent == self.intrinsics.unknown {
                return Some(apparent);
            }
            self.primitive_apparent_type(apparent)
        } else {
            receiver
        };
        if apparent == self.intrinsics.empty_object {
            // The canonical empty object: no own members, the global
            // `Object` augment only (getPropertyOfTypeEx).
            return self.get_property_of_type(apparent, name).is_none().then_some(apparent);
        }
        self.apparent_type_lacks(apparent, name)?.then_some(apparent)
    }

    /// `getPropertyOfType(apparent, name) == nil` with no applicable index
    /// signature, or `None` when this port cannot certify `apparent`'s table.
    ///
    /// The completeness walk certifies most tables. Its declines include two
    /// shapes every global apparent interface has (`Number`, `String`,
    /// `Object`, `Function`, `Date`, ...), and neither hides a dotted name:
    ///
    /// - a merged `declare var X: XConstructor` is the value side; it adds no
    ///   member to the instance table (`resolveDeclaredMembers` reads
    ///   `getMembersOfSymbol` only);
    /// - a `[Symbol.x]` member late-binds to a unique-symbol key
    ///   (`getPropertyNameFromType`), which no identifier text equals.
    ///
    /// Index signatures are asked per name, as `getApplicableIndexInfoForName`
    /// (`checker.go:11330`) does: a `string` key admits every name, a `number`
    /// key only a numeric one, a `symbol` key none; any other key declines.
    fn apparent_type_lacks(&mut self, apparent: TypeId, name: &str) -> Option<bool> {
        // A found property needs no certificate; the completeness walks are
        // the expensive half and only a miss pays for them.
        if self.get_property_of_type(apparent, name).is_some() {
            return Some(false);
        }
        if self.declared_members_are_complete(apparent) {
            // A class's static side is certified without reading its
            // `static [k: string]` signatures; ask them per name too.
            return self.no_index_signature_admits(apparent, name);
        }
        let crate::types::TypeData::Named { members: Some(owner), .. } =
            self.store.get(apparent).data
        else {
            return None;
        };
        let declarations = self.binder.symbols().get(owner).declarations.to_vec();
        let mut interfaces = 0usize;
        for declaration in declarations {
            match self.node_map.get(declaration) {
                Some(Node::InterfaceDeclaration(interface)) => {
                    // Instantiation never changes member names (§41's
                    // audit in `crate::member_completeness`).
                    if !interface.heritage_clauses.is_empty()
                        || !interface.members.iter().all(|member| member_name_is_bound(*member))
                    {
                        return None;
                    }
                    interfaces += 1;
                }
                Some(Node::VariableDeclaration(_)) => {}
                _ => return None,
            }
        }
        if interfaces == 0 {
            return None;
        }
        self.no_index_signature_admits(apparent, name)
    }

    /// `getApplicableIndexInfoForName(apparent, name) == nil`: a `string` key
    /// admits every name, a `number` key only a numeric one, a `symbol` key
    /// none; any other key, or unreadable signatures, decline.
    fn no_index_signature_admits(&mut self, apparent: TypeId, name: &str) -> Option<bool> {
        for info in self.get_index_infos_of_type(apparent)? {
            if info.key == self.intrinsics.string {
                return Some(false);
            }
            if info.key == self.intrinsics.number {
                if crate::index_signatures::is_numeric_literal_name(name) {
                    return Some(false);
                }
            } else if info.key != self.intrinsics.es_symbol {
                return None;
            }
        }
        Some(true)
    }

    /// `getApparentType` (`checker.go:21745-21751`) for a primitive; any other
    /// type is its own answer here.
    fn primitive_apparent_type(&mut self, id: TypeId) -> TypeId {
        if self.store.get(id).flags.intersects(
            crate::flags::TypeFlags::STRING_LIKE
                | crate::flags::TypeFlags::NUMBER_LIKE
                | crate::flags::TypeFlags::BIG_INT_LIKE
                | crate::flags::TypeFlags::BOOLEAN_LIKE
                | crate::flags::TypeFlags::ES_SYMBOL_LIKE,
        ) {
            self.apparent_type(id)
        } else {
            id
        }
    }

    /// `getPropertiesOfType(containingType)` for
    /// `getSuggestedSymbolForNonexistentProperty` (`checker.go:11608`): a
    /// union's properties are the first constituent's that every constituent
    /// has (`getPropertiesOfUnionOrIntersectionType`).
    fn apparent_property_names(&mut self, receiver: TypeId, apparent: TypeId) -> Vec<String> {
        let crate::types::TypeData::Union { types, .. } = &self.store.get(receiver).data else {
            return self.property_names_of(apparent);
        };
        let types: Vec<TypeId> = types
            .iter()
            .copied()
            .filter(|&t| !self.store.get(t).flags.intersects(crate::flags::TypeFlags::NEVER))
            .collect();
        let Some((&first, rest)) = types.split_first() else { return Vec::new() };
        let first = self.primitive_apparent_type(first);
        let rest: Vec<TypeId> = rest.iter().map(|&t| self.primitive_apparent_type(t)).collect();
        let mut names = self.property_names_of(first);
        names.retain(|name| rest.iter().all(|&t| self.get_property_of_type(t, name).is_some()));
        names
    }

    /// `getSuggestedLibForNonExistentProperty` (`checker.go:11593`): the first
    /// `getFeatureMap` entry, under the apparent type's symbol name, whose
    /// property list holds `name`. A union's apparent type has no symbol.
    fn suggested_lib_for_nonexistent_property(
        &self,
        name: &str,
        apparent: TypeId,
    ) -> Option<&'static str> {
        let (crate::types::TypeData::Named { members: Some(symbol), .. }
        | crate::types::TypeData::Anonymous { symbol, .. }) = self.store.get(apparent).data
        else {
            return None;
        };
        let container = &self.binder.symbols().get(symbol).name;
        let index =
            LIB_FEATURE_PROPERTIES.binary_search_by_key(container, |(feature, _)| feature).ok()?;
        LIB_FEATURE_PROPERTIES[index]
            .1
            .iter()
            .find(|(_, properties)| properties.contains(&name))
            .map(|(lib, _)| *lib)
    }

    /// Whether `globalThis.<name>` is a missing property upstream stays silent
    /// about — which is nearly all of them.
    ///
    /// `globalThis` does not reach `reportNonexistentProperty` at all. It has
    /// its own arm several branches earlier
    /// (`internal/checker/checker.go:11337-11344`), and the arm's answer is
    /// `anyType`:
    ///
    /// ```go
    /// if leftType.symbol == c.globalThisSymbol {
    ///     globalSymbol := c.globalThisSymbol.Exports[right.Text()]
    ///     if globalSymbol != nil && globalSymbol.Flags&ast.SymbolFlagsBlockScoped != 0 {
    ///         c.error(right, diagnostics.Property_0_does_not_exist_on_type_1, …)
    ///     } else if c.noImplicitAny {
    ///         c.error(right, diagnostics.Element_implicitly_has_an_any_type_because_type_0_has_no_index_signature, …)
    ///     }
    ///     return c.anyType
    /// }
    /// ```
    ///
    /// So TS2339 is reported for exactly one shape: a name that **is** a global
    /// and is `let`/`const`/`class`/`enum`, because those live in the global
    /// *scope* without being properties of the global *object*. An undeclared
    /// name is TS7017 under `noImplicitAny` and silence otherwise — never
    /// TS2339.
    ///
    /// `SymbolFlagsBlockScoped` is `BlockScopedVariable | Class | Enum`
    /// (`internal/ast/symbolflags.go:77`).
    ///
    /// **Why this landed with the `declare global` merge rather than before
    /// it.** Until globals carried what a `declare global` block declares, this
    /// port's `typeof globalThis` had nothing in it that
    /// [`Checker::declared_members_are_complete`] would call complete, so the
    /// rule declined on that gate and the missing arm was invisible.
    /// `compiler/extendGlobalThis` — which augments `namespace globalThis` and
    /// then writes `globalThis.tests` where `test` was declared — is the case
    /// that turned it up, as a `diagnostics` regression of exactly one. Its
    /// `.types` baseline records `>globalThis.tests : any`, with no error, and
    /// the case runs `@strict: false` so the `noImplicitAny` arm is off too.
    ///
    /// **TS7017 is not ported here.** It is a different code with its own row,
    /// and reporting it from this file would put an implicit-any diagnostic in
    /// the nonexistent-property rule. What this function owes is the silence.
    ///
    /// # The block-scoped branch is unreachable today, and is kept anyway
    ///
    /// Measured: `let blockScoped = 1` in a script, then
    /// `globalThis.blockScoped` from a module, reports **nothing** — with this
    /// guard and without it. §33 of `checker-notes-narrow.md` mints
    /// `typeof globalThis` when the *name* fails to resolve, and a minted type
    /// carries no members table, so `declared_members_are_complete` declines
    /// before this function is consulted.
    ///
    /// So the `TS2339`-for-a-block-scoped-global arm below cannot fire. It is
    /// kept because it is upstream's rule and because it becomes live the
    /// moment §33's mint grows members — writing the silence without it would
    /// make a future members table silently wrong. The divergence is pinned by
    /// `a_block_scoped_globalthis_member_records_a_known_divergence` in
    /// `tests/real_repo_regressions.rs`, whose assertion flips when it closes.
    fn global_this_member_is_not_reported(
        &mut self,
        receiver_type: crate::types::TypeId,
        name: &str,
    ) -> bool {
        if Some(receiver_type) != self.global_this_type {
            return false;
        }
        !self
            .binder
            .global(name)
            .is_some_and(|symbol| self.binder.symbols().get(symbol).flags.intersects(BLOCK_SCOPED))
    }

    /// Is the receiver's type the one its declaration says, rather than one
    /// **narrowing** or **inference** produced?
    ///
    /// The completeness predicate answers for a type. This answers for the
    /// *expression that produced it*, and it is the second half of the same
    /// question: a table can be complete and still be the wrong table, because
    /// this port narrowed where upstream did not or inferred a different return.
    ///
    /// Three shapes are declined, each with its case:
    ///
    /// - **A call receiver.** `c.foo().bar()` — `fluentClasses` — returns the
    ///   polymorphic `this` type, which this port does not model. It was this
    ///   rule's last remaining *loss*, and a loss is the one outcome the bar
    ///   forbids outright.
    /// - ~~**A dotted name.**~~ **DELETED, §37's audit, +2.** It was declined
    ///   because `narrowingOfDottedNames` narrows `a.b` by a guard on `a.b`
    ///   itself and this port's flow graph keys on a narrower set of references.
    ///   That is still true, but §35 made the *namespace* receiver decidable —
    ///   `N.x.y` is not narrowed by anything — and the decline was costing more
    ///   than the narrowing family costs. The narrowing cases are still wrong
    ///   lines; they are outnumbered.
    /// - **A narrowed identifier**, detected by comparing the flow type against
    ///   the symbol's declared type. `controlFlowInstanceof`,
    ///   `narrowByClauseExpressionInSwitchTrue7` and `typePredicateInLoop` are
    ///   the family, and it is exactly the family `checker-notes-narrow.md`
    ///   owns.
    ///
    /// A `this` receiver is **not** declined: `thisBinding` and `statics` are
    /// conversions and `this` in a class body is not narrowed by anything this
    /// port models.
    fn receiver_type_is_the_declared_one(&mut self, receiver: NodeId, flowed: TypeId) -> bool {
        if self.receiver_roots_in_node_format_import(receiver) {
            return false;
        }
        match self.node_map.get(receiver) {
            Some(Node::CallExpression(_)) => false,
            Some(Node::Identifier(identifier)) => {
                let text = identifier.text;
                let Some(symbol) = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    receiver,
                    text,
                    SymbolFlags::VALUE,
                ) else {
                    return false;
                };
                let symbol = self.binder.merged_symbol(symbol);
                self.get_type_of_symbol(symbol) == flowed
            }
            _ => true,
        }
    }

    /// Does the receiver's dotted chain start at a default or namespace import
    /// under Node16..NodeNext?
    ///
    /// `getTargetOfImportClause`/`getTargetOfNamespaceImport` (checker.go)
    /// answer an ESM file's import of a CommonJS-format file with the whole
    /// `module.exports` there. That usage/target format road is unported
    /// (`missing_default_established` declines it the same way), so neither
    /// the alias's type nor its `default` member is certified
    /// (nodeNextCjsNamespaceImportDefault2).
    fn receiver_roots_in_node_format_import(&self, receiver: NodeId) -> bool {
        if !(tsr_core::ModuleKind::Node16..=tsr_core::ModuleKind::NodeNext)
            .contains(&self.module_kind)
        {
            return false;
        }
        let mut root = receiver;
        while let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(root) {
            let Some(left) = access.expression.and_then(|left| left.node_id()) else {
                return false;
            };
            root = left;
        }
        let Some(Node::Identifier(identifier)) = self.node_map.get(root) else { return false };
        self.binder
            .resolve_name(self.nodes, self.node_map, root, identifier.text, SymbolFlags::VALUE)
            .is_some_and(|symbol| {
                self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
                    matches!(
                        self.nodes.kind(declaration),
                        SyntaxKind::ImportClause | SyntaxKind::NamespaceImport
                    )
                })
            })
    }

    /// Is `name` declared on the class's *other* side?
    ///
    /// `class C { x = 1 }` accessed as `C.x` is TS2576
    /// (`Property_0_is_a_static_member_of_type_1`), not TS2339. The check is on
    /// the symbol's two tables, which is where the binder already keeps the two
    /// sides apart.
    fn other_side_of_class_has(&mut self, receiver: TypeId, name: &str) -> bool {
        let Some(symbol) = self.owning_symbol_of(receiver) else { return false };
        let entry = self.binder.symbols().get(symbol);
        entry.members.contains_key(name) || entry.exports.contains_key(name)
    }

    /// Every property name the completeness walk can see on a type.
    ///
    /// Only ever called after [`Checker::declared_members_are_complete`] has
    /// answered `true`, so the list is the whole list.
    /// `pub(crate)` for §933's `X[keyof X]` arm in `crate::declared`, which
    /// needs exactly this walk — own members plus bases, cycle-guarded.
    pub(crate) fn property_names_of(&mut self, receiver: TypeId) -> Vec<String> {
        self.resolve_mapped_type_members(receiver);
        // getPropertiesOfType preserves the complete captured property order,
        // including original literals as well as instantiated object images.
        if let Some((properties, _)) = self.anonymous_properties.get(&receiver) {
            return properties.iter().map(|property| property.name.clone()).collect();
        }
        let Some(symbol) = self.owning_symbol_of(receiver) else { return Vec::new() };
        let mut names = Vec::new();
        let mut visiting = Vec::new();
        self.collect_property_names(symbol, &mut names, &mut visiting);
        names
    }

    fn collect_property_names(
        &mut self,
        owner: SymbolIdAlias,
        names: &mut Vec<String>,
        visiting: &mut Vec<SymbolIdAlias>,
    ) {
        if visiting.contains(&owner) {
            return;
        }
        visiting.push(owner);
        for (member, id) in &self.binder.symbols().get(owner).members {
            if self.binder.symbols().get(*id).flags.intersects(SymbolFlags::VALUE) {
                names.push((*member).to_string());
            }
        }
        let Some(bases) = self.base_symbols_of(owner) else { return };
        for base in bases {
            self.collect_property_names(base, names, visiting);
        }
    }

    /// The symbol a `Named` type's members belong to.
    fn owning_symbol_of(&self, receiver: TypeId) -> Option<SymbolIdAlias> {
        match &self.store.get(receiver).data {
            crate::types::TypeData::Named { members, .. } => *members,
            _ => None,
        }
    }
}

/// `getFeatureMap` (`utilities.go:1292`), sorted by container name; each
/// container's entries keep upstream's order, which decides the first match.
/// One `FeatureMapEntry`: a lib and the properties it introduces.
type LibFeature = (&'static str, &'static [&'static str]);

#[rustfmt::skip]
const LIB_FEATURE_PROPERTIES: &[(&str, &[LibFeature])] = &[
    (
        "Array",
        &[
            ("es2015", &["find", "findIndex", "fill", "copyWithin", "entries", "keys", "values"]),
            ("es2016", &["includes"]),
            ("es2019", &["flat", "flatMap"]),
            ("es2022", &["at"]),
            (
                "es2023",
                &["findLastIndex", "findLast", "toReversed", "toSorted", "toSpliced", "with"],
            ),
        ],
    ),
    (
        "ArrayBuffer",
        &[(
            "es2024",
            &[
                "maxByteLength",
                "resizable",
                "resize",
                "detached",
                "transfer",
                "transferToFixedLength",
            ],
        )],
    ),
    ("ArrayConstructor", &[("es2015", &["from", "of"]), ("esnext", &["fromAsync"])]),
    ("AsyncDisposableStack", &[("esnext", &[])]),
    ("AsyncGenerator", &[("es2018", &[])]),
    ("AsyncGeneratorFunction", &[("es2018", &[])]),
    ("AsyncIterable", &[("es2018", &[])]),
    ("AsyncIterableIterator", &[("es2018", &[])]),
    ("AsyncIterator", &[("es2015", &[])]),
    (
        "Atomics",
        &[
            (
                "es2017",
                &[
                    "add",
                    "and",
                    "compareExchange",
                    "exchange",
                    "isLockFree",
                    "load",
                    "or",
                    "store",
                    "sub",
                    "wait",
                    "notify",
                    "xor",
                ],
            ),
            ("es2024", &["waitAsync"]),
        ],
    ),
    ("BigInt", &[("es2020", &[])]),
    (
        "BigInt64Array",
        &[
            ("es2020", &[]),
            ("es2022", &["at"]),
            (
                "es2023",
                &["findLastIndex", "findLast", "toReversed", "toSorted", "toSpliced", "with"],
            ),
        ],
    ),
    (
        "BigUint64Array",
        &[
            ("es2020", &[]),
            ("es2022", &["at"]),
            (
                "es2023",
                &["findLastIndex", "findLast", "toReversed", "toSorted", "toSpliced", "with"],
            ),
        ],
    ),
    (
        "DataView",
        &[
            ("es2020", &["setBigInt64", "setBigUint64", "getBigInt64", "getBigUint64"]),
            ("es2025", &["setFloat16", "getFloat16"]),
        ],
    ),
    ("Date", &[("esnext", &["toTemporalInstant"])]),
    ("DateTimeFormat", &[("es2017", &["formatToParts"])]),
    ("DisposableStack", &[("esnext", &[])]),
    ("Error", &[("es2022", &["cause"])]),
    ("ErrorConstructor", &[("esnext", &["isError"])]),
    ("Float16Array", &[("es2025", &[])]),
    (
        "Float32Array",
        &[
            ("es2022", &["at"]),
            (
                "es2023",
                &["findLastIndex", "findLast", "toReversed", "toSorted", "toSpliced", "with"],
            ),
        ],
    ),
    (
        "Float64Array",
        &[
            ("es2022", &["at"]),
            (
                "es2023",
                &["findLastIndex", "findLast", "toReversed", "toSorted", "toSpliced", "with"],
            ),
        ],
    ),
    (
        "Int16Array",
        &[
            ("es2022", &["at"]),
            (
                "es2023",
                &["findLastIndex", "findLast", "toReversed", "toSorted", "toSpliced", "with"],
            ),
        ],
    ),
    (
        "Int32Array",
        &[
            ("es2022", &["at"]),
            (
                "es2023",
                &["findLastIndex", "findLast", "toReversed", "toSorted", "toSpliced", "with"],
            ),
        ],
    ),
    (
        "Int8Array",
        &[
            ("es2022", &["at"]),
            (
                "es2023",
                &["findLastIndex", "findLast", "toReversed", "toSorted", "toSpliced", "with"],
            ),
        ],
    ),
    (
        "Intl",
        &[
            ("es2018", &["PluralRules"]),
            ("es2020", &["RelativeTimeFormat", "Locale", "DisplayNames"]),
            ("es2021", &["ListFormat", "DateTimeFormat"]),
            ("es2022", &["Segmenter"]),
            ("es2025", &["DurationFormat"]),
        ],
    ),
    ("Iterator", &[("es2015", &[])]),
    (
        "Map",
        &[
            ("es2015", &["entries", "keys", "values"]),
            ("esnext", &["getOrInsert", "getOrInsertComputed"]),
        ],
    ),
    ("MapConstructor", &[("es2024", &["groupBy"])]),
    (
        "Math",
        &[
            (
                "es2015",
                &[
                    "clz32", "imul", "sign", "log10", "log2", "log1p", "expm1", "cosh", "sinh",
                    "tanh", "acosh", "asinh", "atanh", "hypot", "trunc", "fround", "cbrt",
                ],
            ),
            ("es2025", &["f16round"]),
        ],
    ),
    (
        "NumberConstructor",
        &[(
            "es2015",
            &["isFinite", "isInteger", "isNaN", "isSafeInteger", "parseFloat", "parseInt"],
        )],
    ),
    ("NumberFormat", &[("es2018", &["formatToParts"])]),
    (
        "ObjectConstructor",
        &[
            ("es2015", &["assign", "getOwnPropertySymbols", "keys", "is", "setPrototypeOf"]),
            ("es2017", &["values", "entries", "getOwnPropertyDescriptors"]),
            ("es2019", &["fromEntries"]),
            ("es2022", &["hasOwn"]),
            ("es2024", &["groupBy"]),
        ],
    ),
    ("Promise", &[("es2015", &[]), ("es2018", &["finally"])]),
    (
        "PromiseConstructor",
        &[
            ("es2015", &["all", "race", "reject", "resolve"]),
            ("es2020", &["allSettled"]),
            ("es2021", &["any"]),
            ("es2024", &["withResolvers"]),
            ("es2025", &["try"]),
        ],
    ),
    (
        "Reflect",
        &[(
            "es2015",
            &[
                "apply",
                "construct",
                "defineProperty",
                "deleteProperty",
                "get",
                "getOwnPropertyDescriptor",
                "getPrototypeOf",
                "has",
                "isExtensible",
                "ownKeys",
                "preventExtensions",
                "set",
                "setPrototypeOf",
            ],
        )],
    ),
    (
        "RegExp",
        &[
            ("es2015", &["flags", "sticky", "unicode"]),
            ("es2018", &["dotAll"]),
            ("es2024", &["unicodeSets"]),
        ],
    ),
    ("RegExpConstructor", &[("es2025", &["escape"])]),
    ("RegExpExecArray", &[("es2018", &["groups"])]),
    ("RegExpMatchArray", &[("es2018", &["groups"])]),
    ("RelativeTimeFormat", &[("es2020", &["format", "formatToParts", "resolvedOptions"])]),
    (
        "Set",
        &[
            ("es2015", &["entries", "keys", "values"]),
            (
                "es2025",
                &[
                    "union",
                    "intersection",
                    "difference",
                    "symmetricDifference",
                    "isSubsetOf",
                    "isSupersetOf",
                    "isDisjointFrom",
                ],
            ),
        ],
    ),
    (
        "SharedArrayBuffer",
        &[("es2017", &["byteLength", "slice"]), ("es2024", &["growable", "maxByteLength", "grow"])],
    ),
    (
        "String",
        &[
            (
                "es2015",
                &[
                    "codePointAt",
                    "includes",
                    "endsWith",
                    "normalize",
                    "repeat",
                    "startsWith",
                    "anchor",
                    "big",
                    "blink",
                    "bold",
                    "fixed",
                    "fontcolor",
                    "fontsize",
                    "italics",
                    "link",
                    "small",
                    "strike",
                    "sub",
                    "sup",
                ],
            ),
            ("es2017", &["padStart", "padEnd"]),
            ("es2019", &["trimStart", "trimEnd", "trimLeft", "trimRight"]),
            ("es2020", &["matchAll"]),
            ("es2021", &["replaceAll"]),
            ("es2022", &["at"]),
            ("es2024", &["isWellFormed", "toWellFormed"]),
        ],
    ),
    ("StringConstructor", &[("es2015", &["fromCodePoint", "raw"])]),
    ("Symbol", &[("es2015", &["for", "keyFor"]), ("es2019", &["description"])]),
    (
        "SymbolConstructor",
        &[("es2020", &["matchAll"]), ("esnext", &["metadata", "dispose", "asyncDispose"])],
    ),
    (
        "Uint16Array",
        &[
            ("es2022", &["at"]),
            (
                "es2023",
                &["findLastIndex", "findLast", "toReversed", "toSorted", "toSpliced", "with"],
            ),
        ],
    ),
    (
        "Uint32Array",
        &[
            ("es2022", &["at"]),
            (
                "es2023",
                &["findLastIndex", "findLast", "toReversed", "toSorted", "toSpliced", "with"],
            ),
        ],
    ),
    (
        "Uint8Array",
        &[
            ("es2022", &["at"]),
            (
                "es2023",
                &["findLastIndex", "findLast", "toReversed", "toSorted", "toSpliced", "with"],
            ),
        ],
    ),
    ("Uint8ArrayConstructor", &[("esnext", &["fromBase64", "fromHex"])]),
    (
        "Uint8ClampedArray",
        &[
            ("es2022", &["at"]),
            (
                "es2023",
                &["findLastIndex", "findLast", "toReversed", "toSorted", "toSpliced", "with"],
            ),
        ],
    ),
    ("WeakMap", &[("es2015", &[]), ("esnext", &["getOrInsert", "getOrInsertComputed"])]),
    ("WeakSet", &[("es2015", &[])]),
];

/// An interface member whose name the binder recorded, or whose computed name
/// is a well-known `Symbol.x` (late-bound to a unique-symbol key). Index,
/// call and construct signatures carry no name.
fn member_name_is_bound(member: tsr_ast::TypeElement<'_>) -> bool {
    let name = match member {
        tsr_ast::TypeElement::PropertySignatureDeclaration(property) => property.name,
        tsr_ast::TypeElement::MethodSignatureDeclaration(method) => method.name,
        tsr_ast::TypeElement::GetAccessorDeclaration(accessor) => accessor.name,
        tsr_ast::TypeElement::SetAccessorDeclaration(accessor) => accessor.name,
        _ => return true,
    };
    match name {
        tsr_ast::PropertyName::ComputedPropertyName(computed) => matches!(
            computed.expression,
            Some(tsr_ast::Expression::PropertyAccessExpression(access))
                if matches!(access.expression,
                    Some(tsr_ast::Expression::Identifier(symbol)) if symbol.text == "Symbol")
        ),
        _ => true,
    }
}

use crate::types::TypeId;
type SymbolIdAlias = tsr_binder::SymbolId;
