//! Overflow finder: run every `checker_types` case SEQUENTIALLY, printing the
//! case name to stderr before processing it. A stack overflow aborts the
//! process, so the last name printed is the offender — which the parallel
//! `scorepair` run can never tell you. `TSR_SKIP=<n>` resumes after the first
//! n cases so repeated offenders can be collected in one session.
//! `TSR_ALL=1` also walks the cases the verdict population skips
//! (`has_varied_types`/`has_known_divergence`) — the coverage binary's
//! suites process those, so an abort only they can reach hides from the
//! default walk. `TSR_DIAG=1` drives `check_source_file` instead of the
//! assertion walk — the diagnostics suite's road, which checks statements no
//! `.types` assertion ever queries.
use tsr_conformance::{Corpus, repo_root, types_baseline, types_producer};

fn main() {
    let skip: usize = std::env::var("TSR_SKIP").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let all = std::env::var_os("TSR_ALL").is_some();
    let diag = std::env::var_os("TSR_DIAG").is_some();
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("cases");
    for (n, case) in cases.iter().enumerate() {
        if n < skip {
            continue;
        }
        if !all && (case.has_varied_types() || case.has_known_divergence()) {
            continue;
        }
        if diag {
            eprintln!("{n} {}", case.name);
            let Ok(test) = case.load() else { continue };
            let arena = tsr_core::Arena::new();
            let program = types_producer::program_for_case(&arena, &test);
            let mut checker = tsr_checker::Checker::with_module_host(
                program.binder(),
                program.nodes(),
                program.node_map(),
                Some(&program),
            );
            checker.apply_compiler_options(program.compiler_options());
            let mut own_files = Vec::new();
            for unit in &test.files {
                if tsr_parser::ScriptKind::from_file_name(&unit.name)
                    == tsr_parser::ScriptKind::Json
                {
                    continue;
                }
                if let Some(file) = program.source_file(&unit.name)
                    && let Some(id) = file.source_file().node_id
                {
                    own_files.push(id);
                }
            }
            checker.set_checked_files(own_files.clone());
            for unit in &test.files {
                let Some(file) = program.source_file(&unit.name) else { continue };
                let Some(id) = file.source_file().node_id else { continue };
                let declaration_file = unit.name.ends_with(".d.ts")
                    || unit.name.ends_with(".d.mts")
                    || unit.name.ends_with(".d.cts");
                checker.check_source_file(
                    id,
                    tsr_checker::check::FileContext {
                        ambient: declaration_file,
                        has_parse_errors: !file.diagnostics().is_empty(),
                    },
                );
            }
            continue;
        }
        let Some(text) = case.expected_types() else { continue };
        let expected = types_baseline::parse(&text);
        if types_baseline::assertion_count(&expected) == 0 {
            continue;
        }
        let Ok(parsed) = case.load() else { continue };
        eprintln!("{n} {}", case.name);
        let arena = tsr_core::Arena::new();
        let _ = types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
    }
    eprintln!("done");
}
