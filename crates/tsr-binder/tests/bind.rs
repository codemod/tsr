//! What the binder must get right, stated as scoping questions.
//!
//! Assertions go through the resolved scope tables rather than through symbol
//! counts: "there are four symbols" passes for the wrong four, while "`x` in this
//! block resolves to the `let`, not the outer `var`" does not.

use tsr_ast::{Node, NodeTable};
use tsr_binder::{BindResult, SymbolFlags, SymbolId};
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
    let result = tsr_binder::bind(
        arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
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
        "var v = 1;\nfunction f() {}\nclass C {}\ninterface I {}\ntype T = number;\nenum E { A }\nnamespace N { export var x = 1; }\nnamespace Types { interface X {} }",
    );
    assert_eq!(bound.top_level("f"), Some(SymbolFlags::FUNCTION));
    assert_eq!(bound.top_level("C"), Some(SymbolFlags::CLASS));
    assert_eq!(bound.top_level("I"), Some(SymbolFlags::INTERFACE));
    assert_eq!(bound.top_level("T"), Some(SymbolFlags::TYPE_ALIAS));
    assert_eq!(bound.top_level("E"), Some(SymbolFlags::REGULAR_ENUM));
    // `bindModuleDeclaration` (`binder.go:1268`) picks the module flag from
    // `GetModuleInstanceState`: a namespace that emits JavaScript is a
    // `ValueModule` and one that emits nothing is a `NamespaceModule`. The two
    // get different excludes, which is the whole point of the distinction —
    // `checker-notes-diag2.md` §95.
    assert_eq!(bound.top_level("N"), Some(SymbolFlags::VALUE_MODULE));
    assert_eq!(bound.top_level("Types"), Some(SymbolFlags::NAMESPACE_MODULE));
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
fn an_interface_takes_members_and_an_enum_takes_exports() {
    let arena = Arena::new();
    let bound = bind(&arena, "interface I { a: number; b(): void }\nenum E { X, Y }");

    // `declareSymbolAndAddToSymbolTable`'s interface branch is
    // `declareSymbol(ast.GetMembers(...))` (`internal/binder/binder.go:438-439`).
    let interface = bound.result.lookup_local(bound.root(), "I").expect("interface I");
    let mut members: Vec<&str> =
        bound.result.symbols().get(interface).members.keys().copied().collect();
    members.sort_unstable();
    assert_eq!(members, ["a", "b"]);

    // The enum branch is its own case one line earlier and takes the **other**
    // table: `declareSymbol(ast.GetExports(...))`
    // (`internal/binder/binder.go:436-437`).
    //
    // This assertion read `members` until the container-aware remap landed, which
    // pinned a real divergence from upstream: the checker reads `exports` for a
    // `typeof E` receiver (`checker.go:20672`), so `E.X` resolved to nothing and
    // ~1,560 assertion lines answered `error`. The two tables are asserted
    // together, and the interface half is what keeps the fix from being "move
    // everything to exports".
    let enumeration = bound.result.lookup_local(bound.root(), "E").expect("enum E");
    let mut values: Vec<&str> =
        bound.result.symbols().get(enumeration).exports.keys().copied().collect();
    values.sort_unstable();
    assert_eq!(values, ["X", "Y"]);
    assert!(
        bound.result.symbols().get(enumeration).members.is_empty(),
        "an enum member must be in `exports` and nowhere else — a symbol in both \
         tables would let a lookup succeed against the wrong one"
    );
}

#[test]
fn declarations_that_typescript_merges_produce_one_symbol() {
    let arena = Arena::new();
    for (source, expected) in [
        ("interface I { a: number }\ninterface I { b: number }", SymbolFlags::INTERFACE),
        ("class C {}\ninterface C {}", SymbolFlags::CLASS | SymbolFlags::INTERFACE),
        (
            "function f() {}\nnamespace f { export var x = 1; }",
            SymbolFlags::FUNCTION | SymbolFlags::VALUE_MODULE,
        ),
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
    assert!(resolve_value(&bound, function_id, "shadow").is_some());
    assert!(resolve_value(&bound, function_id, "outer").is_some());
    // The parameter is not visible from the file scope.
    assert!(resolve_value(&bound, bound.root(), "shadow").is_none());
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
    let source = "declare const X: any;\nconst e = <X icon={1} label=\"a\" ns:href={2} />;";
    let parsed = tsr_parser::parse_with_script_kind(&arena, source, tsr_parser::ScriptKind::Tsx);
    assert!(parsed.diagnostics.is_empty(), "the source should parse cleanly");
    let result = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.tsx", text: source },
    );

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
    // A namespaced name is one name upstream, spelled with the colon. No node
    // holds the whole of it, so it is the source range the two halves span.
    assert!(members.contains_key("ns:href"), "and so is `ns:href`: {:?}", members.keys());
}

/// Bind under a chosen file name, which is what decides module-vs-script naming
/// and whether the file is an ambient declaration file.
fn bind_as<'a>(arena: &'a Arena, source: &'a str, file_name: &'a str) -> Bound<'a> {
    let parsed = tsr_parser::parse(arena, source);
    assert!(parsed.diagnostics.is_empty(), "the source should parse cleanly");
    let result = tsr_binder::bind(
        arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: file_name, text: source },
    );
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
    // `static x` is an *export* of the class, not a member — upstream's
    // `declareClassMember` splits on `IsStatic` (`binder.go:415`). That puts the
    // declared static and the expando `C.x = 2` in the same table, which is what
    // makes this test meaningful: the expando now has something to collide with,
    // and must lose. Before the split they were in different tables and could not
    // have met, so the assertion held for the wrong reason.
    let symbols = bound.result.symbols();
    assert!(!symbols.get(c).members.contains_key("x"), "`static x` is not an instance member");
    let exported = *symbols.get(c).exports.get("x").expect("`static x`");
    assert_eq!(
        symbols.get(exported).declarations.len(),
        1,
        "the expando neither merged into nor displaced the declared static"
    );
    assert!(!symbols.get(exported).flags.contains(SymbolFlags::ASSIGNMENT));
}

#[test]
fn a_static_and_an_instance_member_of_the_same_name_are_different_symbols() {
    // `declareClassMember` splits on `IsStatic` (`binder.go:414-419`): a static
    // member is an export of the class, an instance member is a member of it.
    // Sharing one table merged them into a single symbol — silently for methods
    // and properties, and as a spurious `TS2300` for accessors, which is how it
    // was found. Neither conformance suite could see it: `binder_symbols` did not
    // move when this was fixed.
    let arena = Arena::new();
    let bound = bind_as(
        &arena,
        "class C {\n  static m() {}\n  m() {}\n  static get x() { return 1; }\n  get x() { return 1; }\n}\n",
        "a.ts",
    );
    let c = bound.top_level_symbol("C").expect("the class");
    let symbols = bound.result.symbols();
    for name in ["m", "x"] {
        let statik = *symbols.get(c).exports.get(name).unwrap_or_else(|| panic!("static {name}"));
        let instance = *symbols.get(c).members.get(name).unwrap_or_else(|| panic!("{name}"));
        assert_ne!(statik, instance, "`static {name}` and `{name}` must not share a symbol");
        assert_eq!(symbols.get(statik).declarations.len(), 1);
        assert_eq!(symbols.get(instance).declarations.len(), 1);
    }
    assert!(bound.result.diagnostics().is_empty(), "no redeclaration here");
}

#[test]
fn block_scoped_declarations_go_in_the_block_not_the_function() {
    // Interfaces, type aliases, enums and function declarations all reach
    // `bindBlockScopedDeclaration` upstream (`binder.go:681`, `:693`, `:1158`,
    // `:1216`), which files them in `GetLocals(b.blockScopeContainer)`. Filing
    // them in the enclosing *function* instead made two declarations in sibling
    // blocks collide. Only visible inside a nested block: at the top of a function
    // the block and the container are the same node.
    let arena = Arena::new();
    let bound = bind_as(
        &arena,
        "function f() {\n  { type A = string; interface I {} enum E {} function g() {} }\n  { type A = number; interface I {} enum E {} function g() {} }\n}\n",
        "a.ts",
    );
    assert!(
        bound.result.diagnostics().is_empty(),
        "sibling blocks are separate scopes, got {:?}",
        bound.result.diagnostics()
    );
}

#[test]
fn an_export_specifier_is_an_export_not_a_local() {
    // `declareModuleMember` sends an alias to the container's exports when the node
    // is an `ExportSpecifier`, testing the kind rather than looking for a modifier
    // (`binder.go:377`) — there is no `export` keyword on the specifier to find, it
    // belongs to the `export { … }` declaration. Treating it as a local made it
    // collide with the identically-named local of an import in the same file.
    let arena = Arena::new();
    let bound = bind_as(
        &arena,
        "import { a as a1 } from \"m\";\nexport { a as a1 } from \"m\";\na1;\n",
        "a.ts",
    );
    assert!(
        bound.result.diagnostics().is_empty(),
        "an import local and a re-export of the same name do not collide, got {:?}",
        bound.result.diagnostics()
    );
    // The import keeps a local; the re-export is only ever an export.
    let exported = bound.export("a1").expect("the re-export");
    let local = bound.top_level_symbol("a1").expect("the import's local");
    assert_ne!(exported, local, "the export and the local are separate symbols");
    assert_eq!(
        bound.result.symbols().get(local).declarations.len(),
        1,
        "the import declares the local alone"
    );
}

#[test]
fn a_named_class_expressions_name_is_visible_only_inside_it() {
    // `bindClassLikeDeclaration` splits on the *kind*, not on whether there is a
    // name (`binder.go:942-951`): a class *expression* always goes through
    // `bindAnonymousDeclaration`, which takes the written name as the symbol's name
    // and files the symbol in no table. The name is in scope only inside the class,
    // exactly as for a named function expression.
    let arena = Arena::new();
    let bound = bind_as(&arena, "class C { }\nconst C9 = class C { };\n", "a.ts");
    assert!(
        bound.result.diagnostics().is_empty(),
        "the class expression's `C` does not collide with the declaration, got {:?}",
        bound.result.diagnostics()
    );
    // The declaration owns the only top-level `C`, with one declaration.
    let declared = bound.top_level_symbol("C").expect("`class C`");
    assert_eq!(bound.result.symbols().get(declared).declarations.len(), 1);
}

#[test]
fn a_second_export_default_is_not_a_duplicate_identifier() {
    // `A_module_cannot_have_multiple_default_exports` (TS2528), not TS2300 —
    // `binder.go:224-244`, overriding the enum and block-scoped message choices.
    // Reported on every declaration involved, and with no name argument.
    let arena = Arena::new();
    let bound = bind_as(&arena, "export default class D { }\nexport default { };\n", "a.ts");
    let codes: Vec<u32> = bound.result.diagnostics().iter().map(|d| d.message.code()).collect();
    assert_eq!(codes, vec![2528, 2528], "one per declaration, got {codes:?}");
    assert!(
        bound.result.diagnostics().iter().all(|d| d.args.is_empty()),
        "the message takes no name argument"
    );
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

#[test]
fn a_late_bound_member_gets_a_symbol_in_no_table() {
    // Upstream's `__computed`: the name is whatever the expression evaluates to,
    // so no table can hold it — but the declaration still needs a symbol, and
    // the checker needs somewhere to attach the resolved name.
    let arena = Arena::new();
    let bound = bind_as(&arena, "declare const k: string;\nclass C { [k]: number; }\n", "a.ts");
    let class = bound.top_level_symbol("C").expect("the class");
    assert!(
        bound.result.symbols().get(class).members.is_empty(),
        "a late-bound member is in no symbol table"
    );
    let (id, computed) = bound
        .result
        .symbols()
        .iter()
        .find(|(_, symbol)| symbol.name == "__computed")
        .expect("the member still gets a symbol");
    assert_eq!(computed.parent, Some(class), "parented to the class it was written in");
    assert!(computed.flags.contains(SymbolFlags::PROPERTY));
    // The expression is reachable from the declaration, which is what late
    // binding will need.
    let declaration = computed.declarations[0];
    assert!(bound.result.computed_name(declaration).is_some());
    let _ = id;
}

#[test]
fn two_late_bound_members_are_two_symbols() {
    // They share the name `__computed`, and merging them would be wrong: which
    // of them — if either — collides is the checker's answer, not the binder's.
    let arena = Arena::new();
    let bound = bind_as(
        &arena,
        "declare const j: string;\ndeclare const k: string;\nclass C { [j]: number; [k]: string; }\n",
        "a.ts",
    );
    let count =
        bound.result.symbols().iter().filter(|(_, symbol)| symbol.name == "__computed").count();
    assert_eq!(count, 2);
    assert!(bound.result.diagnostics().is_empty(), "and no duplicate-identifier error");
}

#[test]
fn a_statically_computed_name_still_declares_statically() {
    // `['a']` is computed in syntax only: the expression is already the value.
    let arena = Arena::new();
    let bound = bind_as(&arena, "class C { ['a']: number; }\n", "a.ts");
    let class = bound.top_level_symbol("C").expect("the class");
    assert!(bound.result.symbols().get(class).members.contains_key("a"));
    // The written form is recorded even so, because upstream prints it back.
    let member = bound.result.symbols().get(class).members["a"];
    let declaration = bound.result.symbols().get(member).declarations[0];
    assert!(bound.result.computed_name(declaration).is_some());
}

#[test]
fn a_late_bound_object_literal_member_is_parented_to_the_literal() {
    let arena = Arena::new();
    let bound = bind_as(&arena, "declare function f(): string;\nconst o = { [f()]: 1 };\n", "a.ts");
    let literal = bound
        .result
        .symbols()
        .iter()
        .find(|(_, symbol)| symbol.name == "__object")
        .map(|(id, _)| id)
        .expect("the object literal");
    let computed = bound
        .result
        .symbols()
        .iter()
        .find(|(_, symbol)| symbol.name == "__computed")
        .expect("the member");
    assert_eq!(computed.1.parent, Some(literal));
}

// ---- deep nesting (bd tsr-el3.1, ADR-0030) -------------------------------
//
// The parser caps its own recursion at 192 but builds deeper trees than that
// iteratively — `a + a + a …` is precedence-climbed, so `descend()` never fires
// and the tree is as deep as the chain. The binder walks that tree recursively,
// which overflowed a fixed stack until `bind()` was wrapped in
// `tsr_core::stack::ensure_sufficient`. These pin that it no longer does.
//
// Each of these aborts the *process* with "has overflowed its stack" if the
// growth is removed — they are not assertions that merely go red.

fn bind_source<'a>(arena: &'a Arena, source: &'a str) -> BindResult<'a> {
    let parsed = tsr_parser::parse(arena, source);
    tsr_binder::bind(
        arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    )
}

#[test]
fn a_binary_chain_far_deeper_than_the_stack_binds_without_overflowing() {
    // `compiler/binderBinaryExpressionStress` is 4,958 operands of exactly this
    // shape, so this is a real corpus case rather than a hypothetical. 10,000 is
    // deliberately past it: upstream has no ceiling and neither should this.
    let arena = Arena::new();
    let source = arena.alloc_str(&format!("const x = {};\n", "a + ".repeat(10_000) + "a"));
    let result = bind_source(&arena, source);
    assert!(result.max_depth() > 10_000, "the walk really did recurse that deep");
    assert!(result.diagnostics().is_empty(), "deep nesting is not an error natively");
}

#[test]
fn deeply_nested_parentheses_bind_without_overflowing() {
    // A different shape reaching the same walk, so the fix is not accidentally
    // specific to binary expressions. The parser truncates past its own
    // MAX_DEPTH of 192, so the tree here is shallower than the input — the point
    // is that nothing aborts.
    let arena = Arena::new();
    let source =
        arena.alloc_str(&format!("const x = {}a{};\n", "(".repeat(5_000), ")".repeat(5_000)));
    let result = bind_source(&arena, source);
    assert!(result.max_depth() > 0);
}

#[test]
fn declarations_around_a_deep_expression_still_bind() {
    // Growing the stack must not disturb the walk itself.
    let arena = Arena::new();
    let source = arena.alloc_str(&format!(
        "const before = 1;\nconst deep = {};\nconst after = 2;\n",
        "a + ".repeat(10_000) + "a"
    ));
    let bound = bind(&arena, source);
    for name in ["before", "deep", "after"] {
        assert!(bound.top_level_symbol(name).is_some(), "`{name}` should be declared");
    }
}

// ---------------------------------------------------------------------------
// `BindResult::node` — the way back from an id to the typed node.
//
// ADR-0033 (superseding ADR-0032). The checker cannot compute a declaration's
// type without this: a symbol holds a `NodeId`, and the annotation and
// initialiser live in the node. The map is filled by the parser, so these are
// binder tests only in that they start from a symbol.
// ---------------------------------------------------------------------------

/// Every node reachable from the root, by a `push_children` walk.
fn every_node(root: Node<'_>) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    let mut children = Vec::new();
    while let Some(node) = stack.pop() {
        out.push(node);
        children.clear();
        tsr_ast::push_children(node, &mut children);
        stack.extend(children.iter().copied());
    }
    out
}

#[test]
fn every_node_in_the_tree_can_be_reached_from_its_id() {
    let arena = Arena::new();
    let bound = bind(
        &arena,
        "const a: string = 'x';\n\
         function f(p: number) { return p; }\n\
         class C { m: boolean = true; }\n\
         for (const q of [1]) { q; }\n",
    );
    let root = Node::SourceFile(bound.parsed.source_file);
    let mut kinds = std::collections::HashSet::new();
    for node in every_node(root) {
        let Some(id) = node.node_id() else { continue };
        let found = bound.parsed.node_map.get(id).unwrap_or_else(|| {
            panic!("no node for {:?} at {:?}", bound.nodes().kind(id), bound.nodes().span(id))
        });
        // The right node, not merely *a* node: the same id comes back out.
        assert_eq!(found.node_id(), Some(id));
        kinds.insert(bound.nodes().kind(id));
    }
    // Naming the kinds rather than counting nodes: "saw more than N" passes for
    // the wrong N the moment the fixture or the parser changes shape, and says
    // nothing about which constructs were actually covered.
    for required in [
        tsr_ast::SyntaxKind::VariableDeclaration,
        tsr_ast::SyntaxKind::FunctionDeclaration,
        tsr_ast::SyntaxKind::ClassDeclaration,
        tsr_ast::SyntaxKind::ForOfStatement,
        tsr_ast::SyntaxKind::Parameter,
    ] {
        assert!(kinds.contains(&required), "the fixture must cover {required:?}");
    }
}

#[test]
fn a_declarations_annotation_and_initialiser_are_reachable_from_its_symbol() {
    // The query `getTypeOfSymbol` actually makes, end to end: symbol -> its
    // value declaration id -> the typed node -> the fields the checker needs.
    // Without `BindResult::node` this is exactly where the checker stops.
    let arena = Arena::new();
    let bound = bind(&arena, "const a: string = 'x';");
    let symbol_id = bound.result.lookup_local(bound.root(), "a").expect("`a` is declared");
    let symbol = bound.result.symbols().get(symbol_id);
    let declaration = symbol.value_declaration.expect("a `const` has a value declaration");

    let Some(Node::VariableDeclaration(node)) = bound.parsed.node_map.get(declaration) else {
        panic!("expected a VariableDeclaration, got {:?}", bound.parsed.node_map.get(declaration));
    };
    assert!(node.r#type.is_some(), "the annotation must be reachable");
    assert!(node.initializer.is_some(), "the initialiser must be reachable");
}

#[test]
fn a_case_keyword_is_in_the_map_even_though_the_bind_walk_never_visits_it() {
    // Recording during the *bind* walk left 1,728 of 419,565 nodes unreachable
    // across the benchmark fixtures — `case` and `default` keywords among them —
    // because `push_children` is generated from `ast.json` and includes
    // token-valued fields while `bind_children` ports upstream's `bindChildren`,
    // whose `ForEachChild` does not visit them. Filling the map in the parser
    // makes the question moot: every node is recorded where it is created, so
    // no walk's idea of "every node" has to be trusted. This test is kept
    // because it is the one that caught the hole.
    let arena = Arena::new();
    let bound = bind(&arena, "switch (1) { case 2: break; default: break; }");
    let root = Node::SourceFile(bound.parsed.source_file);

    let mut keywords = 0;
    for node in every_node(root) {
        let Some(id) = node.node_id() else { continue };
        let kind = bound.nodes().kind(id);
        if matches!(kind, tsr_ast::SyntaxKind::CaseKeyword | tsr_ast::SyntaxKind::DefaultKeyword) {
            keywords += 1;
            assert!(
                bound.parsed.node_map.get(id).is_some(),
                "{kind:?} at {:?} is in the tree but not in the table",
                bound.nodes().span(id)
            );
        }
    }
    assert_eq!(keywords, 2, "the fixture must actually contain a `case` and a `default`");
}

#[test]
fn an_id_from_beyond_the_tree_yields_none_rather_than_panicking() {
    // A row registered by an abandoned speculative parse is not in the tree
    // (`bd tsr-pum.12`). The checker must see `None` and fall back to an error
    // type, not crash — and an out-of-range id must not panic either.
    let arena = Arena::new();
    let bound = bind(&arena, "const a = 1;");
    let past_the_end = tsr_ast::NodeId::new(u32::try_from(bound.nodes().len()).expect("fits") + 5);
    assert!(bound.parsed.node_map.get(past_the_end).is_none());
}

// ---------------------------------------------------------------------------
// Type parameters of a class or an interface.
//
// `bd tsr-y4u.21` reported these as "bound nowhere", on the strength of a
// `lookup_local` sweep. They are bound — in the **members** table of the class or
// interface symbol, which is where upstream puts them
// (`declareSymbolAndAddToSymbolTable` → `declareClassMember`,
// `internal/binder/binder.go:429-441`, and see the comment quoted in
// `BindResult::resolve_name`). What was missing is the *resolver* arm that reads
// that table, so every one of these asserts through `resolve_name`.
//
// Each test contrasts `resolve_name` against `resolve`, the meaning-less
// locals-only walk, so that it is visible which table produced the answer.
// ---------------------------------------------------------------------------

/// The `NodeId` of the identifier starting at byte `offset`.
///
/// Resolution starts at a *reference*, so the tests need one; a symbol's
/// declaration would not exercise the walk.
fn identifier_at(bound: &Bound<'_>, offset: usize) -> tsr_ast::NodeId {
    let root = Node::SourceFile(bound.parsed.source_file);
    let offset = u32::try_from(offset).expect("fixture fits in u32");
    every_node(root)
        .into_iter()
        .filter_map(|node| node.node_id())
        .find(|&id| {
            bound.nodes().kind(id) == tsr_ast::SyntaxKind::Identifier
                && bound.nodes().span(id).start == offset
        })
        .unwrap_or_else(|| panic!("no identifier starts at {offset}"))
}

/// `resolve_name` with the meaning upstream passes for a type reference.
fn resolve_type(bound: &Bound<'_>, from: tsr_ast::NodeId, name: &str) -> Option<SymbolId> {
    bound.result.resolve_name(bound.nodes(), &bound.parsed.node_map, from, name, SymbolFlags::TYPE)
}

/// `resolve_name` with the meaning upstream passes for an identifier expression.
fn resolve_value(bound: &Bound<'_>, from: tsr_ast::NodeId, name: &str) -> Option<SymbolId> {
    bound.result.resolve_name(bound.nodes(), &bound.parsed.node_map, from, name, SymbolFlags::VALUE)
}

/// Whether `symbol` is a type parameter declared directly by `container`.
fn is_type_parameter_of(bound: &Bound<'_>, symbol: SymbolId, container: tsr_ast::NodeId) -> bool {
    let symbol = bound.result.symbols().get(symbol);
    symbol.flags.contains(SymbolFlags::TYPE_PARAMETER)
        && symbol
            .declarations
            .iter()
            .any(|&declaration| bound.nodes().parent(declaration) == Some(container))
}

#[test]
fn a_class_type_parameter_resolves_from_a_member_annotation() {
    let arena = Arena::new();
    let source = "class C<T> { p: T; }";
    let bound = bind(&arena, source);
    let class = Node::from(bound.parsed.source_file.statements[0]).node_id().expect("registered");
    let reference = identifier_at(&bound, source.rfind('T').expect("the annotation"));

    let found = resolve_type(&bound, reference, "T").expect("`T` is in scope inside its class");
    assert!(
        is_type_parameter_of(&bound, found, class),
        "the answer must be the class's own type parameter, not something of the same name"
    );
    // No `locals` table anywhere on the walk holds `T` — which is the whole
    // reason the members arm exists, and why `bd tsr-y4u.21` read this as "bound
    // nowhere". In value meaning the arm still *runs*: `VALUE & TYPE` is
    // `CLASS | ENUM | ENUM_MEMBER`, not empty. It is the filter, not the arm,
    // that excludes a type parameter — which is upstream's arithmetic exactly.
    assert!(resolve_value(&bound, reference, "T").is_none());
}

#[test]
fn an_interface_type_parameter_resolves_from_a_member_annotation() {
    let arena = Arena::new();
    let source = "interface I<T> { p: T }";
    let bound = bind(&arena, source);
    let interface =
        Node::from(bound.parsed.source_file.statements[0]).node_id().expect("registered");
    let reference = identifier_at(&bound, source.rfind('T').expect("the annotation"));

    let found = resolve_type(&bound, reference, "T").expect("`T` is in scope inside its interface");
    assert!(is_type_parameter_of(&bound, found, interface));
    assert!(resolve_value(&bound, reference, "T").is_none());
}

#[test]
fn a_class_expression_type_parameter_resolves_too() {
    // A class expression is `bindAnonymousDeclaration`'d — in no symbol table at
    // all (ADR-0026) — so its members table is reachable only through the node's
    // own symbol. Upstream lists `KindClassExpression` in the same arm.
    let arena = Arena::new();
    let source = "const E = class<T> { p: T; };";
    let bound = bind(&arena, source);
    let reference = identifier_at(&bound, source.rfind('T').expect("the annotation"));

    assert!(resolve_type(&bound, reference, "T").is_some());
}

#[test]
fn a_value_reference_does_not_find_a_type_parameter() {
    // The members table of a class holds its properties and methods as well, so
    // the lookup is filtered to `meaning & SymbolFlagsType` (`nameresolver.go:174`
    // with the default `lookup`, `:418`). Without the filter a value reference
    // would resolve to a type parameter — an answer upstream never gives.
    let arena = Arena::new();
    let source = "class C<T> { m() { return T; } }";
    let bound = bind(&arena, source);
    let reference = identifier_at(&bound, source.rfind('T').expect("the `return T`"));

    assert!(
        bound
            .result
            .resolve_name(bound.nodes(), &bound.parsed.node_map, reference, "T", SymbolFlags::VALUE)
            .is_none(),
        "`T` is a type, and this is a value position"
    );
    // The same reference in type meaning does resolve, so the test is about the
    // meaning and not about the reference being unreachable.
    assert!(resolve_type(&bound, reference, "T").is_some());
}

#[test]
fn a_static_member_cannot_reference_the_class_type_parameter() {
    // TypeScript 1.0 spec (April 2014) §3.4.1: the scope of a type parameter is
    // the whole declaration **except** static members. Upstream reports
    // `Static_members_cannot_reference_class_type_parameters` and returns nil.
    let arena = Arena::new();
    let source = "class C<T> { static s: T; i: T; }";
    let bound = bind(&arena, source);
    let statik = identifier_at(&bound, source.find("T;").expect("the static annotation"));
    let instance = identifier_at(&bound, source.rfind('T').expect("the instance annotation"));

    assert!(resolve_type(&bound, statik, "T").is_none(), "a static member is outside the scope");
    assert!(resolve_type(&bound, instance, "T").is_some(), "an instance member is inside it");
}

#[test]
fn a_merged_interfaces_type_parameter_is_not_visible_in_the_class() {
    // `class C` and `interface C` merge into one symbol and therefore share one
    // members table, so the interface's `T` is reachable from the class's node.
    // `isTypeParameterSymbolDeclaredInContainer` (`nameresolver.go:477`) is what
    // stops it being answered there.
    let arena = Arena::new();
    let source = "class C { q: T; }\ninterface C<T> { p: T }";
    let bound = bind(&arena, source);
    let in_class = identifier_at(&bound, source.find("T;").expect("the class annotation"));
    let in_interface = identifier_at(&bound, source.rfind('T').expect("the interface annotation"));

    assert!(
        resolve_type(&bound, in_class, "T").is_none(),
        "the class declares no type parameter, and the merged table is not its own"
    );
    assert!(
        resolve_type(&bound, in_interface, "T").is_some(),
        "the interface that declared it can still see it"
    );
}

#[test]
fn a_methods_own_type_parameter_shadows_the_classs() {
    // The nearest declaring container wins: the walk reaches the method — whose
    // `locals` hold its own `T` — before it reaches the class. Both symbols are
    // named `T` and only the declaration's parent tells them apart, which is why
    // this asserts through the parent and not through the name.
    //
    // Note what this does *not* test. Upstream reads `location.Locals()` before
    // the class arm within one location, and so does `resolve_inner`, but that
    // ordering is unobservable here: a class is `IsContainer` without
    // `HasLocals` (`container.rs:53`), so no node ever has both tables. Swapping
    // the two turns nothing red. It is kept in upstream's order because it is
    // upstream's order and it starts to matter if a class ever gains locals.
    let arena = Arena::new();
    let source = "class C<T> { m<T>(p: T): void {} }";
    let bound = bind(&arena, source);
    let class = Node::from(bound.parsed.source_file.statements[0]).node_id().expect("registered");
    let reference = identifier_at(&bound, source.rfind('T').expect("the parameter annotation"));

    let found = resolve_type(&bound, reference, "T").expect("`T` resolves");
    assert!(
        !is_type_parameter_of(&bound, found, class),
        "the method's own type parameter must win over the class's"
    );
    let method = bound
        .nodes()
        .parent(bound.result.symbols().get(found).declarations[0])
        .expect("the type parameter has a parent");
    assert_eq!(bound.nodes().kind(method), tsr_ast::SyntaxKind::MethodDeclaration);
}

/// The symbol a signature-bearing type node gets, and what it can answer.
///
/// Ported from `bindFunctionOrConstructorType` (`binder.go:985`), reduced — see
/// `docs/architecture/binder.md`. Without this the checker's function-type arm
/// answers `errorType` for every one of the corpus's 9,676 function-type lines:
/// a dispatch that looks correct and measures zero.
#[test]
fn a_function_or_constructor_type_node_gets_an_anonymous_type_symbol() {
    let arena = Arena::new();
    for (source, kind) in [
        ("declare const f: (x: number) => void;", tsr_ast::SyntaxKind::FunctionType),
        ("declare const c: new (x: number) => void;", tsr_ast::SyntaxKind::ConstructorType),
    ] {
        let source = arena.alloc_str(source);
        let parsed = tsr_parser::parse(&arena, source);
        let result = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: source },
        );
        let mut found = None;
        for index in 0..parsed.nodes.len() {
            #[allow(clippy::cast_possible_truncation)]
            let id = tsr_ast::NodeId::new(index as u32);
            if parsed.nodes.kind(id) == kind {
                found = result.symbol_of(id);
                break;
            }
        }
        let symbol = found.unwrap_or_else(|| panic!("{source} binds no symbol for its type node"));
        let data = result.symbols().get(symbol);
        // `__type`, not `__call`: upstream binds both to this node and
        // `addDeclarationToSymbol` runs second, so `node.Symbol` is this one.
        assert_eq!(data.name, "__type", "{source}");
        assert!(data.flags.contains(SymbolFlags::TYPE_LITERAL), "{source}");
        // The property the checker actually needs: `getSignaturesOfSymbol` reads
        // `declarations`, so the signature is reachable even though `members` is
        // empty for want of the `__call` symbol.
        assert_eq!(data.declarations.len(), 1, "{source}");
        assert!(data.members.is_empty(), "the `__call` member is the documented gap: {source}");
    }
}

#[test]
fn a_numeric_member_binds_under_its_canonical_value() {
    // Upstream's scanner canonicalises every numeric token value
    // (`scanner.go:2194`, `jsnum.FromString(…).String()`), so `0b11`, `3` and
    // `3.0` all name ONE member and merge (`GetPropertyNameForPropertyNameNode`
    // reads that value, `ast/utilities.go:3160`). The node keeps the source
    // spelling for the printer; the symbol's name is the value's.
    let arena = Arena::new();
    let bound = bind(&arena, "class C { 0b11: number; }\nenum E { 0xF00D }\nconst o = { 1.0: 1 };");

    let class = bound.result.lookup_local(bound.root(), "C").expect("class C");
    let members = &bound.result.symbols().get(class).members;
    assert!(members.contains_key("3"), "0b11 binds as its value");
    assert!(!members.contains_key("0b11"), "the source spelling is not the name");

    let r#enum = bound.result.lookup_local(bound.root(), "E").expect("enum E");
    assert!(bound.result.symbols().get(r#enum).exports.contains_key("61453"));

    // A signed computed name is upstream's third static form
    // (`ast/utilities.go:3170`): `[-1]` declares `-1`.
    let arena2 = Arena::new();
    let bound2 = bind(&arena2, "class D { [-1] = 1; }");
    let class2 = bound2.result.lookup_local(bound2.root(), "D").expect("class D");
    assert!(bound2.result.symbols().get(class2).members.contains_key("-1"));
}

/// §196. A declaration that CONFLICTS gets its own symbol, not a merge.
///
/// `declareSymbolEx` (`binder.go:286`) ends its conflict branch with
/// `symbol = b.newSymbol(SymbolFlagsNone, name)` — a fresh symbol that is
/// deliberately *not* put in the symbol table. The table keeps the first
/// declaration's symbol with its original flags; the conflicting declaration
/// gets a private one carrying only itself.
///
/// This port merged instead, under a comment claiming upstream did the same.
/// The `.symbols` baseline says otherwise: `compiler/varAndFunctionShareName`
/// is `var myFn;` then `function myFn(): any {}` and records **two** symbols,
/// one declaration each — which is also why the two names print `any` and
/// `() => any` in `.types`, an answer one merged symbol cannot give.
#[test]
fn a_conflicting_declaration_does_not_merge_into_the_existing_symbol() {
    let arena = Arena::new();
    let bound = bind(&arena, "var myFn;\nfunction myFn(): any { }");

    // The table's symbol carries ONE declaration's flags, not both ORed
    // together. Which one is a separate question — see the note below.
    let flags = bound.top_level("myFn").expect("myFn is a top-level local");
    assert!(
        !(flags.intersects(SymbolFlags::FUNCTION)
            && flags.intersects(SymbolFlags::FUNCTION_SCOPED_VARIABLE)),
        "a conflicting pair must not have merged into one symbol: {flags:?}"
    );

    // And that symbol carries only its own declaration.
    let id = bound.result.lookup_local(bound.root(), "myFn").expect("myFn");
    assert_eq!(
        bound.result.symbols().get(id).declarations.len(),
        1,
        "a conflicting declaration must not be added to the existing symbol"
    );
}

/// The control, and the thing that makes the test above about *conflict* rather
/// than about declaration counting: declarations that legitimately merge still
/// share one symbol. `var x; var x;` is legal, and an interface merges with a
/// namespace.
#[test]
fn compatible_declarations_still_merge_into_one_symbol() {
    let arena = Arena::new();
    let bound =
        bind(&arena, "var x = 1;\nvar x = 2;\ninterface I {}\nnamespace I { export var y = 1; }");

    let x = bound.result.lookup_local(bound.root(), "x").expect("x");
    assert_eq!(
        bound.result.symbols().get(x).declarations.len(),
        2,
        "two `var`s of the same name are one symbol with two declarations"
    );

    let i = bound.top_level("I").expect("I");
    assert!(
        i.contains(SymbolFlags::INTERFACE) && i.intersects(SymbolFlags::MODULE),
        "an interface and a namespace merge: {i:?}"
    );
}
