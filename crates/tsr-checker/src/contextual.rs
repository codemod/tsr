//! Contextual typing: what an *unannotated* parameter's type comes from when
//! the function it belongs to is written where a function type is expected.
//!
//! Ported from `Checker.getContextuallyTypedParameterType` (`checker.go:29458`),
//! reduced to the three corpus arms that need no type inference: a function
//! expression or arrow passed as a **call argument**, typed from the
//! corresponding parameter of the callee's signature; one that is the
//! initialiser of a variable carrying a **type annotation**, typed from the
//! annotation; and one that is the value of a **member of an object literal**
//! which itself has a contextual type, typed from the matching property of it.
//!
//! ```text
//! declare function each(callback: (item: string) => void): void;
//! each(item => item.length);
//! //   ^^^^ `string`, from `callback`, not the implicit `any`
//! ```
//!
//! # Why this is worth a module of its own, and what it is expected to move
//!
//! Contextual typing did not exist in this port at all, and it is the named
//! owner of the largest *known-wrong* population the checker has: the `any`
//! receiver arm in [`crate::members`] carries a measured **+555 wrong
//! property-access lines**, every one of them an unannotated parameter that
//! answered the implicit `any` here and then answered `any` again on the member.
//! Those lines cost no gradient — the parameter's own line was already wrong —
//! but they cost diagnostic separability, which is what this project's method
//! rests on.
//!
//! So the expected shape of the win is unusual and worth stating before the
//! measurement rather than after: **this turns wrong lines into right ones
//! without moving the gap column.** A flat gradient with an improved right/wrong
//! split is the success case, not the failure case.
//!
//! # Concentration: measured before writing, and it is diffuse
//!
//! Counted over `vendor/typescript-go/testdata/baselines/reference/submodule`
//! at the pinned commit, by the signature a contextually typed function leaves
//! in a `.types` baseline — an assertion whose subject is a function expression
//! with **unannotated** parameters and whose type has **typed** parameters, at
//! least one of them not `any`:
//!
//! - **925** such function expressions, in **349** files. The largest single
//!   file holds 42 (4.5%); the top 25 files hold 36%.
//! - Their assertion-line footprint — lines whose subject roots at one of those
//!   parameters — is **5,230 lines across 325 files**, 0.88% of the corpus's
//!   594,122 assertion lines, top 10 files 30%.
//!
//! That check exists because this project has been burned three times by a large
//! row that turned out to be one file. This one is not: it is genuinely spread,
//! which is what makes it worth porting and also what makes any single test
//! fixture a poor proxy for it.
//!
//! # What is reachable without inference, measured rather than assumed
//!
//! The obvious shape — `arr.map(x => x.foo)` — is **not** reachable. `map` is
//! generic and `every` is an overload set, and this port has no
//! `inferTypeArguments`. Classifying the 925 by what their callee's printed type
//! is:
//!
//! | callee | count | reachable |
//! |---|---|---|
//! | one non-generic signature | 548 (59%) | yes — this module |
//! | generic | 183 (20%) | no: needs inference |
//! | overload set | 102 (11%) | no: needs the full `resolveCall` |
//! | not classified by the counter | 92 (10%) | — |
//!
//! The reachable bucket is itself spread over 206 files, largest 37 (6.7%). It
//! is the first arm this module ports, and the generic and overloaded callees
//! are gaps that stay gaps.
//!
//! The second arm — an arrow initialising an **annotated variable**,
//! `const f: (x: T) => U = x => …` — is smaller but cheaper still: **75
//! functions in 32 files**, largest 8 (10.7%), and it needs no call machinery at
//! all, because `getContextualTypeForVariableLikeDeclaration`
//! (`checker.go:29438`) returns the annotation's type directly.
//!
//! The third arm — a **member of an object literal** that itself has a
//! contextual type — is the largest remaining one: 297 functions in 98 files, of
//! which **66 in 33 files** sit in an object literal whose own context this port
//! can already answer. The per-arm measurement that ranked it above `return`
//! (26), array element (33) and parenthesised expression (101, but ~32
//! reachable in 7 files) is in
//! [`Checker::contextual_type_for_object_literal_element`] and in
//! `docs/architecture/checker-notes-ctx.md`.
//!
//! # A gap here is the implicit `any`, and that is not a violated rule
//!
//! Everywhere else in this checker an unported form answers `errorType`, because
//! a confident wrong answer is worse than an honest "don't know". This module
//! answers `None`, and one level up in
//! `Checker::get_type_for_variable_like_declaration` that becomes `anyType`.
//!
//! That is correct rather than a lapse: for a parameter, `anyType` **is**
//! upstream's answer when no contextual type is available (`checker.go:18264`,
//! reached from a nil `getContextuallyTypedParameterType`). `function f(x) {}`
//! types `x` as `any` in TypeScript too. Returning `errorType` from an
//! unreachable arm here would convert lines that are currently *right* into
//! wrong ones. The rule this module is actually bound by is the stronger one it
//! is derived from: **never invent an answer**. `None` invents nothing — it
//! leaves behaviour exactly as it was before this module existed, so the only
//! lines this module can move are ones it types deliberately.
//!
//! # The recursion upstream guards, and how this avoids needing the guard
//!
//! `getContextualTypeForArgumentAtIndex` (`checker.go:29772`) resolves the
//! call's signature to find the argument's contextual type — while it is being
//! reached *from* checking that same call's argument. Upstream breaks the cycle
//! with signature links: it parks `anySignature`/`resolvingSignature` in
//! `signatureLinks` for the duration (`checker.go:29785`).
//!
//! This port has no such cache, so it cannot use that guard, and it must not
//! call [`crate::calls`]'s `resolve_call_signature`: that function now takes the
//! argument list and hands it to `choose_overload`, which **checks every
//! argument** — including the very arrow whose parameter this is trying to type.
//! Instead this module reads the callee's signatures directly and requires there
//! to be exactly one, non-generic. Selecting that signature needs no argument
//! checked at all, so the cycle is not broken, it is never entered. The cost is
//! the 102 overloaded callees in the table above, and paying it is what makes
//! this arm safe without a links table (`bd` follow-up: signature links).

use tsr_ast::{
    BindingName, CallExpression, Expression, Node, NodeId, ParameterDeclaration,
    PropertyAssignment, PropertyName,
};

use crate::{
    checker::Checker,
    signatures::Signature,
    types::{TypeData, TypeId},
};

impl<'a> Checker<'a, '_> {
    /// The type an unannotated parameter takes from its context, if any.
    ///
    /// Ported from `Checker.getContextuallyTypedParameterType`
    /// (`checker.go:29458`), for the arms [`Checker::get_contextual_type`]
    /// supplies.
    ///
    /// `None` is upstream's nil: no contextual type is available, and the caller
    /// turns that into the implicit `any`. See the module documentation for why
    /// `None` rather than `errorType` is the honest answer *here specifically*.
    ///
    /// Not ported, each answering `None`:
    ///
    /// - **A rest parameter of an IIFE** (`((...n) => n)(1, 2)`), which needs
    ///   `getSpreadArgumentType` (`checker.go:29468`). The positional arm
    ///   below would give it the wrong answer, so it declines. The rest of the
    ///   IIFE road IS ported — see §768 and
    ///   [`Checker::immediately_invoked_call`]; this bullet used to say the
    ///   whole mechanism was unported and was corrected when it landed.
    /// - **Every context beyond the three above**: a `return`, a JSX attribute,
    ///   a binary operand, an array element, a parenthesised expression. Each
    ///   needs its own arm of `getContextualType` (`checker.go:29343`); the
    ///   populations and the concentration that rejected them are in
    ///   [`Checker::get_contextual_type`].
    /// - **A rest parameter or a `this` parameter** on the contextually typed
    ///   function, which shift what "the parameter at this index" means —
    ///   upstream calls `getRestTypeAtPosition` for the first and subtracts the
    ///   second from the index. Both are answerable later; neither is guessed.
    /// - **A generic or overloaded callee**, and a callee that is not a call at
    ///   all. See the module documentation.
    pub(crate) fn get_contextually_typed_parameter_type(
        &mut self,
        parameter: NodeId,
    ) -> Option<TypeId> {
        let function = self.nodes.parent(parameter)?;
        // §175 (`checker-notes-narrow.md`): an unannotated SETTER value
        // parameter takes the ACCESSOR's type, which upstream resolves in a
        // fixed order (`checker.go:18520-18529`): the getter's annotation,
        // the setter's, then the GETTER'S BODY return inference. So
        // `set bar(n)` beside `get bar() { return 0; }` is `number`, not
        // the implicit any (`inferSetterParamType`). This sits on the
        // contextual road because that is the road a parameter's type
        // actually travels.
        if self.nodes.kind(function) == tsr_ast::SyntaxKind::SetAccessor
            && self.paired_get_accessor(function).is_some()
            && let Some(accessor) = self.binder.symbol_of(function)
        {
            // `getTypeOfAccessors` on the SHARED symbol (`checker.go:18515`)
            // — the getter and setter bind to one symbol and its type IS the
            // accessor's type. Read through the symbol rather than the
            // getter's signature because `signature_parts_of` has no
            // GetAccessor arm at all (the audit's fifth missing arm), so the
            // signature road answers None here.
            let accessor_type = self.get_type_of_symbol(accessor);
            if accessor_type != self.intrinsics.error {
                return Some(accessor_type);
            }
        }
        // §768 (`checker.go:29463`-`:29484`): the IIFE arm, which runs BEFORE
        // the contextual-signature road below and is a different road
        // entirely — the type comes from the ARGUMENTS of the call that
        // immediately invokes this function, not from any contextual type the
        // function itself has. `(jake => { })("build")` types `jake` as
        // `string`.
        //
        // Found by §767: after `depend.rs` grew step arms, an un-annotated
        // PARAMETER became the gap board's second-largest root (1,059 lines /
        // 249 cases) with `contextuallyTypedIife` at its head, which is
        // exactly this shape.
        if let Some(iife) = self.immediately_invoked_call(function)
            && let Some(parameters) = self.contextualisable_parameters(function)
            && let Some(index) = parameters.iter().position(|p| p.node_id == Some(parameter))
        {
            let Some(Node::CallExpression(call)) = self.node_map.get(iife) else {
                return None;
            };
            // §771 (`:29468`): the REST arm — `getSpreadArgumentType(args,
            // index, len(args), anyType, …)`. The call site passes `anyType`
            // as the rest type, which collapses most of that function: no
            // const context, no tuple contextual element, and
            // `maybeTypeOfKind(any, Primitive|…)` is false, so every remaining
            // argument contributes its WIDENED type as a REQUIRED tuple
            // element. `((...numbers) => …)(5, 6, 7)` is
            // `[number, number, number]`, which the baseline states outright.
            //
            // This is §769 re-applied. §769 wrote the same arm, measured it at
            // 21:9 adverse and REVERTED it: a tuple inherited no `Array<T>`
            // members then, so giving a parameter its correct tuple type took
            // `noNumbers.some(…)` from RIGHT to GAP. §770 fixed that, and
            // STATUS §5 named this as the re-application.
            //
            // A SPREAD argument (`f(...xs)`) takes the variadic/rest legs at
            // `:29504` and `:29528`; those are not ported and decline here, so
            // the arm answers only for plain argument lists.
            // §795 (`:29504`/`:29528`): the SPREAD-ARGUMENT legs, §771's
            // recorded residue. `(function (a, b, c) {})(...t1)` with
            // `t1: [number, boolean, string]` types `a`, `b`, `c` positionally
            // off the tuple's elements — the baseline records `>a : number`,
            // `>b : boolean`, `>c : string`
            // (`restTuplesFromContextualTypes.types:11-13`), and a rest
            // parameter takes the remaining elements as a tuple
            // (`>x : [number, boolean, string]`, `:21`).
            //
            // Admitted only for a call whose arguments are EXACTLY one spread
            // over a CONCRETE tuple. That is the whole of upstream's
            // decidable slice here: a spread mixed with plain arguments needs
            // the position arithmetic `getSpreadArgumentType` does over
            // several sources, and a spread of an array has no element to
            // land on. Both keep declining, as they did before this arm.
            if let [tsr_ast::Expression::SpreadElement(spread)] = call.arguments
                && let Some(operand) = spread.expression
            {
                let operand_type = self.check_expression(operand);
                let (elements, _) = self.tuple_element_lists.get(&operand_type).cloned()?;
                if parameters[index].dot_dot_dot_token.is_some() {
                    let tail: Vec<TypeId> = elements.into_iter().skip(index).collect();
                    return Some(self.create_tuple_type(tail, false));
                }
                return elements.get(index).copied();
            }
            if parameters[index].dot_dot_dot_token.is_some() {
                if call
                    .arguments
                    .iter()
                    .any(|argument| matches!(argument, tsr_ast::Expression::SpreadElement(_)))
                {
                    return None;
                }
                let mut elements = Vec::new();
                for argument in call.arguments.iter().skip(index) {
                    let argument_type = self.check_expression(*argument);
                    if argument_type == self.intrinsics.error {
                        return None;
                    }
                    elements.push(self.get_widened_literal_type(argument_type));
                }
                return Some(self.create_tuple_type(elements, false));
            }
            if let Some(argument) = call.arguments.get(index) {
                let argument_type = self.check_expression(*argument);
                if argument_type == self.intrinsics.error {
                    return None;
                }
                return Some(self.get_widened_literal_type(argument_type));
            }
            // `:29477`-`:29480`: past the arguments, a parameter WITH an
            // initializer takes its initializer's type (upstream answers nil
            // and lets the ordinary road run); one without is `undefined`.
            if parameters[index].initializer.is_some() {
                return None;
            }
            return Some(self.intrinsics.undefined);
        }
        let parameters = self.contextualisable_parameters(function)?;

        // `slices.Index(fn.Parameters(), parameter)` (`checker.go:29489`). A
        // `this` parameter would make this index disagree with the signature's,
        // which is exactly what upstream's `GetThisParameter` subtraction on the
        // same line repairs; here it is a gap instead, so the two indices are
        // the same index or there is no answer.
        let index = parameters.iter().position(|p| p.node_id == Some(parameter))?;
        if parameters.iter().any(|p| is_this_parameter(p)) {
            return None;
        }
        // §86.1: the function's OWN trailing rest no longer bails the whole
        // list — `(a, b, ...rest)` under `(...args: [number, boolean,
        // ...string[]]) => void` types `a: number` positionally and `rest:
        // string[]` from the tail. A rest anywhere but last still declines.
        let own_rest = parameters.iter().position(|p| p.dot_dot_dot_token.is_some());
        match own_rest {
            Some(position) if position + 1 != parameters.len() => return None,
            _ => {}
        }
        let asking_for_rest = own_rest == Some(index);

        let signature = self.contextual_signature(function)?;
        // §86 (`checker-notes-narrow.md`): a SINGLE REST parameter over
        // tuples expands positionally — `(...args: ['A', number] | ['B',
        // string]) => void` types parameter 0 as `"A" | "B"` and parameter
        // 1 as `number | string` (`getTypeAtPosition`;
        // `restTuplesFromContextualTypes`, `dependentDestructuredVariables`'
        // f50/f51). Every tuple must be long enough; a non-tuple
        // constituent declines.
        if let [rest] = signature.parameters.as_slice()
            && rest.rest
        {
            let constituents: Vec<TypeId> = match &self.store.get(rest.r#type).data {
                TypeData::Union { types, .. } => types.clone(),
                _ => vec![rest.r#type],
            };
            let mut positional = Vec::with_capacity(constituents.len());
            for constituent in constituents {
                if let Some((elements, readonly)) = self.tuple_element_lists.get(&constituent) {
                    if asking_for_rest {
                        // §86.2: the own rest takes the tuple SLICE from its
                        // position — `(a, ...rest)` under `(...args:
                        // [number, string, boolean])` types `rest: [string,
                        // boolean]` (`getRestTypeAtPosition`'s slice).
                        let (elements, readonly) = (elements.clone(), *readonly);
                        if index > elements.len() {
                            return None;
                        }
                        let slice = elements[index..].to_vec();
                        positional.push(self.create_tuple_type(slice, readonly));
                        continue;
                    }
                    positional.push(*elements.get(index)?);
                    continue;
                }
                // §87: a trailing-rest variadic — prefix positions index,
                // tail positions take the rest's element type; resolved
                // lazily from the recorded node.
                let tuple_node = *self.tuple_rest_tails.get(&constituent)?;
                let Some(tsr_ast::Node::TupleTypeNode(tuple)) = self.node_map.get(tuple_node)
                else {
                    return None;
                };
                let [prefix @ .., tsr_ast::TypeNode::RestTypeNode(rest)] = tuple.elements else {
                    return None;
                };
                if asking_for_rest {
                    // The own rest takes the WHOLE tail array when it sits at
                    // or past the prefix boundary; a rest that would swallow
                    // prefix elements needs slice minting — declined.
                    if index < prefix.len() {
                        return None;
                    }
                    let tail_node = rest.r#type?;
                    let resolved = self.get_type_from_type_node(tail_node);
                    if resolved == self.intrinsics.error {
                        return None;
                    }
                    positional.push(resolved);
                    continue;
                }
                let member = if let Some(member) = prefix.get(index) {
                    *member
                } else {
                    let Some(tsr_ast::TypeNode::ArrayTypeNode(array)) = rest.r#type else {
                        return None;
                    };
                    array.element_type?
                };
                let resolved = self.get_type_from_type_node(member);
                if resolved == self.intrinsics.error {
                    return None;
                }
                positional.push(resolved);
            }
            return Some(self.get_union_type(&positional));
        }
        let contextual = signature.parameters.get(index)?;
        // A contextual parameter that is itself optional or rest carries a type
        // whose relationship to the position is not the plain one — upstream
        // reaches those through `getRestTypeAtPosition` and the optionality
        // rules in `crate::optionality`. Neither is asserted from here.
        if contextual.optional || contextual.rest {
            return None;
        }
        Some(contextual.r#type)
    }

    /// The parameter list of `function`, if it is a form that can be
    /// contextually typed.
    ///
    /// Ported from `Checker.isContextSensitiveFunctionOrObjectLiteralMethod`
    /// (`checker.go:29495`), minus the object-literal method — which needs the
    /// object literal's own contextual type, an arm this module does not have.
    ///
    /// The `isContextSensitiveFunctionLikeDeclaration` half of upstream's test
    /// holds by construction rather than by a check: the only caller reaches
    /// here for a parameter that has no type annotation, which is what makes a
    /// function context-sensitive in the first place.
    /// `GetImmediatelyInvokedFunctionExpression` (`ast/utilities.go:1853`).
    /// §768.
    ///
    /// The call that immediately invokes `function`, looking through any
    /// number of parentheses — `((function (x) { }))("!")` counts, which is
    /// what `contextuallyTypedIife`'s "Lots of Irritating Superfluous
    /// Parentheses" block is there to check.
    pub(crate) fn immediately_invoked_call(&self, function: NodeId) -> Option<NodeId> {
        if !matches!(
            self.nodes.kind(function),
            tsr_ast::SyntaxKind::FunctionExpression | tsr_ast::SyntaxKind::ArrowFunction
        ) {
            return None;
        }
        let mut previous = function;
        let mut parent = self.nodes.parent(function)?;
        while self.nodes.kind(parent) == tsr_ast::SyntaxKind::ParenthesizedExpression {
            previous = parent;
            parent = self.nodes.parent(parent)?;
        }
        let Some(Node::CallExpression(call)) = self.node_map.get(parent) else {
            return None;
        };
        (call.expression.and_then(|e| e.node_id()) == Some(previous)).then_some(parent)
    }

    fn contextualisable_parameters(
        &self,
        function: NodeId,
    ) -> Option<&'a [&'a ParameterDeclaration<'a>]> {
        match self.node_map.get(function)? {
            Node::ArrowFunction(node) => Some(node.parameters),
            Node::FunctionExpression(node) => Some(node.parameters),
            _ => None,
        }
    }

    /// The signature `function` is contextually typed by, whichever context
    /// supplies it.
    ///
    /// Ported from `Checker.getContextualSignature` (`checker.go:10264`), which
    /// reaches every context through one `getContextualType` switch — here
    /// [`Checker::get_contextual_type`].
    ///
    /// §886 ports the **union** arm's decidable half. Upstream iterates the
    /// constituents and collects each one's contextual call signature
    /// (`checker.go:10272-10292`); this port keeps the collection and declines
    /// where upstream would *combine*:
    ///
    /// | constituents yielding a signature | upstream | here |
    /// |---|---|---|
    /// | 0 | `nil` | `None` |
    /// | 1 | that signature | that signature |
    /// | 2+ | `compareSignaturesIdentical`, then `createUnionSignature` | `None` |
    ///
    /// The one-signature case is the whole of an optional member: `k?(a: any):
    /// any` gives `((a: any) => any) | undefined`, and `undefined` has no call
    /// signature, so exactly one survives. The previous refusal called this
    /// *"picking one member's signature is a guess"* — it is not a guess when
    /// the other constituents contribute nothing, which is the only case ported.
    ///
    /// Two or more stays refused: telling "identical, so combine" from "different,
    /// so `nil`" needs `compareSignaturesIdentical`, and `createUnionSignature`
    /// needs a signature whose return type is a union of the members'. Neither
    /// exists here, and answering with an arbitrary member would be the guess the
    /// old comment described.
    pub(crate) fn get_contextual_type_of_call(&mut self, call: NodeId) -> Option<TypeId> {
        self.get_contextual_type(call)
    }

    pub(crate) fn contextual_signature(&mut self, function: NodeId) -> Option<Signature> {
        let contextual = self.get_contextual_type(function)?;
        if let TypeData::Union { types, .. } = &self.store.get(contextual).data {
            let constituents = types.clone();
            let mut found: Option<Signature> = None;
            for constituent in constituents {
                let Some(signature) = self.contextual_signature_of_type(constituent) else {
                    continue;
                };
                if found.is_some() {
                    // Two constituents offer one: upstream compares them and may
                    // build a union signature. Declining is the honest answer.
                    return None;
                }
                found = Some(signature);
            }
            return found;
        }
        self.contextual_signature_of_type(contextual)
    }

    /// `getContextualCallSignature` (`checker.go:10305`) for one non-union type:
    /// the single call signature the type offers, by the three reads
    /// [`Checker::contextual_signature`] has always used.
    ///
    /// Upstream additionally filters by arity (`isAritySmaller`) and intersects
    /// what is left; neither is ported, so a type with two or more call
    /// signatures declines here where upstream may still answer.
    fn contextual_signature_of_type(&mut self, contextual: TypeId) -> Option<Signature> {
        // The tsr-0hc type-first read (the freeze-breaking wire).
        if self.is_instantiated_signature_type(contextual) {
            let signatures = self.signature_types.get(&contextual).cloned().unwrap_or_default();
            if let [signature] = signatures.as_slice() {
                return Some(signature.clone());
            }
            return None;
        }
        if let Some(signature) = self.single_call_signature(contextual) {
            return Some(signature);
        }
        // §75 (`checker-notes-narrow.md`): a GENERIC single contextual
        // signature passes through UNINSTANTIATED — `const fn1: <T>(x: T) =>
        // void = t => …` types `t : T`, the signature's OWN type parameter,
        // because the arrow ADOPTS the contextual generics
        // (`contextualOuterTypeParameters`). `single_call_signature`'s
        // generic decline is for CALL positions, where printing a bare `T`
        // would be wrong; here it is exactly upstream's answer.
        let TypeData::Anonymous { symbol, .. } = self.store.get(contextual).data else {
            return None;
        };
        match self.get_signatures_of_symbol(symbol)?.as_slice() {
            [signature] => Some(signature.clone()),
            _ => None,
        }
    }

    /// The type `node` is expected to have, from where it is written.
    ///
    /// Ported from `Checker.getContextualType` (`checker.go:29343`) — the
    /// dispatch every context reaches, a `switch` on the **parent's** kind. Three
    /// of its twenty arms are here.
    ///
    /// # Why this is a dispatch on an expression and not a helper on a function
    ///
    /// The first version of this module had no such function: it asked "is this
    /// *function* a call argument, or the initialiser of an annotated variable?"
    /// and answered a `Signature` directly. That shape cannot express the
    /// object-literal arm, whose subject is the enclosing **object literal** —
    /// an expression that is not a function and has no signature. Upstream's own
    /// structure is the fix and it is a strict generalisation: each of the two
    /// existing arms was already `single_call_signature(<a type>)`, so factoring
    /// the type out changes no answer. The object-literal arm is then a
    /// recursive call rather than a fourth special case, which is exactly how
    /// `checker.go:29925`'s `getApparentTypeOfContextualType(objectLiteral)`
    /// reaches it.
    ///
    /// # The seventeen arms that are not here
    ///
    /// Each was measured over
    /// `vendor/typescript-go/testdata/baselines/reference/submodule` before being
    /// left out; the counts and the concentration are in
    /// `docs/architecture/checker-notes-ctx.md`. The three largest omissions:
    ///
    /// - **`ParenthesizedExpression`** (`checker.go:29392`), a one-line recursion
    ///   — 101 contextually typed functions, but only ~32 of them in a context
    ///   this port can already answer, and those sit in 7 files with
    ///   `parenthesizedContexualTyping{1,2,3}` holding most of them. Rejected on
    ///   concentration, not on cost.
    /// - **`ReturnStatement`** (`checker.go:29621`) — 26 functions in 15 files,
    ///   top 10 files 81%.
    /// - **`ArrayLiteralExpression`** (`checker.go:29380`, via
    ///   `getContextualTypeForElementExpression`, `checker.go:29972`) — 33
    ///   functions in 15 files, top 10 files 85%, and every one of them needs
    ///   tuple types this port does not have.
    ///
    /// A `NewExpression` shares upstream's `CallExpression` arm and would cost
    /// one pattern here, but it is not measured separately, so it is a gap.
    pub(crate) fn get_contextual_type(&mut self, node: NodeId) -> Option<TypeId> {
        let parent = self.nodes.parent(node)?;
        match self.node_map.get(parent)? {
            // `getContextualTypeForInitializerExpression` (`checker.go:29423`) →
            // `getContextualTypeForVariableLikeDeclaration` (`checker.go:29438`),
            // whose first three lines are the whole of this arm: if the
            // declaration has a type node, the contextual type *is* that type.
            //
            // Restricted to a `VariableDeclaration`. The other variable-like
            // carriers of an annotation — a `PropertyDeclaration`, a
            // `PropertySignature`, a parameter with a function-typed annotation
            // and a function initialiser — reach the same upstream function, but
            // each is a separate corpus shape and none is measured, so each is a
            // gap rather than an untested generalisation.
            //
            // No check that `node` is the *initialiser*. A `VariableDeclaration`
            // has three children — name, type annotation, initialiser — and only
            // the initialiser is an expression, so the parent test already
            // establishes it. An earlier draft asserted it anyway; no mutation
            // could make the assertion observable, including the one that looked
            // most dangerous — typing the annotation's own parameter from the
            // signature it belongs to — because such a parameter's parent is the
            // `FunctionType` node and [`Checker::contextualisable_parameters`]
            // turns it away one level up. Removed rather than kept as decoration,
            // which is the call `crate::members` records making for the same
            // reason.
            Node::VariableDeclaration(declaration) => {
                let annotation = declaration.r#type?;
                Some(self.get_type_from_type_node(annotation))
            }
            // §154 (`checker-notes-ctx.md`): the assertion family — `x as T`
            // and `<T>x` answer the asserted type as context, EXCEPT `as
            // const` (isConstContext's business, not a contextual type);
            // `x satisfies T` answers its type node the same way.
            Node::AsExpression(assertion) => {
                let annotation = assertion.r#type?;
                if crate::assertions::is_const_type_reference(annotation) {
                    return None;
                }
                Some(self.get_type_from_type_node(annotation))
            }
            Node::TypeAssertion(assertion) => {
                let annotation = assertion.r#type?;
                if crate::assertions::is_const_type_reference(annotation) {
                    return None;
                }
                Some(self.get_type_from_type_node(annotation))
            }
            Node::SatisfiesExpression(node) => {
                let annotation = node.r#type?;
                Some(self.get_type_from_type_node(annotation))
            }
            // §153 (`checker-notes-ctx.md`): a CLASS property's annotation is
            // the same `getContextualTypeForVariableLikeDeclaration` road —
            // the gap the arm above's comment recorded. Initializer position
            // is established the same way (name and annotation are not
            // expressions); computed names carry no annotation relevant here.
            Node::PropertyDeclaration(declaration) => {
                let annotation = declaration.r#type?;
                Some(self.get_type_from_type_node(annotation))
            }
            Node::CallExpression(call) => self.contextual_type_for_argument(call, node),
            // §155 (`checker-notes-ctx.md`): a NEW argument's context through
            // the §90 arity road — the sole non-generic constructor's written
            // annotation at this position. The recursion the module doc
            // guards is never entered: `sole_constructor_parameters` reads
            // declarations, not signatures.
            Node::NewExpression(new_expression) => {
                if !new_expression.type_arguments.is_empty() {
                    return None;
                }
                let index = new_expression
                    .arguments
                    .iter()
                    .position(|argument| argument.node_id() == Some(node))?;
                let callee = new_expression.expression.and_then(|e| e.node_id())?;
                let arity = self.sole_constructor_parameters(callee)?;
                if !arity.check_argument_types {
                    return None;
                }
                let (annotation, _optional) = arity.annotations.get(index).copied().flatten()?;
                self.type_from_annotation_id(annotation)
            }
            // §152 (`checker-notes-ctx.md`): `getContextualTypeForBinaryOperand`'s
            // equals arm (`checker.go:29809`) — the RIGHT operand of plain `=`
            // answers the LEFT operand's type. The §98 walk built the same
            // rule for object-literal members; this is it at the dispatch
            // site, with the same reentrancy guard and JS-file decline.
            Node::BinaryExpression(binary) => {
                if binary
                    .operator_token
                    .is_none_or(|token| token.kind != tsr_ast::SyntaxKind::EqualsToken)
                    || binary.right.and_then(|e| e.node_id()) != Some(node)
                    || self.in_js_file(node)
                {
                    return None;
                }
                let left = binary.left?;
                if !self.narrow_value_stack.insert(parent) {
                    return None;
                }
                let checked = self.check_expression(left);
                self.narrow_value_stack.remove(&parent);
                (checked != self.intrinsics.error).then_some(checked)
            }
            // SS115: a ternary BRANCH answers the conditional's own context;
            // the CONDITION answers nil
            // (getContextualTypeForConditionalOperand, checker.go:30022).
            Node::ConditionalExpression(conditional) => {
                let is_branch = conditional.when_true.and_then(|e| e.node_id()) == Some(node)
                    || conditional.when_false.and_then(|e| e.node_id()) == Some(node);
                if !is_branch {
                    return None;
                }
                self.get_contextual_type(parent)
            }
            Node::PropertyAssignment(element) => {
                self.contextual_type_for_object_literal_element(parent, element)
            }
            // §68 (`checker-notes-narrow.md`): `getContextualTypeForReturnExpression`
            // (`checker.go:29621`), the WRITTEN-annotation half — a returned
            // expression's contextual type is the enclosing function's
            // declared return type. Rejected once at 26 functions;
            // `generatedContextualTyping` alone holds 62 aligned lines now.
            // §68.1: `ParenthesizedExpression` (`checker.go:29392`) — the
            // one-line recursion, un-rejected with §68's own argument.
            Node::ParenthesizedExpression(_) => self.get_contextual_type(parent),
            // §68.2: an ARRAY LITERAL element's contextual type is the
            // array's contextual ELEMENT type
            // (`getContextualTypeForElementExpression`, `checker.go:29972`) —
            // the Array-reference and tuple halves this port can read.
            Node::ArrayLiteralExpression(literal) => {
                let contextual = self.get_contextual_type(parent)?;
                if let Some((target, arguments)) =
                    self.type_reference_targets.get(&contextual).cloned()
                    && arguments.len() == 1
                    && ["Array", "ReadonlyArray"].iter().any(|name| {
                        self.global_type_symbol(name).map(|s| self.binder.merged_symbol(s))
                            == Some(self.binder.merged_symbol(target))
                    })
                {
                    return Some(arguments[0]);
                }
                if let Some((elements, _)) = self.tuple_element_lists.get(&contextual) {
                    let index = literal
                        .elements
                        .iter()
                        .position(|element| element.node_id() == Some(node))?;
                    return elements.get(index).copied();
                }
                None
            }
            // §68.3: a CONCISE arrow body's contextual type is the arrow's
            // own contextual signature's return
            // (`getContextualReturnType`, `checker.go:29648` region).
            Node::ArrowFunction(arrow)
                if arrow.body.and_then(|b| tsr_ast::Node::from(b).node_id()) == Some(node)
                    && self.nodes.kind(node) != tsr_ast::SyntaxKind::Block =>
            {
                let signature = self.contextual_signature(parent)?;
                (signature.r#type != self.intrinsics.error).then_some(signature.r#type)
            }
            Node::ReturnStatement(_) => {
                let mut function = self.nodes.parent(parent)?;
                loop {
                    match self.nodes.kind(function) {
                        // §171 (`checker-notes-narrow.md`): the widened
                        // census applied to this lane — upstream ascends to
                        // the containing function through `IsFunctionLike`,
                        // which includes ACCESSORS and the CONSTRUCTOR, and
                        // `expressions.rs`'s control_flow_container lists
                        // all seven kinds while this enumeration stopped at
                        // four. A `return` inside a get accessor walked PAST
                        // it to an outer function and took that function's
                        // contextual return type.
                        tsr_ast::SyntaxKind::FunctionDeclaration
                        | tsr_ast::SyntaxKind::FunctionExpression
                        | tsr_ast::SyntaxKind::ArrowFunction
                        | tsr_ast::SyntaxKind::MethodDeclaration
                        | tsr_ast::SyntaxKind::GetAccessor
                        | tsr_ast::SyntaxKind::SetAccessor
                        | tsr_ast::SyntaxKind::Constructor => break,
                        tsr_ast::SyntaxKind::SourceFile => return None,
                        _ => function = self.nodes.parent(function)?,
                    }
                }
                let annotation = match self.node_map.get(function)? {
                    Node::FunctionDeclaration(f) => f.r#type,
                    Node::FunctionExpression(f) => f.r#type,
                    Node::ArrowFunction(f) => f.r#type,
                    Node::MethodDeclaration(f) => f.r#type,
                    _ => None,
                }?;
                Some(self.get_type_from_type_node(annotation))
            }
            _ => None,
        }
    }

    /// The type an object literal's property assignment is expected to have.
    ///
    /// ```text
    /// interface Handlers { onTick: (tick: string) => void }
    /// const handlers: Handlers = { onTick: value => value };
    /// //                                   ^^^^^ `string`, via `Handlers.onTick`
    /// ```
    ///
    /// Ported from `Checker.getContextualTypeForObjectLiteralElement`
    /// (`checker.go:29920`), which is two steps: the object literal's own
    /// contextual type, then the matching property of it.
    ///
    /// # Measured, and why this arm and not another
    ///
    /// 297 contextually typed function expressions in 98 files sit in an object
    /// literal member — the largest arm after the call argument, and the second
    /// largest of the twenty. But the population that matters is the one whose
    /// **prerequisite is met**, because this arm answers nothing unless the
    /// enclosing object literal already has a contextual type. Splitting the 297
    /// by what supplies the object literal's own context:
    ///
    /// | the object literal is… | count | reachable today |
    /// |---|---|---|
    /// | an argument to a generic callee | 82 | no — needs inference |
    /// | initialiser of a simply annotated declaration | 47 | **yes** |
    /// | initialiser of an unannotated declaration | 28 | no |
    /// | a member of another object literal | 23 | no — needs this arm to nest through an index signature |
    /// | an array/tuple element | 22 | no — needs tuple types |
    /// | initialiser of a **union**-annotated declaration | 20 | no — `getApparentTypeOfContextualType` |
    /// | an argument to a one-signature callee | 19 | **yes** |
    /// | parenthesised, in a block, a `return`, other | 56 | no |
    ///
    /// **66 in 33 files**, footprint 259 assertion lines, largest file 21%, top
    /// 10 files 61%. That is the same order as the annotated-variable arm's 75,
    /// and it costs less, because everything below already exists.
    ///
    /// # Not ported
    ///
    /// - **A computed property name.** Upstream checks the name expression and
    ///   uses it if it is usable as a property name (`checker.go:29934`); here a
    ///   `ComputedPropertyName` answers `None`. So do a numeric, bigint and
    ///   template-literal name: [`Checker::get_property_of_type`] is keyed by the
    ///   source text and the literal-to-name mapping is upstream's
    ///   `getLiteralTypeFromPropertyName`, which is not ported.
    /// - **The index-signature fallback** (`checker.go:29946`), which is what
    ///   upstream answers when no property matches. Without it a member whose
    ///   name is absent from the contextual type is a gap, and gapping it is why
    ///   the 23 nested-object-literal cases above are counted as unreachable
    ///   rather than as an untested claim.
    /// - **`ShorthandPropertyAssignment`**, which shares upstream's arm
    ///   (`checker.go:29376`) but cannot hold a function expression, so it could
    ///   never reach [`Checker::get_contextually_typed_parameter_type`].
    /// - **A `SpreadAssignment` or an object-literal method**, each a separate
    ///   upstream branch (`checker.go:29378`, `checker.go:29964`).
    fn contextual_type_for_object_literal_element(
        &mut self,
        element: NodeId,
        assignment: &'a PropertyAssignment<'a>,
    ) -> Option<TypeId> {
        // `if t := element.Type(); t != nil && !ast.IsObjectLiteralMethod(element)`
        // (`checker.go:29921`). A `PropertyAssignment` cannot be an object
        // literal method, so the second half of upstream's test is the pattern
        // match above rather than a check here.
        if let Some(annotation) = assignment.r#type {
            return Some(self.get_type_from_type_node(annotation));
        }
        // `c.hasBindableName(element)` (`checker.go:29927`) reduced to the names
        // `get_property_of_type` can be keyed by. See "Not ported" above.
        let name = match assignment.name {
            PropertyName::Identifier(name) => name.text,
            PropertyName::StringLiteral(name) => name.text,
            _ => return None,
        };
        // `objectLiteral := element.Parent` (`checker.go:29924`). No check that
        // the parent *is* an `ObjectLiteralExpression`: a `PropertyAssignment`
        // has no other possible parent in this AST, so a guard here could not be
        // reddened by any mutation and would read as evidence for a decision that
        // was never made. Same call, same reason, as the initialiser guard above.
        let object_literal = self.nodes.parent(element)?;
        let contextual = self.get_contextual_type(object_literal)?;
        // `getTypeOfPropertyOfContextualTypeEx` (`checker.go:29932`). Upstream
        // maps over a union here; this port does not, so a union-typed context
        // finds nothing and gaps — which is the 20 rows in the table above.
        let property = self.get_property_of_type(contextual, name)?;
        let property_type = self.get_type_of_symbol(property);
        // SS141: a reference context's member instantiates through the
        // reference (Computed<T>'s read serves () => T_call, not the
        // target's own parameter).
        let property_type = {
            let image = self.instantiate_for_reference(contextual, property_type);
            if image == self.intrinsics.error { property_type } else { image }
        };
        // SS135: a member of a literal re-checking under a serve memo reads
        // its type through the pass-1 substitution - the object parameter
        // itself is symbol-backed (uninstantiable structurally), so the
        // instantiation happens here, at the property read.
        if let Some((map, type_parameters, names)) =
            self.intra_expression_member_maps.get(&object_literal).cloned()
        {
            let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
            let image = self.instantiate_type(property_type, &map, &type_parameters, &name_refs);
            if image != self.intrinsics.error {
                return Some(image);
            }
        }
        Some(property_type)
    }

    /// The type an expression is expected to have when it sits directly in the
    /// argument list of a call.
    ///
    /// Ported from `Checker.getContextualTypeForArgumentAtIndex`
    /// (`checker.go:29772`), collapsed to the one path that needs no argument
    /// checked — see the module documentation on the recursion this avoids.
    pub(crate) fn contextual_type_for_argument(
        &mut self,
        call: &'a CallExpression<'a>,
        argument: NodeId,
    ) -> Option<TypeId> {
        // `getEffectiveCallArguments` (`checker.go:29772`'s `argIndex` domain).
        // A spread anywhere before this argument makes the index meaningless, so
        // it is a gap rather than an off-by-one waiting to happen.
        if call.arguments.iter().any(|a| matches!(a, Expression::SpreadElement(_))) {
            return None;
        }
        // `None` when the expression is the call's *callee* rather than one of
        // its arguments, which is the immediately invoked function expression
        // upstream handles at `checker.go:29463` from the argument expressions.
        let index = call.arguments.iter().position(|a| a.node_id() == Some(argument))?;

        // Reunion memo consult (checker-notes-callres2.md): a pass-1
        // instantiated candidate for THIS call outranks every stateless
        // road - it is what upstream's resolved-signature threading gives
        // the second pass.
        if let Some(call_id) = call.node_id
            && let Some(memo) = self.call_inference_signatures.get(&call_id)
        {
            let parameter = memo.parameters.get(index)?;
            if parameter.rest || parameter.optional {
                return None;
            }
            return Some(parameter.r#type);
        }
        let callee = call.expression?;
        // §469 — the signature-links table's read, `checker.go:29785`: while
        // this call's signature is already resolving, don't resolve again —
        // upstream answers `resolvingSignature`, whose `getTypeAtPosition` is
        // `anyType` at every index (`relater.go:1757`, parameterless and
        // restless, so `tryGetTypeAtPosition` is nil and the wrapper fills
        // `any`). This is the cycle-breaker §445's refusal named: everything
        // below resolves the callee, and the callee's own type computation
        // can reach this very call's contextual road again. The park spans
        // exactly the resolving section — the memo consult above is upstream's
        // completed-`resolvedSignature` read and must stay reachable on
        // re-entry paths that arrive after pass-1 populated it.
        let Some(call_id) = call.node_id else {
            return self.contextual_type_for_argument_resolving(call, callee, index);
        };
        if !self.resolving_signature_calls.insert(call_id) {
            return Some(self.intrinsics.any);
        }
        let contextual = self.contextual_type_for_argument_resolving(call, callee, index);
        self.resolving_signature_calls.remove(&call_id);
        contextual
    }

    /// The resolving section of [`Checker::contextual_type_for_argument`] —
    /// everything that computes the callee's type, split out so the §469
    /// sentinel can park around it with one insert/remove pair rather than
    /// one per early return.
    fn contextual_type_for_argument_resolving(
        &mut self,
        call: &'a CallExpression<'a>,
        callee: Expression<'a>,
        index: usize,
    ) -> Option<TypeId> {
        let callee_type = self.check_expression(callee);
        if let Some(parameter) = self
            .single_call_signature(callee_type)
            .and_then(|s| s.parameters.into_iter().nth(index))
        {
            return Some(parameter.r#type);
        }
        // Iteration 4 arm (a) (checker-notes-callres2.md, the priority
        // read): a SINGLE GENERIC candidate's parameter type flows AS-IS -
        // upstream checks every argument with its parameter type as
        // context, generic or not (checkExpressionWithContextualType,
        // checker.go:9485), and the arrow ADOPTS the type parameters (the
        // SS75 semantics, extended from the annotation road to here). The
        // SS70 mention guard stays on the multi-candidate agreement path
        // below, where position-stability is a real question.
        if let TypeData::Anonymous { symbol, .. } = self.store.get(callee_type).data
            && let Some(signatures) = self.get_signatures_of_symbol(symbol)
            && let [single] = signatures.as_slice()
            && !single.type_parameters.is_empty()
        {
            let single = single.clone();
            let parameter_type = {
                let parameter = single.parameters.get(index)?;
                if parameter.rest || parameter.optional {
                    return None;
                }
                parameter.r#type
            };
            // The third rung (the ladder test's final flip): upstream's
            // FIXING mapper — a context consumed with no inference
            // candidates fixes its type parameters to `unknown`
            // (`getInferredType`'s final leg, `inference.go:1317`). This
            // road fires only when NO memo exists, i.e. no pass-1
            // candidates were collected for this call, so the fill is
            // total: `someGenerics6(n => n, ...)` wants
            // `(n: unknown) => unknown`, not the adopted `(n: A) => A`.
            // §134 returnMapper guard, per-parameter: a call in CONTEXTUAL
            // position has a return-position inference source (upstream's
            // returnMapper), which sources exactly the parameters that
            // APPEAR in the return type — those stay adopted
            // (genericContextualTypes1's compose/pipe shapes); the rest
            // fix to `unknown` even there
            // (contextualTypingTwoInstancesOfSameTypeParameter).
            let contextual_call = call
                .node_id
                .is_some_and(|call_id| self.get_contextual_type_of_call(call_id).is_some());
            let Some(type_parameter_ids) = self.type_parameter_types(&single) else {
                return Some(parameter_type);
            };
            let names: Vec<&str> = single.type_parameters.iter().map(|p| p.name.as_str()).collect();
            let unknown = self.intrinsics.unknown;
            let returned = single.r#type;
            // §834: a WRITTEN type argument is not a fill. The `unknown` below is
            // upstream's FIXING mapper, for a context with no inference
            // candidates — but `someGenerics6<number>(n => n)` has `A` decided by
            // the programmer, and upstream instantiates the signature from the
            // written arguments before any argument is checked. So each position
            // takes its written argument where one exists and `unknown` only
            // where none does.
            let written: Vec<TypeId> = call
                .type_arguments
                .iter()
                .map(|argument| self.get_type_from_type_node(*argument))
                .collect();
            let error = self.intrinsics.error;
            let map: Vec<(TypeId, TypeId)> = type_parameter_ids
                .iter()
                .enumerate()
                .filter(|&(position, &t)| {
                    // A written argument overrides the return-mapper guard: that
                    // guard exists to keep an INFERRED parameter adopted, and
                    // there is nothing to infer at a position the source fixed.
                    written.get(position).is_some_and(|&a| a != error)
                        || !(contextual_call
                            && self.mentions_type_parameter(returned, &[t], &[names[position]]))
                })
                .map(|(position, &t)| match written.get(position) {
                    Some(&argument) if argument != error => (t, argument),
                    _ => (t, unknown),
                })
                .collect();
            if map.is_empty() {
                return Some(parameter_type);
            }
            let image = self.instantiate_type(parameter_type, &map, &type_parameter_ids, &names);
            if image != self.intrinsics.error {
                return Some(image);
            }
            return Some(parameter_type);
        }
        // §70 (`checker-notes-narrow.md`): OVERLOADED/GENERIC callees whose
        // every candidate AGREES on the parameter's type at this index — the
        // agreement is what upstream's per-candidate contextual pass
        // converges to when the position's type mentions no type parameter
        // (`parenthesizedContexualTyping2`'s FuncType callbacks, 73 lines).
        let TypeData::Anonymous { symbol, .. } = self.store.get(callee_type).data else {
            return None;
        };
        let candidates = self.get_signatures_of_symbol(symbol)?;
        // SS114 family 1: when the candidates DISAGREE at this index (or a
        // candidate lacks the position), upstream would contextually type
        // through the RESOLVED signature - the first discriminator of which
        // is arity. The single candidate whose parameter count equals the
        // call's argument count decides; ties or no match keep the SS70
        // agreement requirement. Found by round 4's PARAM-SYM instrumentation:
        // thirteen None positions on the head case, all at mixed-arity
        // overload pairs.
        let by_arity: Vec<&Signature> =
            candidates.iter().filter(|c| c.parameters.len() == call.arguments.len()).collect();
        if let [chosen] = by_arity.as_slice() {
            let parameter = chosen.parameters.get(index)?;
            if parameter.rest || parameter.optional {
                return None;
            }
            if self.mentions_any_type_parameter(parameter.r#type, 2) {
                return None;
            }
            return Some(parameter.r#type);
        }
        let mut agreed: Option<TypeId> = None;
        for candidate in &candidates {
            let parameter = candidate.parameters.get(index)?;
            if parameter.rest || parameter.optional {
                return None;
            }
            if self.mentions_any_type_parameter(parameter.r#type, 2) {
                return None;
            }
            match agreed {
                None => agreed = Some(parameter.r#type),
                Some(t) if t == parameter.r#type => {}
                Some(_) => return None,
            }
        }
        agreed
    }

    /// The one call signature of `id`, or `None` if it has any other number of
    /// them or is generic.
    ///
    /// Ported from `Checker.getSignaturesOfType` (`checker.go:18959` via
    /// [`Checker::get_signatures_of_symbol`]) with the two reductions the module
    /// documentation justifies: an overload set is `None` because choosing needs
    /// arguments checked, and a generic signature is `None` because its
    /// parameter types would print as their own type parameters — `T`, not the
    /// type the caller supplied.
    /// §70's mention walk: whether `id` is, or transitively contains (to
    /// `depth` levels through signature parameters/returns and reference
    /// arguments), a type-parameter type. Conservative: ANY type-parameter
    /// mention declines, not just the candidate's own — a callback's inner
    /// generic re-binds its names and cannot leak the outer parameter.
    pub(crate) fn mentions_any_type_parameter(&mut self, id: TypeId, depth: u8) -> bool {
        if self.type_parameter_symbols.contains_key(&id) {
            return true;
        }
        if depth == 0 {
            return false;
        }
        if let Some((_, arguments)) = self.type_reference_targets.get(&id).cloned()
            && arguments.iter().any(|&a| self.mentions_any_type_parameter(a, depth - 1))
        {
            return true;
        }
        if let Some(signatures) = self.signatures_of_type(id) {
            let signatures = signatures.clone();
            for signature in &signatures {
                if signature.type_parameters.is_empty()
                    && (signature
                        .parameters
                        .iter()
                        .any(|p| self.mentions_any_type_parameter(p.r#type, depth - 1))
                        || self.mentions_any_type_parameter(signature.r#type, depth - 1))
                {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn single_call_signature(&mut self, id: TypeId) -> Option<Signature> {
        let TypeData::Anonymous { symbol, .. } = self.store.get(id).data else {
            return None;
        };
        match self.get_signatures_of_symbol(symbol)?.as_slice() {
            [signature] if signature.type_parameters.is_empty() => Some(signature.clone()),
            _ => None,
        }
    }
}

/// Whether `parameter` is the `this` parameter.
///
/// Ported from `ast.GetThisParameter` (`utilities.go`), which upstream reaches
/// on `checker.go:29489` to correct the parameter index. Here it only decides a
/// gap, so the identifier text is the whole test — a `this` parameter cannot be
/// a binding pattern.
fn is_this_parameter(parameter: &ParameterDeclaration<'_>) -> bool {
    matches!(parameter.name, Some(BindingName::Identifier(name)) if name.text == "this")
}
