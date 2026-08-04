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
    let result = tsr_binder::bind(parsed.source_file, &parsed.nodes, "test.ts");
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
    assert_eq!(bound.result.symbols().get(symbol).declarations.as_slice(), [id]);
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

#[test]
fn a_jsx_attribute_declares_a_property_on_the_attributes_object() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse_with_script_kind(
        &arena,
        "declare const X: any;\nconst e = <X icon={1} label=\"a\" />;",
        tsr_parser::ScriptKind::Tsx,
    );
    assert!(parsed.diagnostics.is_empty(), "the source should parse cleanly");
    let result = tsr_binder::bind(parsed.source_file, &parsed.nodes, "test.ts");

    // The attributes object is anonymous — no expression names it — so the
    // attributes hang off an internal symbol rather than off anything in scope.
    let attributes = result
        .symbols()
        .iter()
        .find(|(_, symbol)| symbol.name == "__jsxAttributes")
        .map(|(id, _)| id)
        .expect("the attributes object gets an anonymous symbol");
    let members = &result.symbols().get(attributes).members;
    assert!(members.contains_key("icon"), "`icon` is a property of the attributes");
    assert!(members.contains_key("label"), "`label` is too");
}

/// Bind under a chosen file name, which is what decides module-vs-script naming
/// and whether the file is an ambient declaration file.
fn bind_as<'a>(arena: &'a Arena, source: &'a str, file_name: &'a str) -> Bound<'a> {
    let parsed = tsr_parser::parse(arena, source);
    assert!(parsed.diagnostics.is_empty(), "the source should parse cleanly");
    let result = tsr_binder::bind(parsed.source_file, &parsed.nodes, file_name);
    Bound { parsed, result }
}

impl Bound<'_> {
    /// The file's own symbol, which exists only for an external module.
    fn module_symbol(&self) -> Option<tsr_binder::SymbolId> {
        self.result.symbol_of(self.root())
    }

    /// A top-level local's symbol id.
    fn top_level_symbol(&self, name: &str) -> Option<tsr_binder::SymbolId> {
        self.result.lookup_local(self.root(), name)
    }

    /// What the module exports under `name`.
    fn export(&self, name: &str) -> Option<tsr_binder::SymbolId> {
        let module = self.module_symbol()?;
        self.result.symbols().get(module).exports.get(name).copied()
    }
}

#[test]
fn a_file_with_no_import_or_export_is_a_script() {
    let arena = Arena::new();
    let bound = bind_as(&arena, "const x = 1;", "a.ts");
    assert!(bound.module_symbol().is_none(), "a script has no symbol of its own");
    // Its declarations are still locals, reachable by name.
    assert!(bound.top_level("x").is_some());
}

#[test]
fn a_top_level_export_makes_the_file_a_module_named_after_its_path() {
    let arena = Arena::new();
    let bound = bind_as(&arena, "export const x = 1;", "src/a.ts");
    let module = bound.module_symbol().expect("an external module has a symbol");
    // Upstream: `"\"" + RemoveFileExtension(fileName) + "\""`. We store the value
    // rather than the quoted spelling; see the binder's `bind_source_file`.
    assert_eq!(bound.result.symbols().get(module).name, "src/a");
    assert!(bound.export("x").is_some(), "`x` is an export of the module");
}

#[test]
fn an_exported_declaration_gets_both_a_local_and_an_export() {
    // The distinction upstream's `declareModuleMember` explains at length: an
    // unqualified reference inside the file resolves to the local.
    let arena = Arena::new();
    let bound = bind_as(&arena, "export class C {}\n", "a.ts");
    let local = bound.top_level("C").expect("the local half exists");
    assert!(local.contains(SymbolFlags::EXPORT_VALUE), "the local is marked exported: {local:?}");
    let exported = bound.export("C").expect("the export half exists");
    assert!(bound.result.symbols().get(exported).flags.contains(SymbolFlags::CLASS));
}

#[test]
fn every_default_export_in_a_file_shares_one_symbol() {
    // `export default function f` and `export default interface F` merge, because
    // the *export* is named `default` however the declaration was named.
    let arena = Arena::new();
    let bound = bind_as(
        &arena,
        "export default function foo(): void;\nexport default interface Foo {}\n",
        "a.ts",
    );
    let default = bound.export("default").expect("the file has a default export");
    assert_eq!(bound.result.symbols().get(default).declarations.len(), 2);
    // The local half keeps the name the source wrote.
    assert!(bound.top_level("foo").is_some());
    assert!(bound.top_level("Foo").is_some());
}

#[test]
fn an_unnamed_default_export_has_no_local() {
    let arena = Arena::new();
    let bound = bind_as(&arena, "export default class { m() {} }\n", "a.ts");
    assert!(bound.export("default").is_some());
    // "No local symbol for an unnamed default!" — there is no name to use.
    assert_eq!(bound.top_level_names(), Vec::<&str>::new());
}

#[test]
fn a_declaration_file_exports_everything_it_declares() {
    // An ambient file with no `export` statement is an export context: upstream's
    // `setExportContextFlag`.
    let arena = Arena::new();
    let bound = bind_as(&arena, "import \"./x\";\ndeclare var y: number;\n", "a.d.ts");
    assert!(bound.export("y").is_some(), "implicitly exported");
}

#[test]
fn a_umd_global_name_is_not_an_export() {
    // `export as namespace N` claims a *global*; putting it in `exports` would
    // make `import { N }` resolve.
    let arena = Arena::new();
    let bound =
        bind_as(&arena, "export as namespace N;\nexport declare var y: number;\n", "a.d.ts");
    assert!(bound.export("N").is_none(), "not an export");
    assert!(bound.result.global_exports().contains_key("N"), "a global the module claims");
}

#[test]
fn a_script_with_an_export_keyword_still_declares_a_local() {
    // A `export` in a script is an error the checker reports; the binder must not
    // lose the declaration over it.
    let arena = Arena::new();
    let bound = bind_as(&arena, "namespace M { export const x = 1; }", "a.ts");
    assert!(bound.top_level("M").is_some());
}

#[test]
fn a_commonjs_export_makes_a_javascript_file_a_module() {
    // `module.exports = x` is what `export = x` is in TypeScript, and it is the
    // only thing that tells a `.js` script it is a module.
    let arena = Arena::new();
    let bound = bind_as(&arena, "class Bar {}\nmodule.exports = Bar;\n", "index.js");
    assert!(bound.module_symbol().is_some(), "the file has a symbol of its own");
    assert!(bound.export("export=").is_some(), "filed under the same name as `export =`");
    // `module` and `exports` are declared nowhere in the source, so the binder
    // declares them; `module.exports` resolves through `module`'s members.
    let module = bound.top_level_symbol("module").expect("`module` is a local");
    assert!(bound.result.symbols().get(module).members.contains_key("exports"));
    assert!(bound.top_level("exports").is_some());
}

#[test]
fn the_same_file_in_typescript_declares_nothing_of_the_kind() {
    // In a `.ts` file `module.exports = x` is an assignment to a global, not a
    // declaration — which is why the whole form is gated on the file's dialect.
    let arena = Arena::new();
    let bound = bind_as(&arena, "class Bar {}\nmodule.exports = Bar;\n", "index.ts");
    assert!(bound.module_symbol().is_none());
    assert!(bound.top_level("module").is_none());
}

#[test]
fn a_commonjs_named_export_is_an_export_of_the_file() {
    let arena = Arena::new();
    let bound = bind_as(&arena, "exports.foo = 1;\nmodule.exports.bar = 2;\n", "index.js");
    assert!(bound.export("foo").is_some());
    assert!(bound.export("bar").is_some());
}

#[test]
fn this_property_assignment_declares_a_class_member_in_javascript() {
    let arena = Arena::new();
    let bound = bind_as(&arena, "class C {\n  constructor() {\n    this.x = 1;\n  }\n}\n", "a.js");
    let class = bound.top_level_symbol("C").expect("the class");
    assert!(bound.result.symbols().get(class).members.contains_key("x"));
}

#[test]
fn a_real_declaration_beats_a_this_property_of_the_same_name() {
    // `this.m = this.m.bind(this)` in a constructor must not turn the method
    // into a second declaration — upstream's isReplaceableByMethod.
    let arena = Arena::new();
    let bound = bind_as(
        &arena,
        "class C {\n  m() {}\n  constructor() {\n    this.m = this.m.bind(this);\n  }\n}\n",
        "a.js",
    );
    let class = bound.top_level_symbol("C").expect("the class");
    let member = *bound.result.symbols().get(class).members.get("m").expect("`m` exists");
    assert_eq!(
        bound.result.symbols().get(member).declarations.len(),
        1,
        "the method is the only declaration"
    );
}

#[test]
fn this_outside_a_class_member_declares_nothing() {
    // Upstream marks the constructor-function case unimplemented, and so is it
    // here; the important part is that it does not attach to whatever happens
    // to be the enclosing owner.
    let arena = Arena::new();
    let bound = bind_as(&arena, "function f() {\n  this.x = 1;\n}\n", "a.js");
    assert!(bound.result.symbols().iter().all(|(_, symbol)| symbol.name != "x"));
}

#[test]
fn a_contextual_keyword_in_expression_position_is_a_name() {
    // `module`, `type`, `of` and friends are keywords only where the grammar
    // says so; as values they are ordinary identifiers, and parsing them as
    // keyword expressions lost the text entirely.
    let arena = Arena::new();
    let bound = bind_as(&arena, "const type = 1;\nconst x = type;\n", "a.ts");
    assert!(bound.top_level("type").is_some());
}

#[test]
fn a_property_assigned_to_a_function_declares_on_it() {
    // `f.cache = …` is an expando: TypeScript reads it as a declaration in
    // `.ts` files too, which is why this one is not gated on the dialect.
    let arena = Arena::new();
    let bound = bind_as(&arena, "function f() {}\nf.cache = 1;\n", "a.ts");
    let f = bound.top_level_symbol("f").expect("the function");
    assert!(bound.result.symbols().get(f).exports.contains_key("cache"));
}

#[test]
fn an_expando_reaches_a_target_declared_after_it() {
    // The whole reason the pass is deferred: `f` is bound after the assignment
    // that extends it.
    let arena = Arena::new();
    let bound = bind_as(&arena, "f.late = 1;\nfunction f() {}\n", "a.ts");
    let f = bound.top_level_symbol("f").expect("the function");
    assert!(bound.result.symbols().get(f).exports.contains_key("late"));
}

#[test]
fn an_expando_on_a_const_lands_on_the_function_expression() {
    // Upstream's getInitializerSymbol: the properties belong to the initializer's
    // symbol, not to the variable's.
    let arena = Arena::new();
    let bound = bind_as(&arena, "const g = function () {};\ng.y = 1;\n", "a.ts");
    let g = bound.top_level_symbol("g").expect("the variable");
    assert!(bound.result.symbols().get(g).exports.is_empty(), "not on the variable");
    let function = bound
        .result
        .symbols()
        .iter()
        .find(|(_, symbol)| symbol.name == "__function")
        .map(|(id, _)| id)
        .expect("the function expression has an anonymous symbol");
    assert!(bound.result.symbols().get(function).exports.contains_key("y"));
}

#[test]
fn a_let_only_carries_expandos_in_javascript() {
    // `const` or a JavaScript file; a `let` in TypeScript may be reassigned, so
    // its initializer is not the declaration of anything.
    let arena = Arena::new();
    let typescript = bind_as(&arena, "let h = function () {};\nh.y = 1;\n", "a.ts");
    assert!(typescript.result.symbols().iter().all(|(_, symbol)| symbol.name != "y"));

    let javascript = bind_as(&arena, "let h = function () {};\nh.y = 1;\n", "a.js");
    assert!(javascript.result.symbols().iter().any(|(_, symbol)| symbol.name == "y"));
}

#[test]
fn a_real_declaration_beats_an_expando_of_the_same_name() {
    // "We declare expandos only when there are no non-expando declarations for
    // that name."
    let arena = Arena::new();
    let bound = bind_as(&arena, "class C {\n  static x = 1;\n}\nC.x = 2;\n", "a.js");
    let c = bound.top_level_symbol("C").expect("the class");
    // The static member is in `members` here rather than `exports`, so the
    // expando is the only thing in `exports` — what matters is that it did not
    // merge into, or displace, the declared one.
    let member = *bound.result.symbols().get(c).members.get("x").expect("`static x`");
    assert_eq!(bound.result.symbols().get(member).declarations.len(), 1);
}

#[test]
fn object_define_property_declares_what_it_names() {
    let arena = Arena::new();
    let bound =
        bind_as(&arena, "function h() {}\nObject.defineProperty(h, 'w', { value: 1 });\n", "a.js");
    let h = bound.top_level_symbol("h").expect("the function");
    assert!(bound.result.symbols().get(h).exports.contains_key("w"));

    // The `exports` form goes to the file instead.
    let on_exports =
        bind_as(&arena, "Object.defineProperty(exports, 'v', { value: 1 });\n", "a.js");
    assert!(on_exports.export("v").is_some());
}

#[test]
fn an_assignment_to_a_call_result_declares_nothing() {
    // `f().x = 1` assigns to whatever `f()` returned; only a *name* on the left
    // makes a declaration.
    let arena = Arena::new();
    let bound = bind_as(&arena, "function f() { return {}; }\nf().x = 1;\n", "a.ts");
    assert!(bound.result.symbols().iter().all(|(_, symbol)| symbol.name != "x"));
}
