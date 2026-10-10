//! `tryGetAnyFileFromPath` (`modulespecifiers/util.go:194`) as the program
//! answers it after loading: for the directory of each `index.*` program
//! file, whether a file shares the directory's name.
//! `docs/parity/notes/r6-specifiers2.md` §3.

use tsr_checker::resolution::ModuleHost;
use tsr_compiler::{LoadOptions, Program};

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

/// `declarationEmitCommonJsModuleReferencedType`'s `foo` package, reduced:
/// `foo/other/index.d.ts` beside `foo/other.d.ts`, and `foo/sole/index.d.ts`
/// with no sibling.
const FILES: [(&str, &str); 4] = [
    ("/a.ts", "import \"foo/other/index\";\nimport \"foo/sole\";\nimport \"foo/other\";\n"),
    ("/node_modules/foo/other/index.d.ts", "export interface OtherIndexProps {}\n"),
    ("/node_modules/foo/other.d.ts", "export interface OtherProps {}\n"),
    ("/node_modules/foo/sole/index.d.ts", "export interface SoleProps {}\n"),
];

#[test]
fn the_program_answers_the_index_directories_it_loaded() {
    let host = Host {
        fs: tsr_vfs::InMemoryFileSystem::new(
            FILES.iter().map(|(name, text)| ((*name).to_string(), (*text).to_string())),
            [],
            true,
        ),
    };
    let arena = tsr_core::Arena::new();
    let program = Program::from_root_files(
        &arena,
        &host,
        LoadOptions { root_file_names: vec!["/a.ts".to_string()], ..Default::default() },
    );
    assert!(program.source_file("/node_modules/foo/sole/index.d.ts").is_some());
    let module_host: &dyn ModuleHost = &program;
    // `/node_modules/foo/other.d.ts` exists, so `/index` stays.
    assert_eq!(module_host.any_file_from_path("/node_modules/foo/other"), Some(true));
    assert_eq!(module_host.any_file_from_path("node_modules/foo/other"), Some(true));
    assert_eq!(module_host.any_file_from_path("/node_modules/foo/sole"), Some(false));
    // Not the directory of an `index.*` program file: not probed.
    assert_eq!(module_host.any_file_from_path("/node_modules/foo/elsewhere"), None);
}
