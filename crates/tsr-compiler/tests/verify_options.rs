//! `verify_compiler_options` against `Program.verifyCompilerOptions`
//! (5b1047d `internal/compiler/program.go:751`): what is reported, where it is
//! positioned, and the controls that must stay silent.

use tsr_compiler::{
    Program, ProgramOptions,
    program_diagnostics::{OptionsVerification, verify_compiler_options},
};
use tsr_core::{CompilerOptions, ModuleKind, ModuleResolutionKind, Tristate};
use tsr_tsoptions::syntax::ConfigSyntax;

/// No config file, no output-path suppression.
const NONE: OptionsVerification<'static> =
    OptionsVerification { config_file: None, suppress_output_path_check: Tristate::Unknown };

fn verify(
    files: &[(&str, &str)],
    options: CompilerOptions,
    input: OptionsVerification<'_>,
) -> Vec<(String, u32, String, Vec<u32>)> {
    let arena = tsr_core::Arena::new();
    let program = Program::in_arena(
        &arena,
        ProgramOptions {
            files: files.iter().map(|(n, t)| ((*n).to_string(), (*t).to_string())).collect(),
            compiler_options: options,
            ..Default::default()
        },
    );
    verify_compiler_options(&program, input)
        .into_iter()
        .map(|(file, d)| {
            let chain = d.message_chain().iter().map(|c| c.message.code()).collect();
            let at =
                if file.is_empty() { String::new() } else { format!("{file}@{}", d.span.start) };
            (at, d.message.code(), d.text(), chain)
        })
        .collect()
}

#[test]
fn a_js_input_emitted_in_place_overwrites_itself_unless_output_moves() {
    let js = CompilerOptions { allow_js: Tristate::True, ..CompilerOptions::default() };
    let found = verify(&[("/a.js", "var x;"), ("/b.ts", "var y;")], js.clone(), NONE);
    assert_eq!(
        found,
        [(
            String::new(),
            5055,
            "Cannot write file '/a.js' because it would overwrite input file.".to_string(),
            vec![5068],
        )]
    );
    // Controls: an output directory, `noEmit`, and the harness's suppression.
    let out_dir = CompilerOptions { out_dir: "/out".into(), ..js.clone() };
    assert!(verify(&[("/a.js", "var x;")], out_dir, NONE).is_empty());
    let no_emit = CompilerOptions { no_emit: Tristate::True, ..js.clone() };
    assert!(verify(&[("/a.js", "var x;")], no_emit, NONE).is_empty());
    let suppressed =
        OptionsVerification { suppress_output_path_check: Tristate::True, ..Default::default() };
    assert!(verify(&[("/a.js", "var x;")], js, suppressed).is_empty());
}

#[test]
fn node_resolution_without_a_node_module_is_reported_on_the_config_value() {
    let text = r#"{ "compilerOptions": { "module": "esnext", "moduleResolution": "node16" } }"#;
    let syntax = ConfigSyntax::parse(text);
    let options = CompilerOptions {
        module: ModuleKind::ESNext,
        module_resolution: ModuleResolutionKind::Node16,
        config_file_path: "/tsconfig.json".into(),
        ..CompilerOptions::default()
    };
    let input = OptionsVerification {
        config_file: Some(("/tsconfig.json", &syntax)),
        ..Default::default()
    };
    let found = verify(&[("/a.ts", "export {};")], options.clone(), input);
    let value = u32::try_from(text.find("\"esnext\"").unwrap()).unwrap();
    assert_eq!(
        found,
        [(
            format!("/tsconfig.json@{value}"),
            5110,
            "Option 'module' must be set to 'Node16' when option 'moduleResolution' is set to 'Node16'."
                .to_string(),
            vec![],
        )]
    );
    // Without a config file the same diagnostic has no position.
    let found = verify(&[("/a.ts", "export {};")], options, NONE);
    assert_eq!(found[0].0, "");
    // Control: a matching module kind reports nothing.
    let node = CompilerOptions {
        module: ModuleKind::Node16,
        module_resolution: ModuleResolutionKind::Node16,
        ..CompilerOptions::default()
    };
    assert!(verify(&[("/a.ts", "export {};")], node, NONE).is_empty());
}

#[test]
fn a_set_downlevel_iteration_is_a_removed_option() {
    let options =
        CompilerOptions { downlevel_iteration: Tristate::False, ..CompilerOptions::default() };
    let found = verify(&[("/a.ts", "var x;")], options, NONE);
    assert_eq!(found[0].1, 5102);
    assert_eq!(
        found[0].2,
        "Option 'downlevelIteration' has been removed. Please remove it from your configuration."
    );
}
