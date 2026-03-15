use clap::Parser as _;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = buba_paint::cli::Cli::parse();
    buba_paint::cli::run(cli).await
}
