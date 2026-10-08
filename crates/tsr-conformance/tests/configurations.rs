//! The configuration expansion against native's own, over the whole corpus.
//!
//! `fixtures/native_configurations.tsv` is upstream's
//! `GetFileBasedTestConfigurations(t, extractCompilerSettings(content),
//! compilerVaryBy)` run over every `compiler/` and `conformance/` case at the
//! pinned commit, one `suite/case<TAB>configuration` row per named
//! configuration and `suite/case<TAB>FATAL` for a case the runner fails before
//! compiling. ADR-0047 records the Go test that wrote it.
//!
//! This is the falsifier for `crate::configuration`: the vary-by table, the
//! enum maps, the value deduplication, `*` and exclusions, and the naming are
//! all exercised by real cases here, and a single row of drift fails it.

use std::collections::BTreeSet;

use tsr_conformance::{Corpus, configuration::Configurations, repo_root};

#[test]
fn every_case_expands_into_exactly_the_configurations_native_runs() {
    let corpus = Corpus::from_repo_root(&repo_root());
    if !corpus.is_available() {
        eprintln!(
            "SKIPPED: corpus unavailable; run `git submodule update --init --recursive`. \
             This is not a pass."
        );
        return;
    }
    let native: BTreeSet<String> = include_str!("fixtures/native_configurations.tsv")
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();

    let mut ours = BTreeSet::new();
    for case in corpus.discover().expect("discovering cases") {
        match case.configurations() {
            Configurations::Varied(list) => {
                for configuration in list {
                    ours.insert(format!("{}\t{}", case.name, configuration.name));
                }
            }
            Configurations::Rejected(_) => {
                ours.insert(format!("{}\tFATAL", case.name));
            }
            Configurations::None | Configurations::Single(_) => {}
        }
    }

    let missing: Vec<_> = native.difference(&ours).take(20).collect();
    let extra: Vec<_> = ours.difference(&native).take(20).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "native runs but we do not: {missing:#?}\nwe run but native does not: {extra:#?}"
    );
    assert_eq!(ours.len(), native.len());
}
