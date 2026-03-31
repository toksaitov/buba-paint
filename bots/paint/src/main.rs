use clap::Parser as _;

/// Parses CLI arguments and runs the selected bot command.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = buba_paint::cli::Cli::parse();
    buba_paint::cli::run(cli).await
}
