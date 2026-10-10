//! r6-modules3's const-enum index diff (`docs/parity/notes/r6-modules3.md`
//! §2): `checkElementAccessExpression`'s const-enum arm (`checker.go:8157`)
//! reports TS2476 at an index that is not string-literal-like.

use tsr_checker::check::FileContext;
use tsr_compiler::{LoadOptions, Program};
use tsr_core::{CompilerOptions, ScriptTarget};

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

fn es2015() -> CompilerOptions {
    CompilerOptions { target: ScriptTarget::ES2015, ..Default::default() }
}

#[test]
fn a_const_enum_index_must_be_a_string_literal() {
    // Native reports TS2476 on lines 3, 4 and 6, wherever the access sits,
    // and accepts both string-literal-like spellings on line 5.
    let files = [(
        "/a.ts",
        "const enum E { A }\nlet key = \"A\";\nE[0];\nlet a: E = E[key];\nlet b = E[\"A\"] + E[`A`];\nfunction f() { return E[`${key}`]; }\n",
    )];
    assert_eq!(reported(&files, es2015(), "/a.ts"), [(3, 2476), (4, 2476), (6, 2476)]);
}
