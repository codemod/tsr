//! One bounded TSR oracle worker: `full_oracle_actual SOURCE REQUEST OUTPUT`.
//!
//! `REQUEST` is one plan row (`CASE identity variant options`); the output is the
//! record stream of `tsr_conformance::full_oracle::actual`. See
//! `docs/parity/notes/oracle.md`.
use anyhow::{Context, Result, ensure};
use std::path::Path;
use tsr_conformance::full_oracle::{Request, actual, unhex, write_atomic};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(args.len() == 3, "usage: full_oracle_actual SOURCE REQUEST OUTPUT");
    let row = std::fs::read_to_string(&args[1])?;
    let f: Vec<_> = row.trim_end_matches('\n').split('\t').collect();
    ensure!(f.len() == 4 && f[0] == "CASE", "request is not a plan CASE row");
    let request = Request {
        identity: unhex(f[1])?,
        variant: unhex(f[2])?,
        options: Request::decode_options(f[3]).context("request options")?,
    };
    let artifact = actual(Path::new(&args[0]), &request)?;
    write_atomic(Path::new(&args[2]), &artifact)
}
