//! TSR oracle worker.
//!
//! ```text
//! full_oracle_actual SOURCE REQUEST OUTPUT   one case per process
//! full_oracle_actual --serve                 one case per stdin line
//! ```
//!
//! `REQUEST` is one plan row (`CASE identity variant options`); the output is the
//! record stream of `tsr_conformance::full_oracle::actual`. A `--serve` line is
//! `hex(SOURCE) TAB hex(OUTPUT) TAB plan-row`; the reply is one stdout line,
//! `ok` or `error TAB message`, after the artifact is written. Each case runs on
//! a fresh thread, so no thread-local state crosses cases; process-wide state is
//! the immutable bundled-library text. A panic is reported on stderr and ends
//! the process (the runner records the case and starts a new worker), so no
//! case ever runs after another case's unwinding. See
//! `docs/parity/notes/oracle.md`.
use anyhow::{Context, Result, ensure};
use std::io::{BufRead, Write};
use std::path::Path;
use tsr_conformance::full_oracle::{Request, actual, unhex, write_atomic};

fn one(source: &Path, row: &str, output: &Path) -> Result<()> {
    let request = Request::from_row(row).context("request")?;
    let artifact = actual(source, &request)?;
    write_atomic(output, &artifact)
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "--serve" {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        for line in stdin.lock().lines() {
            let line = line?;
            let mut f = line.splitn(3, '\t');
            let (Some(source), Some(output), Some(row)) = (f.next(), f.next(), f.next()) else {
                anyhow::bail!("malformed serve request");
            };
            let (source, output, row) = (unhex(source)?, unhex(output)?, row.to_string());
            // The main thread's default size; `tsr_core::stack` grows past it.
            let result = std::thread::Builder::new()
                .stack_size(8 << 20)
                .spawn(move || one(Path::new(&source), &row, Path::new(&output)))?
                .join();
            match result {
                Ok(Ok(())) => writeln!(stdout, "ok")?,
                Ok(Err(e)) => {
                    eprintln!("Error: {e:?}");
                    writeln!(stdout, "error\t{}", format!("{e}").replace(['\t', '\n'], " "))?;
                }
                // The panic hook already printed the message to stderr.
                Err(_) => std::process::exit(101),
            }
            stdout.flush()?;
        }
        return Ok(());
    }
    ensure!(args.len() == 3, "usage: full_oracle_actual SOURCE REQUEST OUTPUT | --serve");
    let row = std::fs::read_to_string(&args[1])?;
    one(Path::new(&args[0]), row.trim_end_matches('\n'), Path::new(&args[2]))
}
