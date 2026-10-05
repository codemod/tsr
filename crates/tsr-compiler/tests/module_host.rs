//! The seam between a `Program` and a `Checker`.
//!
//! `tsr_checker::resolution::ModuleHost` is declared in `tsr-checker` and
//! satisfied here, which is the direction upstream uses — `internal/compiler`
//! imports `internal/checker`, and the checker declares its own `Program`
//! interface (`internal/checker/checker.go:547`) rather than naming the
//! concrete type. See
//! [ADR-0041](../../../docs/adr/0041-the-checker-asks-its-program-for-a-module.md).
//!
//! These live in an integration test rather than in `src/lib.rs` because they
//! need both crates at once: the trait belongs to `tsr-checker`, the impl to
//! `tsr-compiler`, and the point of each test is that the two meet.

use tsr_ast::NodeId;
use tsr_checker::{Checker, resolution::ModuleHost};
use tsr_compiler::{LoadOptions, Program, ProgramFile, ProgramOptions};

struct Host {
    fs: tsr_vfs::InMemoryFileSystem,
}

impl tsr_module::types::ResolutionHost for Host {
    fn fs(&self) -> &dyn tsr_vfs::FileSystem {
        &self.fs
    }

    fn current_directory(&self) -> &'static str {
        "/"
    }
}

fn host(files: &[(&str, &str)]) -> Host {
    Host {
        fs: tsr_vfs::InMemoryFileSystem::new(
            files.iter().map(|(name, text)| ((*name).to_string(), (*text).to_string())),
            [],
            true,
        ),
    }
}

fn file_id(program: &Program<'_>, name: &str) -> NodeId {
    program
        .source_file(name)
        .expect("the file is in the program")
        .source_file()
        .node_id
        .expect("a parsed file's SourceFile is registered")
}

/// `/a.ts` imports `./b`, `/b.ts` imports nothing.
const FIXTURE: [(&str, &str); 2] = [
    ("/a.ts", "import { b } from \"./b\";\nvar u = undefined;\nexport const a = b;\n"),
    ("/b.ts", "export const b = 1;\n"),
];

#[test]
fn a_program_answers_the_checkers_module_question_through_the_trait() {
    // Deliberately through `&dyn ModuleHost`, not through the inherent method:
    // what this pins is that the *trait object the checker will hold* reaches
    // the program's cache, which is the only thing the impl adds over
    // `Program::resolved_module`.
    let arena = tsr_core::Arena::new();
    let program = Program::from_root_files(
        &arena,
        &host(&FIXTURE),
        LoadOptions { root_file_names: vec!["/a.ts".to_string()], ..Default::default() },
    );
    let a = file_id(&program, "/a.ts");
    let b = file_id(&program, "/b.ts");
    let module_host: &dyn ModuleHost = &program;

    assert_eq!(module_host.resolved_module(a, "./b"), Some(b));
    assert_eq!(
        module_host.resolved_module(a, "./unwritten"),
        None,
        "a specifier the file never wrote answers nothing"
    );
    // Pinned by construction: `/b.ts`'s text contains no import at all, so no
    // code path can give it an entry. A host that answered per *program* rather
    // than per *importing file* would answer `Some(b)` here and leave every
    // other assertion in this file intact.
    assert_eq!(
        module_host.resolved_module(b, "./b"),
        None,
        "`/b.ts` never imported `./b`; only `/a.ts` did"
    );
}

#[test]
fn a_checker_with_no_host_computes_exactly_what_it_computed_before() {
    // The property the whole constructor decision is judged on. `Checker::new`
    // is `Checker::with_module_host(.., None)` and there is one body, so this
    // is true by construction rather than by 84 hand-edited call sites — and
    // asserting it is what turns "by construction" into something that can
    // fail.
    //
    // The fixture carries `var u = undefined;` on purpose: seeding
    // `symbol_types` with the synthesised `undefined` global is the one piece
    // of work `Checker::new` does beyond field initialisation, so it is the
    // part a divergence between the two bodies would land on.
    let arena = tsr_core::Arena::new();
    let program = Program::from_root_files(
        &arena,
        &host(&FIXTURE),
        LoadOptions { root_file_names: vec!["/a.ts".to_string()], ..Default::default() },
    );

    let types = |checker: &mut Checker<'_, '_>| -> Vec<String> {
        let mut out: Vec<String> = program
            .binder()
            .symbols()
            .iter()
            .map(|(id, symbol)| {
                let type_id = checker.get_type_of_symbol(id);
                format!("{}: {}", symbol.name, checker.type_to_string(type_id))
            })
            .collect();
        out.sort();
        out
    };

    // The direct evidence that `new` supplies no host — the property every one
    // of the 84 existing call sites depends on, asserted rather than argued.
    let plain = Checker::new(program.binder(), program.nodes(), program.node_map());
    assert!(plain.module_host().is_none(), "`Checker::new` gives the checker no program");
    let hosted = Checker::with_module_host(
        program.binder(),
        program.nodes(),
        program.node_map(),
        Some(&program),
    );
    assert!(hosted.module_host().is_some(), "…and `with_module_host` does");
    drop((plain, hosted));

    let mut old_way = Checker::new(program.binder(), program.nodes(), program.node_map());
    let mut new_way =
        Checker::with_module_host(program.binder(), program.nodes(), program.node_map(), None);

    let baseline = types(&mut old_way);
    // A positive control. Without it this passes just as well when both sides
    // answer nothing at all, which is the shape every "identical output" test
    // fails in.
    assert!(
        baseline.iter().any(|line| line.starts_with("u: ")),
        "the fixture's `var u = undefined` must reach the checker"
    );
    assert_eq!(
        types(&mut new_way),
        baseline,
        "`new` is `with_module_host(.., None)`; there is one body and it must stay one"
    );

    // **Not** an assertion that a host changes nothing. It changes a great
    // deal — that is the point of the seam, and measured on this fixture the
    // moment the consuming arm landed: `a` and `b` in `/a.ts` go from `error`
    // to `1`. What must not change is a call site that has no host, which is
    // every existing one.
}

#[test]
fn a_program_from_a_file_list_satisfies_the_trait_and_answers_nothing() {
    // Same source text and same file set as `FIXTURE`; only the constructor
    // differs. `Program::new` ran no resolver, so the answer here is fixed
    // before any of this code runs — and it is a gap rather than an attempt to
    // re-derive the resolution by matching `"./b"` against the file list.
    let arena = tsr_core::Arena::new();
    let program = Program::in_arena(
        &arena,
        ProgramOptions {
            files: FIXTURE
                .iter()
                .map(|(name, text)| ((*name).to_string(), (*text).to_string()))
                .collect(),
            ..Default::default()
        },
    );
    let module_host: &dyn ModuleHost = &program;
    assert_eq!(program.source_files().len(), 2, "both files are in the program");
    assert_eq!(module_host.resolved_module(file_id(&program, "/a.ts"), "./b"), None);
}

/// The file names a program built from roots discovered, for the record: `./b`
/// was never named, only imported.
#[test]
fn the_fixture_discovers_the_imported_file() {
    let arena = tsr_core::Arena::new();
    let program = Program::from_root_files(
        &arena,
        &host(&FIXTURE),
        LoadOptions { root_file_names: vec!["/a.ts".to_string()], ..Default::default() },
    );
    let mut names: Vec<&str> = program.source_files().iter().map(ProgramFile::file_name).collect();
    names.sort_unstable();
    assert_eq!(names, ["/a.ts", "/b.ts"]);
}

#[test]
fn source_text_is_borrowed_from_the_exact_file_and_node_table() {
    let arena = tsr_core::Arena::new();
    let program = Program::in_arena(
        &arena,
        ProgramOptions {
            files: vec![
                ("/index.ts".into(), "var left: Shape = {};".into()),
                ("/branch/index.ts".into(), "var right: Shape = {};".into()),
            ],
            ..Default::default()
        },
    );
    let foreign_arena = tsr_core::Arena::new();
    let foreign = Program::in_arena(
        &foreign_arena,
        ProgramOptions {
            files: vec![("/index.ts".into(), "var alien: Other = {};".into())],
            ..Default::default()
        },
    );
    let left = file_id(&program, "/index.ts");
    let right = file_id(&program, "/branch/index.ts");
    assert_eq!(left, file_id(&foreign, "/index.ts"), "raw ids deliberately collide");
    let module_host: &dyn ModuleHost = &program;
    let text = module_host.source_text(left, program.nodes()).expect("original source");
    assert_eq!(text, "var left: Shape = {};");
    assert!(std::ptr::eq(text, program.source_file("/index.ts").unwrap().text()));
    assert_eq!(
        module_host.source_text(right, program.nodes()),
        Some("var right: Shape = {};"),
        "same basenames and source positions are different owners"
    );
    assert_eq!(module_host.source_text(left, foreign.nodes()), None);
    let foreign_host: &dyn ModuleHost = &foreign;
    assert_eq!(foreign_host.source_text(left, program.nodes()), None);
    assert_eq!(module_host.source_text(NodeId::new(0), program.nodes()), None);
    assert_eq!(module_host.source_text(NodeId::new(1_000_000), program.nodes()), None);
}

#[test]
fn absent_and_legacy_hosts_do_not_supply_source_text() {
    struct LegacyHost;

    impl ModuleHost for LegacyHost {
        fn resolved_module(&self, _file: NodeId, _specifier: &str) -> Option<NodeId> {
            None
        }

        fn module_resolution_found(&self, _file: NodeId, _specifier: &str) -> bool {
            false
        }
    }

    let arena = tsr_core::Arena::new();
    let program = Program::in_arena(
        &arena,
        ProgramOptions {
            files: vec![("/index.ts".into(), "const value = 37;".into())],
            ..Default::default()
        },
    );
    let file = file_id(&program, "/index.ts");
    let legacy: &dyn ModuleHost = &LegacyHost;
    assert_eq!(legacy.source_text(file, program.nodes()), None);
    let checker = Checker::new(program.binder(), program.nodes(), program.node_map());
    assert_eq!(
        checker.module_host().and_then(|host| host.source_text(file, program.nodes())),
        None
    );
}
