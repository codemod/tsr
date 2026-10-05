//! Archive example: reuse the configured conformance producer and its query API.
use std::collections::BTreeSet;
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn main() {
    let path = std::env::args().nth(1).expect("fixture path");
    let text = std::fs::read_to_string(&path).expect("fixture text");
    let case = TestCase::parse("reference-spelling", "contract.ts", &text);
    assert!(case.error.is_none(), "invalid case directives");
    for file in &case.files {
        print!("input\t{}\t", file.name);
        for byte in file.content.as_bytes() {
            print!("{byte:02x}");
        }
        println!();
    }
    let expected: Vec<_> = case
        .files
        .iter()
        .filter(|file| {
            std::path::Path::new(&file.name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
        })
        .map(|file| FileTypes { file: file.name.clone(), assertions: Vec::new() })
        .collect();
    let arena = tsr_core::Arena::new();
    let (program, rendered, ids) =
        types_producer::assertions_for_case_with_ids(&arena, &case, &expected);
    let mut chosen = Vec::new();
    let mut seen = BTreeSet::new();
    for ((file, assertions), nodes) in expected.iter().zip(&rendered).zip(ids) {
        for (assertion, id) in assertions.iter().zip(nodes) {
            if assertion.text.starts_with("probe_")
                && assertion.text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && seen.insert((file.file.clone(), assertion.text.clone()))
            {
                println!("producer\t{}\t{}\t{}", file.file, assertion.text, assertion.type_string);
                chosen.push((file.file.clone(), assertion.text.clone(), id));
            }
        }
    }
    assert!(!chosen.is_empty(), "fixture has no named probes");
    for (phase, reverse) in [("forward", false), ("reverse", true)] {
        let mut checker = types_producer::configured_checker(&program);
        let mut order = chosen.clone();
        if reverse {
            order.reverse();
        }
        for (file, name, id) in order {
            let rendered = types_producer::type_at_location(
                &mut checker,
                program.binder(),
                program.nodes(),
                program.node_map(),
                id,
            );
            println!("{phase}\t{file}\t{name}\t{rendered}");
        }
    }
}
