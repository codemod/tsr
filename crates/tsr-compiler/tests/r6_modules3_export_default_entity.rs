//! r6-modules3's export-default entity diff (`docs/parity/notes/r6-modules3.md`
//! §1): `export default a.b` is the alias upstream makes, and the default
//! import names a merged `Property|Interface` target by its first named
//! declaration (`getNameOfSymbolAsWritten`).
#![allow(
    clippy::cast_possible_truncation,
    reason = "line/column arithmetic over small test sources"
)]

use tsr_checker::check::FileContext;
use tsr_compiler::{LoadOptions, Program};
use tsr_core::{CompilerOptions, ModuleKind, ScriptTarget};

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

fn commonjs() -> CompilerOptions {
    CompilerOptions {
        module: ModuleKind::CommonJS,
        target: ScriptTarget::ES2015,
        ..Default::default()
    }
}

#[test]
fn export_default_of_a_dotted_name_carries_both_meanings() {
    // `compiler/exportDefaultProperty2` plus two uses. Native reports both
    // TS2322s: `x: B` is the interface and `B` the static's `number`.
    let files = [
        (
            "/a.ts",
            "class C {\n    static B: number;\n}\nnamespace C {\n    export interface B { c: number }\n}\nexport default C.B;\n",
        ),
        (
            "/b.ts",
            "import B from \"./a\";\nconst x: B = { c: B };\nconst y: number = x;\nconst z: string = B;\n",
        ),
    ];
    assert_eq!(reported(&files, commonjs(), "/b.ts"), [(3, 2322), (4, 2322)]);
}
