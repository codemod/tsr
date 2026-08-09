//! probe
fn main() {
    let arena = tsr_core::Arena::new();
    let source = r#"declare module ".\\relativeModule" { var x: string; }"#;
    let program = tsr_compiler::Program::in_arena(&arena, tsr_compiler::ProgramOptions {
        files: vec![("a.ts".into(), source.into())], ..Default::default() });
    for (_, s) in program.binder().symbols().iter() { println!("{:?}", s.name); }
}
