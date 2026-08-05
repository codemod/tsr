//! Where a type parameter's symbol actually lands, per declaring construct.
//!
//! A probe, not a test: `bd tsr-y4u.21` reported that a class's or interface's
//! type parameters are "in no scope at all", on the strength of a `lookup_local`
//! sweep. `lookup_local` reads only `locals`, and upstream files a class's or
//! interface's type parameters in the **members** table of the class symbol
//! (`declareSymbolAndAddToSymbolTable` → `declareClassMember`,
//! `internal/binder/binder.go:429-441`). This prints both tables so the two
//! claims can be told apart.

use tsr_binder::SymbolFlags;
use tsr_core::Arena;

fn main() {
    let sources = [
        "declare function f<T>(p: T): void;",
        "type A<T> = { p: T };",
        "declare const g: <T>(p: T) => void;",
        "interface I<T> { p: T }",
        "class C<T> { p: T; }",
        "declare class D<T> { m(p: T): void; }",
        "const E = class<T> { p: T; };",
    ];
    for source in sources {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        let bound = tsr_binder::bind(
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "probe.ts", text: source },
        );
        let mut where_is_t = Vec::new();
        for (id, symbol) in bound.symbols().iter() {
            if symbol.flags.contains(SymbolFlags::TYPE_PARAMETER) {
                let owner = symbol.parent.map_or("<no parent>", |p| bound.symbols().get(p).name);
                where_is_t.push(format!("{} (id {:?}) parent={owner}", symbol.name, id));
            }
            for (name, member) in &symbol.members {
                if bound.symbols().get(*member).flags.contains(SymbolFlags::TYPE_PARAMETER) {
                    where_is_t.push(format!("  members[{name}] of `{}`", symbol.name));
                }
            }
        }
        println!("{source}\n    {}", where_is_t.join("\n    "));
    }
}
