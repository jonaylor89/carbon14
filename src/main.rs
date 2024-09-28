use carbon_14::analysis::{fetch_page, Analysis};
use chrono::Utc;
use clap::{command, Parser};
use color_eyre::Result;
use reqwest::Client;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// URL of the page
    url: String,

    /// Author to be included in the report
    #[arg(short, long)]
    author: Option<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let args = Args::parse();
    let client = Client::new();
    let start = Utc::now();

    let (headers, html) = fetch_page(&client, &args.url).await?;
    let end = Utc::now();

    let analysis = Analysis::new(args.url, args.author, &html, headers, start, end, &client).await;
    analysis.report();

    Ok(())
}
