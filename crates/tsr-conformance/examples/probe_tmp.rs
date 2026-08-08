fn main() {
    for src in ["function f1(enum) {}", "enum void {}", "class C { m(this: number) {} }"] {
        let parsed = tsr_parser::ParsedFile::parse(src.to_string());
        println!("{src:?} -> diags {}", parsed.diagnostics().len());
    }
}
