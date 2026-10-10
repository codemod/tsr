//! TS2540 on a mapped member (`isAssignmentToReadonlyEntity`,
//! `checker.go:27279`): a homomorphic mapped type's member is readonly when
//! its modifiers property is (resolveMappedTypeMembers, `checker.go:20894`),
//! through an alias instance too (`omitTypeHelperModifiers01`). Expectations
//! are the pinned tsgo's (`docs/parity/notes/r6-mapped.md` §4).

use tsr_ast::{NodeMap, NodeTable};
use tsr_checker::{Checker, check::FileContext};
use tsr_core::{Arena, CompilerOptions, ScriptTarget, Tristate};

/// The 1-based lines of every TS2540 in `source`, checked under `strict`.
fn ts2540_lines(source: &str) -> Vec<usize> {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let globals = "interface Array<T> { [n: number]: T; length: number } \
        interface ReadonlyArray<T> { readonly [n: number]: T; readonly length: number } \
        interface String { length: number; [n: number]: string } \
        interface Number { toFixed(): string } interface Boolean {} \
        interface Object {} interface Function {} interface RegExp {} \
        interface IArguments {} interface Symbol {} \
        type Partial<T> = { [P in keyof T]?: T[P] }; \
        type Required<T> = { [P in keyof T]-?: T[P] }; \
        type Readonly<T> = { readonly [P in keyof T]: T[P] }; \
        type Record<K extends keyof any, T> = { [P in K]: T };";
    let global = tsr_parser::parse_into(
        &arena,
        globals,
        tsr_parser::ParseOptions::for_file("global.d.ts"),
        &mut nodes,
        &mut map,
    );
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        global.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "global.d.ts", text: globals },
    );
    let parsed = tsr_parser::parse_into(
        &arena,
        source,
        tsr_parser::ParseOptions::for_file("a.ts"),
        &mut nodes,
        &mut map,
    );
    let root = parsed.source_file.node_id.expect("registered source file");
    let bound = tsr_binder::bind_into(
        bound,
        &arena,
        parsed.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "a.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &map);
    checker.apply_compiler_options(&CompilerOptions {
        strict: Tristate::from_bool(true),
        target: ScriptTarget::ES2015,
        ..CompilerOptions::default()
    });
    checker.check_source_file(
        root,
        FileContext { ambient: false, has_parse_errors: !parsed.diagnostics.is_empty() },
    );
    let mut lines: Vec<usize> = checker
        .diagnostics()
        .iter()
        .filter(|(file, diagnostic)| *file == root && diagnostic.message.code() == 2540)
        .map(|(_, diagnostic)| {
            source[..usize::try_from(diagnostic.span.start).unwrap()].matches('\n').count() + 1
        })
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines
}

/// The lines of `source` that carry a `// error` marker.
fn marked(source: &str) -> Vec<usize> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains("// error"))
        .map(|(index, _)| index + 1)
        .collect()
}

#[test]
fn mapped_members_keep_their_modifiers_property_readonly() {
    let source = r"type A = { a: number; readonly c: boolean };
interface I { a: number; readonly c: boolean }
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Exclude<T, U> = T extends U ? never : T;
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type MyPick<T, K extends keyof T> = { [P in K]: T[P] };
declare let p1: Pick<A, 'c'>;
p1.c = true; // error
declare let p2: Pick<I, 'c'>;
p2.c = true; // error
declare let p4: MyPick<A, 'c'>;
p4.c = true; // error
declare let p5: { [P in keyof A]: A[P] };
p5.c = true; // error
p5.a = 1;
declare let p6: { [P in 'c']: A[P] };
p6.c = true;
type B = Omit<A, 'a'>;
function f(x: B) { x.c = true; } // error
";
    assert_eq!(ts2540_lines(source), marked(source));
}
