//! Only this required-feature example accepts a scheduling policy argument.
use clap::Parser;
use llmgw::admission::BenchmarkPolicy as Policy;
use llmgw::{config::LoadedConfig, server};
use std::path::PathBuf;
#[derive(Parser)]
struct Args {
    #[arg(long)]
    config: PathBuf,
    #[arg(long, value_enum)]
    policy: Policy,
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let loaded = LoadedConfig::load(args.config)?;
    let credentials = server::RuntimeCredentials::load(&loaded)?;
    server::spawn_benchmark(loaded.config, credentials, args.policy)
        .await?
        .run_foreground()
        .await?;
    Ok(())
}
