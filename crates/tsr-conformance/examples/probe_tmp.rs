fn main() {
    let p = tsr_parser::ParsedFile::parse("class C3 { accessor x = 0; }".into());
    println!("diags {}", p.diagnostics().len());
    let arena = tsr_core::Arena::new();
    let program = tsr_compiler::Program::in_arena(
        &arena,
        tsr_compiler::ProgramOptions {
            files: vec![("a.ts".into(), "class C3 { accessor x = 0; }".into())],
            ..Default::default()
        },
    );
    for (_, s) in program.binder().symbols().iter() {
        println!("{:?} parent={:?}", s.name, s.parent);
    }
}
