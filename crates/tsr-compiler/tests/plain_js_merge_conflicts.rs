//! `reportMergeSymbolError`'s plain-JavaScript test (`checker.go:14215`),
//! over a real program: a cross-file redeclaration is suppressed only for a
//! side declared in a plain JS file (`ast.IsPlainJSFile`: no directive and
//! `checkJs` unset). `docs/parity/notes/r6-jsdoc.md` §7.

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

/// Every diagnostic the checker reports for the program's files, as
/// `(file, line, code)`, sorted.
fn diagnostics(files: &[(&str, &str)], check_js: Tristate) -> Vec<(String, usize, u32)> {
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
            compiler_options: CompilerOptions {
                module: ModuleKind::CommonJS,
                target: ScriptTarget::ES2015,
                allow_js: Tristate::True,
                check_js,
                ..Default::default()
            },
            root_file_names: files.iter().map(|(name, _)| (*name).to_string()).collect(),
            ..Default::default()
        },
    );
    let mut checker = tsr_checker::Checker::with_module_host(
        program.binder(),
        program.nodes(),
        program.node_map(),
        Some(&program),
    );
    checker.apply_compiler_options(program.compiler_options());
    let mut ids = Vec::new();
    for (name, _) in files {
        let id = program.source_file(name).expect("loaded").source_file().node_id.expect("id");
        ids.push((id, *name));
        checker.check_source_file(id, FileContext { ambient: false, has_parse_errors: false });
    }
    let mut out: Vec<_> = checker
        .diagnostics()
        .iter()
        .filter_map(|(file, diagnostic)| {
            let (_, name) = ids.iter().find(|(id, _)| id == file)?;
            let text = files.iter().find(|(n, _)| n == name)?.1;
            let line = text[..diagnostic.span.start as usize].matches('\n').count() + 1;
            Some(((*name).to_string(), line, diagnostic.message.code()))
        })
        .collect();
    out.sort();
    out
}

const FILES: [(&str, &str); 2] = [("/m1.js", "class Bar {}\n"), ("/m2.js", "const Bar = 3;\n")];

#[test]
fn checked_js_files_report_a_cross_file_redeclaration() {
    assert_eq!(
        diagnostics(&FILES, Tristate::True),
        [("/m1.js".to_string(), 1, 2451), ("/m2.js".to_string(), 1, 2451)]
    );
}

#[test]
fn plain_js_files_do_not() {
    assert_eq!(diagnostics(&FILES, Tristate::Unknown), []);
}
