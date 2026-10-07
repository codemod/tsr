//! Complete pinned native/TSR oracle. Both producers are mandatory.
use std::{collections::BTreeSet, io::Write, path::PathBuf};
use anyhow::{Result, bail};
use rayon::prelude::*;
use tsr_conformance::full_oracle::{self, Verdict};
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 1 || args.len() > 3 {
        bail!("usage: full_oracle_run OUTPUT_DIRECTORY [SELECTION_REGEX] [PRIOR_VERDICTS_TSV]");
    }
    let dir = PathBuf::from(&args[0]);
    let producer = std::env::current_exe()?.with_file_name("full_oracle_actual");
    if !producer.is_file() { bail!("build the required full_oracle_actual example before running"); }
    let prior: BTreeSet<String> = if let Some(path) = args.get(2) {
        std::fs::read_to_string(path)?.lines().filter_map(|line| {
            let (id, rest) = line.split_once('\t')?;
            (rest.split('\t').next() == Some("Exact")).then(|| id.to_string())
        }).collect()
    } else { BTreeSet::new() };
    let population = full_oracle::native_population(
        &tsr_conformance::repo_root().join("vendor/typescript-go"), &dir.join("native"), args.get(1).map(String::as_str),
    )?;
    let total = population.configurations.len() + population.discovery_failures.len();
    let checkpoints = std::sync::Mutex::new(std::fs::File::create(dir.join("verdicts.tsv"))?);
    let exact_ids = std::sync::Mutex::new(BTreeSet::new());
    for (id, reason) in &population.discovery_failures {
        writeln!(checkpoints.lock().expect("checkpoint lock"), "{id}\t{:?}", Verdict::DiscoveryFailure(reason.clone()))?;
    }
    checkpoints.lock().expect("checkpoint lock").sync_all()?;
    let pool = rayon::ThreadPoolBuilder::new().num_threads(10).build()?;
    let records: Vec<_> = population.configurations.iter().enumerate().collect();
    pool.install(|| records.par_iter().for_each(|(index, (id, native))| {
        // Actual runs even when native fails, keeping both independent outcomes.
        let actual = full_oracle::actual_artifacts(&producer, &native.configuration, &dir.join(format!("actual/{index}")));
        let verdict = match (&native.output, actual) {
            (Err(reason), _) => Verdict::NativeFailure(reason.clone()),
            (Ok(expected), Ok(actual)) => full_oracle::compare(expected, &actual),
            (Ok(_), Err(error)) => Verdict::ActualFailure(format!("{error:#}")),
        };
        if verdict == Verdict::Exact { exact_ids.lock().expect("exact lock").insert((*id).clone()); }
        let mut file = checkpoints.lock().expect("checkpoint lock");
        writeln!(file, "{id}\t{verdict:?}\t{}", native.eligibility).expect("verdict checkpoint");
        file.sync_data().expect("durable verdict checkpoint");
        println!("{id}\t{verdict:?}");
    }));
    let exact = exact_ids.lock().expect("exact lock");
    let losses: Vec<_> = prior.difference(&exact).collect();
    std::fs::write(dir.join("prior-right-losses.txt"), losses.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n"))?;
    let summary = format!("FULL_POPULATION {total} CONFIGURATIONS {} DISCOVERY_FAILURES {} EXACT {} PRIOR_RIGHT_LOSSES {}\n",
        population.configurations.len(), population.discovery_failures.len(), exact.len(), losses.len());
    std::fs::write(dir.join("summary.txt"), &summary)?;
    eprint!("{summary}");
    if exact.len() * 1000 < total * 999 || !losses.is_empty() { bail!("full exact parity acceptance failed; durable outcomes retained"); }
    Ok(())
}
