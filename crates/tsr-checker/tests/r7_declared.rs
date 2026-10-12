//! r7-declared's ports (`docs/parity/notes/r7-declared.md`). Every
//! expectation is native tsgo's print at the pinned submodule.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the initializer of the last `const` in `source`,
/// descending into function bodies so a fixture can introduce type
/// parameters.
fn type_of_last_initializer(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let options = tsr_core::CompilerOptions {
        strict_null_checks: tsr_core::Tristate::True,
        ..Default::default()
    };
    checker.apply_compiler_options(&options);
    let last = last_initializer(parsed.source_file.statements)
        .expect("the fixture must contain a const with an initializer");
    let id = checker.check_expression(last);
    checker.type_to_string(id)
}

fn last_initializer<'a>(statements: &[Statement<'a>]) -> Option<tsr_ast::Expression<'a>> {
    let mut found = None;
    for statement in statements {
        match statement {
            Statement::VariableStatement(node) => {
                if let Some(list) = node.declaration_list {
                    for declaration in list.declarations {
                        found = declaration.initializer.or(found);
                    }
                }
            }
            Statement::FunctionDeclaration(node) => {
                if let Some(tsr_ast::FunctionBody::Block(block)) = node.body {
                    found = last_initializer(block.statements).or(found);
                }
            }
            _ => {}
        }
    }
    found
}

/// §2: getConditionalType's checkTuples (checker.go:24310): with a simple
/// tuple check and extends of the same length, a tuple whose element is
/// generic defers (isDeferredType, checker.go:24475), so the member reads
/// as the alias reference (tsgo: `q : Both<T, string>`).
#[test]
fn a_simple_tuple_check_with_a_generic_element_defers() {
    let source = "type Both<A, B> = [A, B] extends [string, string] ? 1 : 0;\n\
         type Box<T> = { v: Both<T, string> };\n\
         function f<T>(b: Box<T>) { const q = b.v; }\n";
    assert_eq!(type_of_last_initializer(source), "Both<T, string>");
}

/// §2: a generic mapped check type is generic (getGenericObjectFlags,
/// checker.go:24895), so the conditional defers with its alias (tsgo:
/// `q : IsP<T>`).
#[test]
fn a_generic_mapped_check_defers() {
    let source = "type IsP<T> = Partial<T> extends { a: 1 } ? 1 : 0;\n\
         type Box<T> = { v: IsP<T> };\n\
         function f<T>(b: Box<T>) { const q = b.v; }\n";
    assert_eq!(type_of_last_initializer(source), "IsP<T>");
}

/// §3: a check type that mentions only the type parameter its own signature
/// binds is not generic (getGenericObjectFlags), so getConditionalType relates
/// its permissive and restrictive instantiations and takes the false branch
/// (tsgo: `r : { isAny: <T>(obj: any) => obj is T; }`).
#[test]
fn a_signature_bound_parameter_does_not_defer_the_check() {
    let source = "type Ex<T, U> = T extends U ? never : T;\n\
         type Outer<P> = { v: Ex<P, null> };\n\
         declare const o: Outer<{ isAny: <T>(obj: any) => obj is T }>;\n\
         const r = o.v;\n";
    assert_eq!(type_of_last_initializer(source), "{ isAny: <T>(obj: any) => obj is T; }");
}

/// §3: a non-generic check that mentions a type parameter is decided when
/// even its permissive instantiation is unrelated (checker.go:24377): `{ a:
/// T }` against `{ b: string }` is the false branch whatever `T` is (tsgo:
/// `q : 0`).
#[test]
fn a_definitely_false_check_mentioning_a_parameter_takes_the_false_branch() {
    let source = "type Has<T> = { a: T } extends { b: string } ? 1 : 0;\n\
         type Box<T> = { v: Has<T> };\n\
         function f<T>(b: Box<T>) { const q = b.v; }\n";
    assert_eq!(type_of_last_initializer(source), "0");
}

/// §5: a mapped template whose conditional has a parenthesized nested
/// conditional branch is a deferred conditional native prints from its
/// parts; the member reads the instantiated template (tsgo: `r : 0`). The
/// mint had no written text, so the template was `error`.
#[test]
fn a_parenthesized_conditional_branch_in_a_mapped_template_instantiates() {
    let source = "type M<T> = { [K in keyof T]: T[K] extends number ? (T[K] extends string ? 2 : 1) : 0 };\n\
         declare const m: M<{ a: boolean }>;\n\
         const r = m.a;\n";
    assert_eq!(type_of_last_initializer(source), "0");
}

/// §7: a type-literal alias mentioned inside its own body through a union
/// alias (`N = F | B` inside `F`) is `F` itself in `N`'s constituents
/// (getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode creates it once,
/// members lazy), so relating `N` reads `F`'s members (tsgo: `r : 1`). The
/// placeholder had no members table and the relation was undecidable.
#[test]
fn a_self_mentioned_type_literal_alias_has_its_members_in_the_union() {
    let source = "type F = { kind: 'foo'; children: N };\n\
         type B = { kind: 'bar' };\n\
         type N = F | B;\n\
         type R = N extends { kind: 'foo' | 'bar' } ? 1 : 0;\n\
         declare const x: R;\n\
         const r = x;\n";
    assert_eq!(type_of_last_initializer(source), "1");
}

/// §8: a union of an alias-named union and an object keeps the alias in its
/// origin (getUnionTypeWorker, checker.go:25705) instead of failing (tsgo:
/// `z : C3 | CC`).
#[test]
fn an_alias_named_union_beside_an_object_keeps_its_origin() {
    let source = "interface C1 { a: 1 } interface C2 { b: 2 } interface C3 { c: 3 }\n\
         type CC = C1 | C2;\n\
         declare const y: CC | C3;\n\
         const z = y;\n";
    assert_eq!(type_of_last_initializer(source), "C3 | CC");
}

/// §9: an alias whose own body is an indexed access resolves to the
/// property's type without the alias (getIndexedAccessTypeOrUndefined's
/// resolved arm), so `BivariantHack<Animal, Animal>` is the method type
/// instantiated (tsgo: `x : (x: Animal) => Animal`).
#[test]
fn an_own_indexed_alias_body_instantiates_without_the_alias() {
    let source = "interface Animal { a: string }\n\
         type BivariantHack<I, O> = { foo(x: I): O }[\"foo\"];\n\
         declare let f1: BivariantHack<Animal, Animal>;\n\
         const x = f1;\n";
    assert_eq!(type_of_last_initializer(source), "(x: Animal) => Animal");
}

/// §9: a body that is a reference to such an alias hands the outer alias to
/// the instantiated object type (instantiateTypeWithAlias with the new
/// alias), so it keeps the outer name (tsgo: `h : Handler<number>`).
#[test]
fn a_reference_body_to_an_indexed_alias_keeps_the_outer_alias() {
    let source = "type Ev<E> = { hack(event: E): void }[\"hack\"];\n\
         type Handler<T> = Ev<T[]>;\n\
         declare let h1: Handler<number>;\n\
         const h = h1;\n";
    assert_eq!(type_of_last_initializer(source), "Handler<number>");
}

/// §10: a generic alias whose declared type is a union instantiates to the
/// union of its instantiated constituents, carrying the alias
/// (instantiateTypeWithAlias over a union), so an arrow function takes its
/// contextual signature from the function constituent (tsgo: `w => w :
/// (w: number) => number`).
#[test]
fn a_union_alias_instance_is_a_union_for_contextual_typing() {
    let source = "type R<T> = string | ((i: T) => any);\n\
         const r: R<number> = w => w;\n";
    assert_eq!(type_of_last_initializer(source), "(w: number) => number");
}

/// §10: a non-generic alias whose body is a reference to a union-bodied
/// alias declares that union under its own name (getUnionType with the new
/// alias; tsgo: `xx : X`).
#[test]
fn a_union_alias_reference_body_takes_the_declaring_alias() {
    let source = "type R<T> = string | ((i: T) => any);\n\
         type X = R<number>;\n\
         declare const x: X;\n\
         const xx = x;\n";
    assert_eq!(type_of_last_initializer(source), "X");
}

/// §11: an alias whose declared type is a union by its type, not its syntax
/// (`keyof (T & B)` is `"b" | keyof T`), hands the declaring alias to the
/// instantiated union (instantiateTypeWorker's union arm; tsgo: `t5 : T05`).
#[test]
fn a_declared_union_type_takes_the_declaring_alias_whatever_its_syntax() {
    let source = "type A = { a: string };\n\
         type B = { b: string };\n\
         type T02<T> = keyof (T & B);\n\
         type T05 = T02<A>;\n\
         declare const t: T05;\n\
         const t5 = t;\n";
    assert_eq!(type_of_last_initializer(source), "T05");
}

/// §13: a deferred conditional with a function-type extends operand is
/// printed from its parts, so the declaring signature resolves (tsgo: `w :
/// <S>(x: (S extends () => any ? never : S) | (() => S)) => S`); it was
/// `error`.
#[test]
fn a_deferred_conditional_with_a_function_operand_keeps_its_signature() {
    let source = "declare function f3<S>(x: (S extends () => any ? never : S) | (() => S)): S;\n\
         const w = f3;\n";
    assert_eq!(
        type_of_last_initializer(source),
        "<S>(x: (S extends () => any ? never : S) | (() => S)) => S"
    );
}

/// §15: a rest tuple inherits `Array<E>`'s members with `E` the union of its
/// element type arguments (getTupleBaseType), so `slice` resolves (tsgo:
/// `t : (string | number)[]`); it was `error`.
#[test]
fn a_rest_tuple_reads_its_array_base_members() {
    let source = "interface Array<T> { slice(start?: number): T[]; length: number }\n\
         declare const r: [string, ...number[]];\n\
         const t = r.slice(1);\n";
    assert_eq!(type_of_last_initializer(source), "(string | number)[]");
}

/// §20: a homomorphic mapping over a class enumerates no key for a private or
/// protected member (getLiteralTypeFromProperty's non-public arm; tsgo:
/// `Property 'a' does not exist on type 'Req<PrivateState>'`), so its keys
/// are the public ones only.
#[test]
fn a_mapped_type_over_a_class_drops_non_public_members() {
    let source = "class PrivateState { private a = 1; protected c = 3; b = 2; }\n\
         type Req<T> = { [K in keyof T]-?: T[K] };\n\
         declare const r: Req<PrivateState>;\n\
         declare function keys<T>(x: T): keyof T;\n\
         const k = keys(r);\n";
    assert_eq!(type_of_last_initializer(source), "\"b\"");
}

/// §22: a generic alias whose body is an interface reference instantiates to
/// that reference (a deferred type reference carrying the alias), so its
/// call signature types an arrow function contextually (tsgo: `y : { v: number; }`).
#[test]
fn an_interface_reference_alias_instance_keeps_its_call_signature() {
    let source = "interface EH<E> { (event: E): void; }\n\
         type MH<T> = EH<{ v: T }>;\n\
         const b: MH<number> = (y) => {};\n";
    assert_eq!(type_of_last_initializer(source), "(y: { v: number; }) => void");
}

/// §26: getLiteralTypeFromProperties lists a string index's keys as the one
/// stringOrNumberType entry, and getUnionTypeEx answers a one-entry list
/// with that entry, so no `keyof T1` origin is attached (tsgo: `string |
/// number`).
#[test]
fn keyof_a_string_index_alias_is_string_or_number() {
    let source = "type T1 = { [key: string]: { a: string } };\n\
         type K<T> = keyof T;\n\
         declare const k: K<T1>;\n\
         const x = k;\n";
    assert_eq!(type_of_last_initializer(source), "string | number");
}

/// §27: a generic alias over a homomorphic mapped alias keeps the inner alias
/// (instantiateMappedType's instantiateConstituent passes no alias; tsgo:
/// `Partial<{ a: number; }>`).
#[test]
fn a_homomorphic_mapped_reference_alias_keeps_the_inner_alias() {
    let source = "type P<T> = { [K in keyof T]?: T[K] };\n\
         type Q<T> = P<T>;\n\
         declare const q: Q<{ a: number }>;\n\
         const x = q;\n";
    assert_eq!(type_of_last_initializer(source), "P<{ a: number; }>");
}

/// §28: a conditional whose check mentions a type parameter but is not
/// generic (a function type) infers and picks its branch, which carries no
/// alias (tsgo: `[head: V, b: string]`).
#[test]
fn a_definite_generic_infer_conditional_answers_its_branch() {
    let source = "type P<V> = ((head: V, b: string) => void) extends (...args: infer R) => void ? R : any;\n\
         function f<V>() {\n\
             declare const p: P<V>;\n\
             const x = p;\n\
         }\n";
    assert_eq!(type_of_last_initializer(source), "[head: V, b: string]");
}

/// §29: a non-generic alias over a generic alias whose body is an interface
/// reference names the instance (tsgo: `R1`).
#[test]
fn a_new_alias_over_an_interface_reference_alias_names_the_instance() {
    let source = "interface S<P> { config: P; }\n\
         type A<T> = S<{ t: T }>;\n\
         type R1 = A<number>;\n\
         declare const r: R1;\n\
         const x = r;\n";
    assert_eq!(type_of_last_initializer(source), "R1");
}

/// §31: an optional mapped property includes undefined in its type
/// (getTypeOfMappedSymbol; tsgo: `{ a?: 1 | undefined; b?: 2 | undefined; }`).
#[test]
fn an_optional_mapped_property_includes_undefined() {
    let source = "type Obj = { a: 1; b: 2 };\n\
         declare const m: { [K in keyof Obj]?: Obj[K] };\n\
         const x = m;\n";
    assert_eq!(type_of_last_initializer(source), "{ a?: 1 | undefined; b?: 2 | undefined; }");
}

/// §32: a homomorphic mapping over a reference to a mapped alias
/// enumerates that instance's keys (tsgo: `{ "12": number; 42: string; }`).
#[test]
fn a_homomorphic_mapping_over_a_mapped_alias_reference_has_its_keys() {
    let source = "type M = { foo: boolean; \"12\": number; 42: string };\n\
         type P<T, K extends keyof T> = { [Q in K]: T[Q] };\n\
         type O = P<M, \"12\" | 42>;\n\
         declare const m: { [K in keyof O]: O[K] };\n\
         const x = m;\n";
    assert_eq!(type_of_last_initializer(source), "{ \"12\": number; 42: string; }");
}

/// §33: an intersection of mapped-alias references lists every constituent's
/// members, optional only where each constituent is, in declaration order
/// (§47; tsgo: `{ foo?: boolean | undefined; bar: number; }`).
#[test]
fn an_intersection_of_mapped_aliases_lists_its_members() {
    let source = "type M = { foo: boolean; bar: number };\n\
         type P<T, K extends keyof T> = { [Q in K]: T[Q] };\n\
         type O<T> = { [Q in keyof T]?: T[Q] };\n\
         type I = P<M, \"bar\"> & O<P<M, \"foo\">>;\n\
         declare const m: { [K in keyof I]: I[K] };\n\
         const x = m;\n";
    assert_eq!(type_of_last_initializer(source), "{ foo?: boolean | undefined; bar: number; }");
}

/// §35: a non-distributive infer conditional infers from the whole union
/// check (tsgo: `"email" | "text"`).
#[test]
fn a_non_distributive_infer_conditional_infers_from_a_union() {
    let source = "interface A { type: \"text\" }\n\
         interface B { type: \"email\" }\n\
         type C = A | B;\n\
         type T = C extends { type: infer R } ? R : never;\n\
         declare const t: T;\n\
         const x = t;\n";
    assert_eq!(type_of_last_initializer(source), "\"email\" | \"text\"");
}

/// §34: a generic mapped constraint's members are its lower bound's keys
/// (getLowerBoundOfKeyType; tsgo: `T["b"]`).
#[test]
fn a_generic_mapped_constraint_enumerates_its_lower_bound() {
    let source = "type Ex<T, U> = T extends U ? never : T;\n\
         type Pk<T, K extends keyof T> = { [P in K]: T[P] };\n\
         function f<T extends { a: string; b: string }>(obj: Pk<T, Ex<keyof T, \"a\">>) {\n\
             const x = obj.b;\n\
         }\n";
    assert_eq!(type_of_last_initializer(source), "T[\"b\"]");
}

/// §36: a reference to an instantiation-expression alias is the
/// instantiated signature (tsgo: `(x: string) => { v: string; }`).
#[test]
fn an_instantiation_expression_alias_reference_is_its_body() {
    let source = "declare function g<T>(x: T): { v: T };\n\
         type G<U> = typeof g<U>;\n\
         declare const v: G<string>;\n\
         const x = v;\n";
    assert_eq!(type_of_last_initializer(source), "(x: string) => { v: string; }");
}

/// §37: a reference to an alias declared as a deferred conditional
/// reference is the root alias's instance (tsgo: `C<T>`).
#[test]
fn a_deferred_conditional_reference_alias_instance_is_the_roots() {
    let source = "type C<P> = P extends string ? 1 : 2;\n\
         type D<T> = C<T>;\n\
         function f<T>(d: D<T>) {\n\
             const x = d;\n\
         }\n";
    assert_eq!(type_of_last_initializer(source), "C<T>");
}

/// §39: `keyof` over a deferred indexed access is one deferred index type
/// (tsgo: `keyof O[T]`).
#[test]
fn keyof_a_deferred_indexed_access_defers() {
    let source = "interface O { a: { x: string }; b: { y: string } }\n\
         type N<T extends keyof O> = keyof O[T];\n\
         function f<T extends keyof O>() {\n\
             declare const n: N<T>;\n\
             const x = n;\n\
         }\n";
    assert_eq!(type_of_last_initializer(source), "keyof O[T]");
}

/// §41: an argument-less qualified reference to a fully defaulted generic
/// alias prints its default arguments (tsgo: `NS.RT<number>`).
#[test]
fn a_bare_qualified_reference_fills_its_defaults() {
    let source = "declare namespace NS {\n\
             type RT<P = number> = { p: P };\n\
         }\n\
         declare const c: NS.RT;\n\
         const x = c;\n";
    assert_eq!(type_of_last_initializer(source), "NS.RT<number>");
}

/// §42: a partially written qualified reference fills its defaults
/// (tsgo: `NS.CC<string, number>`).
#[test]
fn a_partial_qualified_reference_fills_its_defaults() {
    let source = "declare namespace NS {\n\
             interface CC<P = {}, S = number> { p: P; s: S }\n\
         }\n\
         declare const c: NS.CC<string>;\n\
         const x = c;\n";
    assert_eq!(type_of_last_initializer(source), "NS.CC<string, number>");
}

/// §44: `readonly T` over a type parameter is the parameter (tsgo: `T31<number>`
/// reads `number`).
#[test]
fn readonly_over_a_type_parameter_is_the_parameter() {
    let source = "type T31<T> = readonly T;\n\
         declare const t: T31<number>;\n\
         const x = t;\n";
    assert_eq!(type_of_last_initializer(source), "number");
}

/// §45 (§21): `keyof` of a non-generic object that mentions a type
/// parameter has its keys now (tsgo: `string | number`).
#[test]
fn keyof_a_non_generic_object_mentioning_a_parameter_resolves() {
    let source = "interface Dictionary<T> { [key: string]: T }\n\
         function f<T>() {\n\
             declare const k: keyof Dictionary<T>;\n\
             const x = k;\n\
         }\n";
    assert_eq!(type_of_last_initializer(source), "string | number");
}

/// §47: mapped members are ordered by name when nothing links them
/// (compareSymbols; tsgo: `{ 1: 1; 10: 1; 2: 1; }`).
#[test]
fn mapped_members_are_ordered_by_name() {
    let source = "declare const m: { [K in 2 | 10 | 1]: 1 };\n\
         const x = m;\n";
    assert_eq!(type_of_last_initializer(source), "{ 1: 1; 10: 1; 2: 1; }");
}

/// §51: an unconstrained parameter is its own restrictive form, and a
/// conditional whose permissive relation holds while its restrictive one
/// fails is deferred under the reference's alias (tsgo: `[A<T>] extends
/// [0, 0]` is definitely false, so `T4<T>` is `0`).
#[test]
fn an_open_conditional_reference_defers_and_a_definite_outer_check_decides() {
    let source = "type A<T> = { x: T } extends { x: 0 } ? 1 : 0;\n\
         type T4<T> = [A<T>] extends [0, 0] ? 1 : 0;\n\
         function f<T>() {\n\
             declare const t: T4<T>;\n\
             const x = t;\n\
         }\n";
    assert_eq!(type_of_last_initializer(source), "0");
}

/// §52: a generic union body that reduces to one type is that type, with no
/// alias (tsgo: `type T23<T> = T | unknown` reads `unknown`).
#[test]
fn a_generic_union_body_reducing_to_unknown_is_unknown() {
    let source = "type T23<T> = T | unknown;\n\
         function f<T>(a: T23<T>) {\n\
             const x = a;\n\
         }\n";
    assert_eq!(type_of_last_initializer(source), "unknown");
}

/// §53: `keyof object` is `never`, so a generic key reads `T & object`
/// through a deferred access (tsgo: `(T & object)[keyof T]`).
#[test]
fn a_generic_key_reads_an_intersection_with_object() {
    let source = "function g<T>(obj: T & object, k: keyof T) {\n\
             const v = obj[k];\n\
         }\n";
    assert_eq!(type_of_last_initializer(source), "(T & object)[keyof T]");
}
