//! Run the complete native population; optionally compare an independent TSR
//! artifact producer. Never translate missing support into exact verdicts.
use std::path::{Path, PathBuf};
use anyhow::{Result, bail};
use tsr_conformance::full_oracle::{self, Verdict};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 1 && args.len() != 2 {
        bail!("usage: full_oracle_run OUTPUT_DIRECTORY [ACTUAL_PRODUCER]");
    }
    let dir = PathBuf::from(&args[0]);
    let population = full_oracle::native_population(
        &tsr_conformance::repo_root().join("vendor/typescript-go"), &dir.join("native"), None,
    )?;
    let mut exact = 0;
    let total = population.configurations.len() + population.discovery_failures.len();
    for (id, reason) in &population.discovery_failures {
        println!("{}\t{:?}", id, Verdict::DiscoveryFailure(reason.clone()));
    }
    for (index, (id, native)) in population.configurations.iter().enumerate() {
        let verdict = match &native.output {
            Err(reason) => Verdict::NativeFailure(reason.clone()),
            Ok(expected) => match args.get(1) {
                None => Verdict::ActualFailure("lossless TSR artifact producer not supplied".into()),
                Some(producer) => match full_oracle::actual_artifacts(
                    Path::new(producer), &native.configuration, &dir.join(format!("actual/{index}")),
                ) {
                    Ok(actual) => full_oracle::compare(expected, &actual),
                    Err(error) => Verdict::ActualFailure(format!("{error:#}")),
                },
            },
        };
        exact += usize::from(verdict == Verdict::Exact);
        println!("{id}\t{verdict:?}\t{}", native.eligibility);
    }
    eprintln!("FULL_POPULATION {total} CONFIGURATIONS {} DISCOVERY_FAILURES {} EXACT {exact}",
        population.configurations.len(), population.discovery_failures.len());
    Ok(())
}
