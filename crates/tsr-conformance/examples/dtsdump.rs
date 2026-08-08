//! Dump produced against expected `.d.ts` text for named cases.
//!
//! `dts_emit`'s snapshot names one differing line per failure, which orders the
//! board but does not show the whole disagreement. This prints both texts per
//! unit so a failure can be read in full before a fix is sized.
//!
//! Usage: `cargo run -p tsr-conformance --example dtsdump -- <name-filter>...`

use tsr_conformance::{Corpus, dts_emit_suite, repo_root};

fn main() {
    let filters: Vec<String> = std::env::args().skip(1).collect();
    assert!(!filters.is_empty(), "pass at least one case-name filter");
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("discover corpus");
    for case in &cases {
        if !filters.iter().any(|filter| case.name.contains(filter.as_str())) {
            continue;
        }
        let Some(units) = dts_emit_suite::dump_case(case) else {
            println!("== {} — not judged (skip)", case.name);
            continue;
        };
        for (unit, produced, expected) in units {
            println!("== {} :: {unit}", case.name);
            if produced == expected {
                println!("   (identical)");
                continue;
            }
            println!("-- produced --");
            println!("{produced}");
            println!("-- expected --");
            println!("{expected}");
        }
    }
}
