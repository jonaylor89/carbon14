use chrono::{DateTime, Local, Utc};
use color_eyre::Result;
use colored::*;
use reqwest::{header::HeaderMap, Client};
use scraper::{Html, Selector};
use std::collections::HashSet;
use url::Url;

#[derive(Debug)]
pub struct AnalysisResult {
    pub timestamp: DateTime<Utc>,
    pub absolute: String,
    pub internal: bool,
}

pub struct Analysis {
    pub url: String,
    pub author: Option<String>,
    pub images: Vec<AnalysisResult>,
    pub headers: HeaderMap,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub title: Option<String>,
}

impl Analysis {
    pub async fn new(
        url: String,
        author: Option<String>,
        html: &str,
        headers: HeaderMap,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        client: &Client,
    ) -> Self {
        let title = extract_title(html);
        let images = collect_images(html, &url, client).await;

        Analysis {
            url,
            author,
            images,
            headers,
            start,
            end,
            title,
        }
    }

    fn report_section(&self, title: &str, selector: impl Fn(&AnalysisResult) -> bool) {
        println!("\n{}# {}\n", "#".red(), title);
        let filtered: Vec<_> = self.images.iter().filter(|i| selector(i)).collect();
        if filtered.is_empty() {
            println!("Nothing found.");
            return;
        }

        println!("{}", "-".repeat(80).dimmed());
        println!(
            "{}",
            format!("{:20} {:20} {}", "Date (UTC)", "Date (Local)", "URL").bold()
        );
        println!(
            "{}",
            format!("{} {} {}", "-".repeat(20), "-".repeat(20), "-".repeat(38)).dimmed()
        );
        for result in filtered {
            println!(
                "{:20} {:20} <{}>",
                result.timestamp.format("%Y-%m-%d %H:%M:%S"),
                result
                    .timestamp
                    .with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M:%S"),
                result.absolute
            );
        }
        println!("{}", "-".repeat(80).dimmed());
    }

    pub fn report(&self) {
        println!("---");
        println!("{}", format!("title: Carbon14 web page analysis").magenta());
        if let Some(ref author) = self.author {
            println!("{}", format!("author: {}", author).magenta());
        }
        println!(
            "{}",
            format!("date: {}", self.start.format("%Y-%m-%d")).magenta()
        );
        println!("---");

        println!("\n{}# General information\n", "#".red());
        let started = self.start.with_timezone(&Local);
        let ended = self.end.with_timezone(&Local);
        let metadata = vec![
            ("Page URL", format!("<{}>", self.url)),
            (
                "Page title",
                self.title.clone().unwrap_or_else(|| "N/A".to_string()),
            ),
            (
                "Analysis started",
                format!(
                    "{} ({:?})",
                    started.format("%Y-%m-%d %H:%M:%S"),
                    started.timezone()
                ),
            ),
            (
                "Analysis ended",
                format!(
                    "{} ({:?})",
                    ended.format("%Y-%m-%d %H:%M:%S"),
                    ended.timezone()
                ),
            ),
        ];

        for (label, value) in metadata {
            println!("- {}**{}:** {}", label.cyan().bold(), label, value);
        }

        println!("\n{}# HTTP headers\n", "#".red());
        for (key, value) in &self.headers {
            println!("    {:?}: {}", key, value.to_str().unwrap_or(""));
        }

        self.report_section("Internal images", |r| r.internal);
        self.report_section("External images", |r| !r.internal);
        self.report_section("All images", |_| true);
    }
}

pub async fn fetch_page(client: &Client, url: &str) -> Result<(HeaderMap, String)> {
    println!("{}", format!("Fetching page {}", url).dimmed());
    let response = client.get(url).send().await?;
    let headers = response.headers().to_owned();
    let html = response.text().await?;
    Ok((headers, html))
}

fn extract_title(html: &str) -> Option<String> {
    let document = Html::parse_document(html);
    let title_selector = Selector::parse("title").unwrap();
    document
        .select(&title_selector)
        .next()
        .map(|el| el.inner_html())
}

async fn handle_image(
    base_url: &str,
    address: &str,
    client: &Client,
    requested: &mut HashSet<String>,
) -> Option<AnalysisResult> {
    if address.is_empty() || requested.contains(address) {
        return None;
    }
    requested.insert(address.to_string());
    println!("{}", format!("Working on image {}", address).dimmed());

    let absolute = Url::parse(base_url).ok()?.join(address).ok()?;
    let headers = client
        .get(absolute.as_str())
        .send()
        .await
        .ok()?
        .headers()
        .clone();
    let last_modified = headers.get("Last-Modified").and_then(|h| h.to_str().ok())?;
    let timestamp = DateTime::parse_from_rfc2822(last_modified)
        .ok()?
        .with_timezone(&Utc);

    let internal = Url::parse(base_url).ok()?.host() == absolute.host();
    Some(AnalysisResult {
        timestamp,
        absolute: absolute.to_string(),
        internal,
    })
}

pub async fn collect_images(html: &str, base_url: &str, client: &Client) -> Vec<AnalysisResult> {
    let document = Html::parse_document(html);
    let mut requested = HashSet::new();
    let mut images = Vec::new();

    // Collect images from <img> tags
    let img_selector = Selector::parse("img").unwrap();
    for element in document.select(&img_selector) {
        if let Some(src) = element.value().attr("src") {
            if !src.starts_with("data:") {
                if let Some(result) = handle_image(base_url, src, client, &mut requested).await {
                    images.push(result);
                }
            }
        }
    }

    // Collect OpenGraph images
    let og_selector = Selector::parse("meta[property=\"og:image\"]").unwrap();
    for element in document.select(&og_selector) {
        if let Some(content) = element.value().attr("content") {
            if let Some(result) = handle_image(base_url, content, client, &mut requested).await {
                images.push(result);
            }
        }
    }

    images.sort_by_key(|i| i.timestamp);
    images
}
