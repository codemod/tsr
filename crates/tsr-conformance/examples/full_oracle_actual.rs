#[path = "../src/full_oracle.rs"]
mod full_oracle;
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    anyhow::ensure!(args.len() == 3, "usage: full_oracle_actual SOURCE REQUEST OUTPUT");
    let artifact =
        full_oracle::actual(std::path::Path::new(&args[0]), std::path::Path::new(&args[1]))?;
    full_oracle::write_atomic(std::path::Path::new(&args[2]), &artifact)
}
