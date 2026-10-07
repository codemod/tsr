//! Native getInstantiationExpressionType and getTypeAliasInstantiation, pinned 5b1047d.
use tsr_ast::{NodeId, NodeTable, Statement};
use tsr_checker::{Checker, resolution::ModuleHost};
use tsr_core::Arena;

struct Source<'a> {
    file: NodeId,
    nodes: &'a NodeTable,
    text: &'a str,
}

impl ModuleHost for Source<'_> {
    fn resolved_module(&self, _: NodeId, _: &str) -> Option<NodeId> { None }
    fn module_resolution_found(&self, _: NodeId, _: &str) -> bool { false }
    fn source_text(&self, file: NodeId, nodes: &NodeTable) -> Option<&str> {
        (file == self.file && std::ptr::eq(nodes, self.nodes)).then_some(self.text)
    }
}

fn with_checker(source: &str, inspect: impl FnOnce(&mut Checker<'_, '_>, &[Statement<'_>])) {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let bound = tsr_binder::bind(&arena, parsed.source_file, &parsed.nodes,
        tsr_binder::FileInfo { name: "control.ts", text: source });
    let host = Source { file: parsed.source_file.node_id.unwrap(), nodes: &parsed.nodes, text: source };
    let mut checker = Checker::with_module_host(&bound, &parsed.nodes, &parsed.node_map, Some(&host));
    inspect(&mut checker, parsed.source_file.statements);
}

fn initializer_types(checker: &mut Checker<'_, '_>, statements: &[Statement<'_>]) -> Vec<(String, String)> {
    let mut types = Vec::new();
    for statement in statements {
        let Statement::VariableStatement(statement) = statement else { continue };
        for declaration in statement.declaration_list.unwrap().declarations {
            let Some(tsr_ast::BindingName::Identifier(name)) = declaration.name else { continue };
            if let Some(expression) = declaration.initializer {
                let ty = checker.check_expression(expression);
                types.push((name.text.to_owned(), checker.type_to_string(ty)));
            }
        }
    }
    types
}

#[test]
fn captured_factory_alias_arguments_remain_distinct_after_warm_reads() {
    let source = "declare function identity<U>(value: U): U; type Factory<T> = typeof identity<T>; declare const stringFactory: Factory<string>; declare const numberFactory: Factory<number>; const first = stringFactory('text'); const second = numberFactory(1); const third = stringFactory('again');";
    with_checker(source, |checker, statements| {
        let expected = [("first".to_owned(), "string".to_owned()),
            ("second".to_owned(), "number".to_owned()), ("third".to_owned(), "string".to_owned())];
        assert_eq!(initializer_types(checker, statements), expected);
        assert_eq!(initializer_types(checker, statements), expected);
    });
}

#[test]
fn enclosing_aliases_keep_distinct_names_and_semantic_argument_maps() {
    let source = "declare function identity<U>(value: U): U; type Fn<U> = typeof identity<U>; type Fixed<T> = Fn<number>; type AliasOne = Fn<string>; type AliasTwo = Fn<string>; declare const fixed: Fixed<string>; declare const one: AliasOne; declare const two: AliasTwo; const sameFirst = one; const sameSecond = two; const fixedResult = fixed(1); const first = one('first'); const second = two('second'); const again = one('again');";
    with_checker(source, |checker, statements| {
        let expected = [
            ("sameFirst".to_owned(), "AliasOne".to_owned()),
            ("sameSecond".to_owned(), "AliasTwo".to_owned()),
            ("fixedResult".to_owned(), "number".to_owned()),
            ("first".to_owned(), "string".to_owned()),
            ("second".to_owned(), "string".to_owned()),
            ("again".to_owned(), "string".to_owned()),
        ];
        assert_eq!(initializer_types(checker, statements), expected);
        assert_eq!(initializer_types(checker, statements), expected);
    });
}

#[test]
fn filtered_view_retains_readonly_members_and_drops_inapplicable_intersection_signatures() {
    let source = "declare const callable: { <T>(value: T): T; readonly tag: 'callable'; }; const selected = callable<number>; const tag = selected.tag; declare const both: (<T>(value: T) => T) & ((value: string) => string); const surviving = both<number>; const result = surviving(1);";
    with_checker(source, |checker, statements| {
        assert_eq!(initializer_types(checker, statements), [
            ("selected".to_owned(), "{ (value: number): number; readonly tag: \"callable\"; }".to_owned()),
            ("tag".to_owned(), "\"callable\"".to_owned()),
            ("surviving".to_owned(), "(value: number) => number".to_owned()),
            ("result".to_owned(), "number".to_owned()),
        ]);
    });
}

#[test]
fn inapplicable_list_diagnostic_uses_native_trivia_skipped_list_span() {
    let source = "declare const plain: (value: number) => number; const selected = plain</* trivia */ string>;";
    with_checker(source, |checker, statements| {
        initializer_types(checker, statements);
        let diagnostics = checker.diagnostics();
        let [(_, diagnostic)] = diagnostics else { panic!("one applicability diagnostic") };
        assert_eq!(diagnostic.code(), "TS2635");
        assert_eq!(diagnostic.text(), "Type '(value: number) => number' has no signatures for which the type argument list is applicable.");
        let start = u32::try_from(source.find("string>").unwrap()).unwrap();
        assert_eq!(diagnostic.span, tsr_core::Span::new(start, start + 6));
    });
}
