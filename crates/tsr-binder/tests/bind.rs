//! What the binder must get right, stated as scoping questions.
//!
//! Assertions go through the resolved scope tables rather than through symbol
//! counts: "there are four symbols" passes for the wrong four, while "`x` in this
//! block resolves to the `let`, not the outer `var`" does not.

use tsr_ast::{Node, NodeTable};
use tsr_binder::{BindResult, SymbolFlags};
use tsr_core::Arena;
use tsr_parser::ParsedSourceFile;

struct Bound<'a> {
    parsed: ParsedSourceFile<'a>,
    result: BindResult<'a>,
}

fn bind<'a>(arena: &'a Arena, source: &'a str) -> Bound<'a> {
    let parsed = tsr_parser::parse(arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "the source should parse cleanly: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    // SAFETY-free: `result` borrows the same arena as `parsed`, and both live
    // as long as the caller's `arena`.
    let result = tsr_binder::bind(parsed.source_file, &parsed.nodes);
    Bound { parsed, result }
}

impl Bound<'_> {
    fn nodes(&self) -> &NodeTable {
        &self.parsed.nodes
    }

    fn root(&self) -> tsr_ast::NodeId {
        Node::SourceFile(self.parsed.source_file).node_id().expect("registered")
    }

    /// Flags of a name resolved from the file's top-level scope.
    fn top_level(&self, name: &str) -> Option<SymbolFlags> {
        let id = self.result.lookup_local(self.root(), name)?;
        Some(self.result.symbols().get(id).flags)
    }

    /// Names declared at the top level, sorted.
    fn top_level_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .result
            .locals(self.root())
            .map(|t| t.keys().copied().collect())
            .unwrap_or_default();
        names.sort_unstable();
        names
    }
}

#[test]
fn top_level_declarations_get_the_flags_they_should() {
    let arena = Arena::new();
    let bound = bind(
        &arena,
        "var v = 1;\nfunction f() {}\nclass C {}\ninterface I {}\ntype T = number;\nenum E { A }\nnamespace N {}",
    );
    assert_eq!(bound.top_level("f"), Some(SymbolFlags::FUNCTION));
    assert_eq!(bound.top_level("C"), Some(SymbolFlags::CLASS));
    assert_eq!(bound.top_level("I"), Some(SymbolFlags::INTERFACE));
    assert_eq!(bound.top_level("T"), Some(SymbolFlags::TYPE_ALIAS));
    assert_eq!(bound.top_level("E"), Some(SymbolFlags::REGULAR_ENUM));
    assert_eq!(bound.top_level("N"), Some(SymbolFlags::VALUE_MODULE));
}

#[test]
fn function_parameters_are_scoped_to_the_function_not_the_file() {
    let arena = Arena::new();
    let bound = bind(&arena, "function f(a, b) { return a; }");
    assert_eq!(bound.top_level_names(), ["f"], "parameters must not leak to the file scope");

    // Inside the function, both parameters resolve.
    let function = bound.parsed.source_file.statements[0];
    let function_id = Node::from(function).node_id().expect("registered");
    for parameter in ["a", "b"] {
        assert!(
            bound.result.lookup_local(function_id, parameter).is_some(),
            "{parameter} should be a local of the function"
        );
    }
}

#[test]
fn a_block_scopes_let_but_not_var() {
    // The behaviour that makes `var` hoisting real: the `var` lands in the
    // function scope even though it is written inside a block.
    let arena = Arena::new();
    let bound = bind(&arena, "function f() { { var hoisted = 1; let scoped = 2; } }");

    let function = bound.parsed.source_file.statements[0];
    let function_id = Node::from(function).node_id().expect("registered");
    let function_locals: Vec<&str> = {
        let mut names: Vec<&str> = bound
            .result
            .locals(function_id)
            .map(|t| t.keys().copied().collect())
            .unwrap_or_default();
        names.sort_unstable();
        names
    };
    assert!(
        function_locals.contains(&"hoisted"),
        "`var` should hoist to the function scope, got {function_locals:?}"
    );
    assert!(
        !function_locals.contains(&"scoped"),
        "`let` should stay in the block, got {function_locals:?}"
    );
}

#[test]
fn class_members_go_on_the_class_not_in_the_enclosing_scope() {
    let arena = Arena::new();
    let bound = bind(&arena, "class C { x = 1; m() {} get g() { return 1; } }");
    assert_eq!(bound.top_level_names(), ["C"], "members must not leak");

    let class = bound.result.lookup_local(bound.root(), "C").expect("class C");
    let members = &bound.result.symbols().get(class).members;
    let mut names: Vec<&str> = members.keys().copied().collect();
    names.sort_unstable();
    assert_eq!(names, ["g", "m", "x"]);

    let method = members["m"];
    assert_eq!(bound.result.symbols().get(method).flags, SymbolFlags::METHOD);
}

#[test]
fn interface_members_and_enum_members_land_on_their_owner() {
    let arena = Arena::new();
    let bound = bind(&arena, "interface I { a: number; b(): void }\nenum E { X, Y }");

    let interface = bound.result.lookup_local(bound.root(), "I").expect("interface I");
    let mut members: Vec<&str> =
        bound.result.symbols().get(interface).members.keys().copied().collect();
    members.sort_unstable();
    assert_eq!(members, ["a", "b"]);

    let enumeration = bound.result.lookup_local(bound.root(), "E").expect("enum E");
    let mut values: Vec<&str> =
        bound.result.symbols().get(enumeration).members.keys().copied().collect();
    values.sort_unstable();
    assert_eq!(values, ["X", "Y"]);
}

#[test]
fn declarations_that_typescript_merges_produce_one_symbol() {
    let arena = Arena::new();
    for (source, expected) in [
        ("interface I { a: number }\ninterface I { b: number }", SymbolFlags::INTERFACE),
        ("class C {}\ninterface C {}", SymbolFlags::CLASS | SymbolFlags::INTERFACE),
        ("function f() {}\nnamespace f {}", SymbolFlags::FUNCTION | SymbolFlags::VALUE_MODULE),
        ("function f(): void;\nfunction f() {}", SymbolFlags::FUNCTION),
    ] {
        let bound = bind(&arena, source);
        let name = if source.contains("I ") {
            "I"
        } else if source.contains("C ") {
            "C"
        } else {
            "f"
        };
        assert_eq!(
            bound.top_level("name").or(bound.top_level(name)),
            Some(expected),
            "for {source:?}"
        );
        assert!(
            bound.result.diagnostics().is_empty(),
            "merging should not report a duplicate for {source:?}"
        );
    }
}

#[test]
fn declarations_that_collide_report_a_duplicate() {
    let arena = Arena::new();
    for source in [
        "let a = 1;\nlet a = 2;",
        "const b = 1;\nfunction b() {}",
        "type T = number;\ntype T = string;",
    ] {
        let bound = bind(&arena, source);
        assert!(
            !bound.result.diagnostics().is_empty(),
            "{source:?} should report a duplicate identifier"
        );
    }
}

#[test]
fn merged_declarations_are_all_recorded_on_the_symbol() {
    // Both interface declarations must be reachable from the symbol, since the
    // checker builds the type from every one of them.
    let arena = Arena::new();
    let bound = bind(&arena, "interface I { a: number }\ninterface I { b: number }");
    let interface = bound.result.lookup_local(bound.root(), "I").expect("interface I");
    assert_eq!(bound.result.symbols().get(interface).declarations.len(), 2);
}

#[test]
fn a_declaration_can_be_found_from_its_node_and_back() {
    let arena = Arena::new();
    let bound = bind(&arena, "function f() {}");
    let function = Node::from(bound.parsed.source_file.statements[0]);
    let id = function.node_id().expect("registered");
    let symbol = bound.result.symbol_of(id).expect("the function declares a symbol");
    assert_eq!(bound.result.symbols().get(symbol).name, "f");
    assert_eq!(bound.result.symbols().get(symbol).declarations, [id]);
    assert_eq!(bound.result.symbols().get(symbol).value_declaration, Some(id));
}

#[test]
fn lexical_resolution_walks_outward_and_stops_at_the_nearest_binding() {
    let arena = Arena::new();
    let source = "var outer = 1;\nfunction f(shadow) { return shadow; }";
    let bound = bind(&arena, source);

    let function = Node::from(bound.parsed.source_file.statements[1]);
    let function_id = function.node_id().expect("registered");

    // From inside the function, both the parameter and the outer var resolve.
    assert!(bound.result.resolve(bound.nodes(), function_id, "shadow").is_some());
    assert!(bound.result.resolve(bound.nodes(), function_id, "outer").is_some());
    // The parameter is not visible from the file scope.
    assert!(bound.result.resolve(bound.nodes(), bound.root(), "shadow").is_none());
}

#[test]
fn the_bind_result_can_be_read_from_several_threads() {
    // The property the parallel checker needs. A compile-time assertion plus a
    // use, for the same reason as the AST's.
    const fn assert_sync<T: Sync>() {}
    assert_sync::<SymbolFlags>();

    let arena = Arena::new();
    let bound = bind(&arena, "class C { x = 1; m() {} }\nfunction f(a) { return a; }");
    let result = &bound.result;
    let root = bound.root();

    let counts: Vec<usize> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(move || result.locals(root).map_or(0, std::collections::HashMap::len))
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("thread panicked")).collect()
    });
    assert!(counts.iter().all(|c| *c == counts[0]), "threads disagreed: {counts:?}");
}

#[test]
fn a_destructuring_declaration_declares_one_symbol_per_name() {
    let arena = Arena::new();
    let bound =
        bind(&arena, "const { a, b: renamed } = { a: 1, b: 2 };\nconst [first, second] = [1, 2];");
    // The pattern declares nothing; each element does.
    for name in ["a", "renamed", "first", "second"] {
        assert!(
            bound.top_level(name).is_some(),
            "{name} should be declared by the pattern, got {:?}",
            bound.top_level_names()
        );
    }
    assert_eq!(
        bound.top_level("a"),
        Some(SymbolFlags::BLOCK_SCOPED_VARIABLE),
        "a `const` pattern declares block-scoped names"
    );
}

#[test]
fn a_catch_clause_variable_is_scoped_to_the_clause() {
    let arena = Arena::new();
    let bound = bind(&arena, "function f() {\n  try { 1; } catch (e) { e; }\n}");
    // `catch (e)` is neither `let` nor `var`; upstream still block-scopes it, and
    // reading only the declaration list's flags made it function-scoped.
    assert!(bound.top_level("e").is_none(), "the catch variable must not escape to the file scope");
}

#[test]
fn an_exported_namespace_member_is_declared_twice() {
    let arena = Arena::new();
    let bound = bind(&arena, "namespace M { export const X = 1; const Y = 2; }");
    let module = bound.result.lookup_local(bound.root(), "M").expect("M is declared");
    let symbols = bound.result.symbols();

    // The export, reachable as `M.X` …
    let exported = symbols.get(module).exports.get("X").copied().expect("M exports X");
    assert_eq!(symbols.get(exported).parent, Some(module));
    assert_eq!(symbols.get(exported).name, "X");
    // An unexported member stays local only.
    assert!(!symbols.get(module).exports.contains_key("Y"), "`Y` is not exported");
}

#[test]
fn a_parameter_property_also_declares_a_class_member() {
    let arena = Arena::new();
    let bound = bind(&arena, "class C { constructor(public p: number, q: number) {} }");
    let class = bound.result.lookup_local(bound.root(), "C").expect("C is declared");
    let members = &bound.result.symbols().get(class).members;
    assert!(members.contains_key("p"), "`public p` is a property of C");
    assert!(!members.contains_key("q"), "a plain parameter is not");
}

#[test]
fn an_object_literal_does_not_put_its_properties_on_the_enclosing_symbol() {
    let arena = Arena::new();
    let bound = bind(&arena, "interface I { salt: number; }\nconst x: I = { salt: 2, pepper: 0 };");
    let interface = bound.result.lookup_local(bound.root(), "I").expect("I is declared");
    let members = &bound.result.symbols().get(interface).members;
    assert!(members.contains_key("salt"), "the interface declares `salt`");
    assert!(
        !members.contains_key("pepper"),
        "the object literal's properties belong to the literal, not to `I`"
    );
}

#[test]
fn a_dotted_namespace_name_declares_nested_namespaces() {
    let arena = Arena::new();
    let bound = bind(&arena, "namespace A.B { export const x = 1; }");
    let symbols = bound.result.symbols();
    // `namespace A.B {}` means `namespace A { export namespace B {} }`, so only
    // `A` is visible at the top level and `B` hangs off it.
    assert_eq!(bound.top_level_names(), vec!["A"]);
    let outer = bound.result.lookup_local(bound.root(), "A").expect("A is declared");
    let inner = symbols
        .get(outer)
        .exports
        .get("B")
        .copied()
        .expect("the inner namespace is exported from the outer");
    assert_eq!(symbols.get(inner).name, "B");
    assert!(
        symbols.get(inner).exports.contains_key("x"),
        "the body belongs to the innermost segment"
    );
}

#[test]
fn a_computed_property_name_that_is_a_literal_declares_statically() {
    let arena = Arena::new();
    let bound = bind(&arena, "class C { ['a']() {} [2]() {} [\"b\"]() {} }");
    let class = bound.result.lookup_local(bound.root(), "C").expect("C is declared");
    let members = &bound.result.symbols().get(class).members;
    for name in ["a", "2", "b"] {
        assert!(members.contains_key(name), "[{name}] names a member statically");
    }
}

#[test]
fn a_late_bound_computed_name_declares_nothing() {
    let arena = Arena::new();
    let bound =
        bind(&arena, "declare const k: string;\nclass C { [k]() {} [Symbol.iterator]() {} }");
    let class = bound.result.lookup_local(bound.root(), "C").expect("C is declared");
    // The name is whatever the expression evaluates to, which needs the checker.
    // Declaring *something* here would be worse than declaring nothing: it would
    // be a symbol under a name no reference can ever match.
    assert!(
        bound.result.symbols().get(class).members.is_empty(),
        "a late-bound member is invisible until the checker resolves its name"
    );
}
