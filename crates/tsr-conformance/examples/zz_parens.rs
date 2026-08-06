//! Throwaway: which lines did the union-paren rule break?
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    let cases = corpus.discover().expect("cases");
    let targets = [
        "compiler/inferTypePredicates",
        "compiler/narrowingIntersection",
        "conformance/indexSignatures1",
    ];
    for case in &cases {
        if !targets.contains(&case.name.as_str()) {
            continue;
        }
        let Some(text) = case.expected_types() else { continue };
        let expected = types_baseline::parse(&text);
        let Ok(parsed) = case.load() else { continue };
        let ours = types_producer::assertions_for_case(&parsed, &expected, false);
        for (i, ef) in expected.iter().enumerate() {
            let Some(of) = ours.get(i) else { continue };
            for (p, want) in ef.assertions.iter().enumerate() {
                let Some(got) = of.get(p) else { continue };
                if want.text == got.line() || !got.type_string.contains('(') {
                    continue;
                }
                // Only the lines our parens could have broken: removing every
                // paren we could have added makes it match.
                if want.text
                    == format!("{} : {}", got.text, got.type_string.replace(['(', ')'], ""))
                {
                    println!(
                        "{}\n   ours {}\n   want {}",
                        case.name,
                        got.type_string,
                        want.text.strip_prefix(&format!("{} : ", got.text)).unwrap_or("")
                    );
                }
            }
        }
    }
}
