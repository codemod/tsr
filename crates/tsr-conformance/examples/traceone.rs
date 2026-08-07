//! §12.2's mandated trace instrument: run ONE case, print chosen lines'
//! want/got. `TSR_TRACE_CASE=<name>` selects; positions via TSR_TRACE_POS.
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

fn main() {
    let case_name = std::env::var("TSR_TRACE_CASE").expect("TSR_TRACE_CASE");
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("cases");
    let case = cases.iter().find(|c| c.name.ends_with(&case_name)).expect("case found");
    let text = case.expected_types().expect("types");
    let expected = types_baseline::parse(&text);
    let parsed = case.load().expect("load");
    let arena = tsr_core::Arena::new();
    let (_program, ours, _ids) =
        types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
    let want_pos: Vec<usize> = std::env::var("TSR_TRACE_POS")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    for (index, expected_file) in expected.iter().enumerate() {
        let Some(our_file) = ours.get(index) else { continue };
        for (position, want) in expected_file.assertions.iter().enumerate() {
            if !want_pos.is_empty() && !want_pos.contains(&position) {
                continue;
            }
            let Some(got) = our_file.get(position) else { continue };
            if want.text != got.line() {
                println!("{index}:{position} WANT {} | GOT {}", want.text, got.line());
            }
        }
    }
}
