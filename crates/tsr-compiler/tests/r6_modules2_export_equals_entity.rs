//! r6-modules2's export-equals entity diff (`docs/parity/notes/r6-modules2.md`
//! §5): `export = a.b` is an alias (`ExpressionIsAlias`) resolved by
//! `resolveEntityName`, `globalThis` included.
#![allow(
    clippy::cast_possible_truncation,
    reason = "line/column arithmetic over small test sources"
)]

use tsr_checker::check::FileContext;
use tsr_compiler::{LoadOptions, Program};
use tsr_core::{CompilerOptions, ModuleKind, ScriptTarget, Tristate};

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

/// The `(line, code)` pairs the checker reports in `file`, sorted.
fn reported(files: &[(&str, &str)], options: CompilerOptions, file: &str) -> Vec<(u32, u32)> {
    let host = Host {
        fs: tsr_vfs::InMemoryFileSystem::new(
            files.iter().map(|(name, text)| ((*name).to_string(), (*text).to_string())),
            [],
            true,
        ),
    };
    let arena = tsr_core::Arena::new();
    let program = Program::from_root_files(
        &arena,
        &host,
        LoadOptions {
            compiler_options: options,
            root_file_names: files.iter().map(|(name, _)| (*name).to_string()).collect(),
            ..Default::default()
        },
    );
    let text = files.iter().find(|(name, _)| *name == file).expect("listed").1;
    let id = program.source_file(file).expect("loaded").source_file().node_id.expect("id");
    let mut checker = tsr_checker::Checker::with_module_host(
        program.binder(),
        program.nodes(),
        program.node_map(),
        Some(&program),
    );
    checker.apply_compiler_options(program.compiler_options());
    checker.check_source_file(id, FileContext { ambient: false, has_parse_errors: false });
    let mut reported: Vec<(u32, u32)> = checker
        .diagnostics()
        .iter()
        .filter(|(at, _)| *at == id)
        .map(|(_, diagnostic)| {
            let line = text[..diagnostic.span.start as usize].matches('\n').count() as u32 + 1;
            (line, diagnostic.message.code())
        })
        .collect();
    reported.sort_unstable();
    reported
}

fn commonjs(verbatim_module_syntax: bool) -> CompilerOptions {
    CompilerOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2015,
        verbatim_module_syntax: Tristate::from_bool(verbatim_module_syntax),
        ..Default::default()
    }
}

/// `compiler/isolatedModulesShadowGlobalTypeNotValue`'s `node.d.ts` and
/// `good.ts`, reduced.
const GLOBAL_THIS_EXPORT: [(&str, &str); 2] = [
    (
        "/node.d.ts",
        "declare module 'node:console' {\n    global {\n        interface Console { Console: console.ConsoleConstructor; }\n        namespace console {\n            interface ConsoleConstructor { prototype: Console; new (): Console; }\n        }\n        var console: Console;\n    }\n    export = globalThis.console;\n}\n",
    ),
    ("/good.ts", "import { Console } from 'node:console';\nconst baz: Console = new Console();\n"),
];

#[test]
fn an_import_through_export_equals_global_this_is_an_es_import_in_commonjs() {
    // TS1295, as native reports; nothing without the option.
    assert_eq!(reported(&GLOBAL_THIS_EXPORT, commonjs(true), "/good.ts"), [(1, 1295)]);
    assert_eq!(reported(&GLOBAL_THIS_EXPORT, commonjs(false), "/good.ts"), []);
}

#[test]
fn export_equals_of_a_dotted_name_carries_both_meanings() {
    // `compiler/exportEqualsProperty2` plus a use: native reports TS2322,
    // so `x: B` is the interface and `{ c: B }` reads the static.
    let files = [
        (
            "/a.ts",
            "class C {\n    static B: number;\n}\nnamespace C {\n    export interface B { c: number }\n}\nexport = C.B;\n",
        ),
        ("/b.ts", "import B = require(\"./a\");\nconst x: B = { c: B };\nconst y: number = x;\n"),
    ];
    assert_eq!(reported(&files, commonjs(false), "/b.ts"), [(3, 2322)]);
}
