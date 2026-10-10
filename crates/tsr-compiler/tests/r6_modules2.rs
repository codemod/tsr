//! r6-modules2's ports over a real program, so imports resolve.
//! `docs/parity/notes/r6-modules2.md`.
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

/// The `(line, column, code)` triples the checker reports in `file`, sorted,
/// 1-based as the baselines print them.
fn reported(files: &[(&str, &str)], options: CompilerOptions, file: &str) -> Vec<(u32, u32, u32)> {
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
    let source = program.source_file(file).expect("loaded");
    let text = files.iter().find(|(name, _)| *name == file).expect("listed").1;
    let id = source.source_file().node_id.expect("id");
    let mut checker = tsr_checker::Checker::with_module_host(
        program.binder(),
        program.nodes(),
        program.node_map(),
        Some(&program),
    );
    checker.apply_compiler_options(program.compiler_options());
    checker.check_source_file(id, FileContext { ambient: false, has_parse_errors: false });
    let mut reported: Vec<(u32, u32, u32)> = checker
        .diagnostics()
        .iter()
        .filter(|(at, _)| *at == id)
        .map(|(_, diagnostic)| {
            let start = diagnostic.span.start as usize;
            let before = &text[..start];
            let line = before.matches('\n').count() as u32 + 1;
            let column = (start - before.rfind('\n').map_or(0, |at| at + 1)) as u32 + 1;
            (line, column, diagnostic.message.code())
        })
        .collect();
    reported.sort_unstable();
    reported
}

fn verbatim() -> CompilerOptions {
    CompilerOptions {
        module: ModuleKind::ESNext,
        target: ScriptTarget::ESNext,
        verbatim_module_syntax: Tristate::True,
        ..Default::default()
    }
}

/// An alias merged with a local value ends the type-only walk
/// (`getTypeOnlyAliasDeclarationEx`'s `Alias && !meaning` loop, and the link
/// `resolveIndirectionAlias` copies only through a pure alias). Native `tsgo`
/// reports nothing in either file.
const VALUE_MERGED_REEXPORT: [(&str, &str); 3] = [
    ("/a.ts", "export type A = \"a\";\n"),
    ("/b.ts", "import type { A } from \"./a\";\nconst A: A = \"a\";\nexport { A };\n"),
    ("/c.ts", "import { A } from \"./b\";\nexport default A;\n"),
];

#[test]
fn a_value_merged_alias_ends_the_type_only_walk() {
    assert_eq!(reported(&VALUE_MERGED_REEXPORT, verbatim(), "/b.ts"), []);
    assert_eq!(reported(&VALUE_MERGED_REEXPORT, verbatim(), "/c.ts"), []);
}

#[test]
fn the_type_only_link_still_crosses_a_pure_alias() {
    // TS1485 and TS1448 through `f.ts`'s pure re-export, as native reports.
    let files = [
        ("/a.ts", "export class C {}\n"),
        ("/f.ts", "import type { C } from \"./a\";\nexport { C };\n"),
        ("/g.ts", "import { C } from \"./f\";\nexport { C };\n"),
    ];
    assert_eq!(reported(&files, verbatim(), "/g.ts"), [(1, 10, 1485), (2, 10, 1448)]);
}
