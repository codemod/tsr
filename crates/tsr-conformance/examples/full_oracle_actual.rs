//! Isolated TSR producer entry point; inputs contain no expected artifacts.
use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use tsr_conformance::full_oracle::Configuration;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 { bail!("usage: full_oracle_actual SOURCE SETTINGS OUTPUT_DIRECTORY"); }
    let dir = PathBuf::from(&args[2]);
    let artifacts = tsr_conformance::full_oracle_actual::produce(&Configuration {
        id: std::env::var("FULL_ORACLE_ID").context("configuration identity")?,
        source: PathBuf::from(&args[0]), name: String::new(),
        settings: std::fs::read_to_string(&args[1])?,
    })?;
    std::fs::create_dir_all(&dir)?;
    for (name, bytes) in [("diagnostics.semantic", artifacts.diagnostics.into_bytes()), ("errors.txt", artifacts.errors), ("types", artifacts.types)] {
        use std::io::Write;
        let mut file = std::fs::File::create(dir.join(name))?;
        file.write_all(&bytes)?; file.sync_all()?;
    }
    Ok(())
}
