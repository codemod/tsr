//! `getExternalModuleIndicator`'s JSX arm (`ast/parseoptions.go:85`): under
//! `moduleDetection: auto` and `jsx: react-jsx`/`react-jsxdev`, a file holding
//! a JSX tag is a module even with no `import` or `export`.
//! `docs/parity/notes/r5-config.md` §5.

use tsr_compiler::{LoadOptions, Program};
use tsr_core::{CompilerOptions, JsxEmit, ModuleDetectionKind};

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

/// Whether `name`, compiled alone under `options`, binds as an external module
/// (the binder gives an external module's `SourceFile` a symbol).
fn is_module(name: &str, text: &str, options: CompilerOptions) -> bool {
    let host = Host {
        fs: tsr_vfs::InMemoryFileSystem::new([(name.to_string(), text.to_string())], [], true),
    };
    let arena = tsr_core::Arena::new();
    let program = Program::from_root_files(
        &arena,
        &host,
        LoadOptions {
            compiler_options: options,
            root_file_names: vec![name.to_string()],
            ..Default::default()
        },
    );
    let file = program.source_file(name).expect("the root is loaded");
    let id = file.source_file().node_id.expect("a parsed file has an id");
    program.binder().symbol_of(id).is_some()
}

fn jsx(jsx: JsxEmit) -> CompilerOptions {
    CompilerOptions { jsx, ..Default::default() }
}

const TAGGED: &str = "declare namespace JSX { interface IntrinsicElements { [s: string]: any } }\n\
                      const x = <div />;\n";

#[test]
fn a_jsx_tag_makes_a_file_a_module_under_the_automatic_runtime() {
    assert!(is_module("/a.tsx", TAGGED, jsx(JsxEmit::ReactJsx)));
    assert!(is_module("/a.tsx", TAGGED, jsx(JsxEmit::ReactJsxDev)));
    assert!(is_module("/a.tsx", "const x = <></>;\n", jsx(JsxEmit::ReactJsx)), "a fragment counts");
}

#[test]
fn the_classic_runtime_and_untagged_files_stay_scripts() {
    assert!(!is_module("/a.tsx", TAGGED, jsx(JsxEmit::React)));
    assert!(!is_module("/a.tsx", TAGGED, jsx(JsxEmit::Preserve)));
    assert!(!is_module("/a.tsx", "const x = 1;\n", jsx(JsxEmit::ReactJsx)));
}

#[test]
fn only_auto_detection_reads_the_tags() {
    let legacy = CompilerOptions {
        jsx: JsxEmit::ReactJsx,
        module_detection: ModuleDetectionKind::Legacy,
        ..Default::default()
    };
    assert!(!is_module("/a.tsx", TAGGED, legacy), "legacy detection ignores JSX");
}
