use anyhow::{Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

const TIMESTAMP_URL: &str = "https://api.mangabaka.org/v1/database/series.timestamp.txt";
const DUMP_URL: &str = "https://api.mangabaka.org/v1/database/series.jsonl.zst";
const USER_AGENT: &str = "Dartotsu/1.0 (dartotsu_manga_mapper; +https://github.com/itsmechinmoy/dartotsu_manga_mapper)";

#[derive(Debug, Deserialize)]
struct SourceEntry {
    #[serde(default)]
    id: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct MangaBakaRecord {
    id: i64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default, rename = "type")]
    media_type: Option<String>,
    #[serde(default)]
    source: Option<HashMap<String, SourceEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MangaMappingItem {
    pub mangabaka_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anilist_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mal_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mangaupdates_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mangaupdates_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kitsu_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anime_planet_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anime_news_network_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shikimori_id: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FlatMapping {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<i64>>,
}

fn parse_i64(val: Option<&serde_json::Value>) -> Option<i64> {
    match val {
        Some(serde_json::Value::Number(n)) => n.as_i64(),
        Some(serde_json::Value::String(s)) => s.parse::<i64>().ok(),
        _ => None,
    }
}

fn parse_string(val: Option<&serde_json::Value>) -> Option<String> {
    match val {
        Some(serde_json::Value::String(s)) if !s.trim().is_empty() => Some(s.clone()),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

fn parse_base36(val: Option<&serde_json::Value>) -> (Option<i64>, Option<String>) {
    match val {
        Some(serde_json::Value::String(s)) if !s.trim().is_empty() => {
            let slug = s.trim().to_lowercase();
            let id = i64::from_str_radix(&slug, 36).ok();
            (id, Some(slug))
        }
        Some(serde_json::Value::Number(n)) => {
            let id = n.as_i64();
            let slug = id.map(|i| radix_36(i));
            (id, slug)
        }
        _ => (None, None),
    }
}

fn radix_36(mut val: i64) -> String {
    if val == 0 {
        return "0".to_string();
    }
    const CHARSET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut digits = Vec::new();
    while val > 0 {
        let rem = (val % 36) as usize;
        digits.push(CHARSET[rem]);
        val /= 36;
    }
    digits.reverse();
    String::from_utf8(digits).unwrap_or_default()
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let mut force = false;
    let mut input_file: Option<PathBuf> = None;
    let mut output_dir = PathBuf::from(".");
    let mut limit: Option<usize> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--force" | "-f" => force = true,
            "--input" | "-i" => {
                i += 1;
                if i < args.len() {
                    input_file = Some(PathBuf::from(&args[i]));
                }
            }
            "--output-dir" | "-o" => {
                i += 1;
                if i < args.len() {
                    output_dir = PathBuf::from(&args[i]);
                }
            }
            "--limit" | "-l" => {
                i += 1;
                if i < args.len() {
                    limit = args[i].parse().ok();
                }
            }
            _ => {}
        }
        i += 1;
    }

    fs::create_dir_all(&output_dir)?;

    println!("Checking MangaBaka database timestamp...");
    let remote_timestamp = ureq::get(TIMESTAMP_URL)
        .set("User-Agent", USER_AGENT)
        .call()
        .context("Failed to fetch remote timestamp")?
        .into_string()
        .unwrap_or_default()
        .trim()
        .to_string();

    let local_timestamp_path = output_dir.join("timestamp.txt");
    let local_timestamp = fs::read_to_string(&local_timestamp_path)
        .unwrap_or_default()
        .trim()
        .to_string();

    if !force && input_file.is_none() && !remote_timestamp.is_empty() && remote_timestamp == local_timestamp {
        println!("Database is already up-to-date (timestamp: {}). Use --force to regenerate.", remote_timestamp);
        return Ok(());
    }

    println!("Remote dump timestamp: {}", remote_timestamp);

    let reader: Box<dyn BufRead> = if let Some(path) = input_file {
        println!("Reading from local file: {:?}", path);
        let file = File::open(&path)?;
        if path.extension().and_then(|s| s.to_str()) == Some("zst") {
            let decoder = zstd::stream::read::Decoder::new(file)?;
            Box::new(BufReader::with_capacity(1024 * 1024, decoder))
        } else {
            Box::new(BufReader::with_capacity(1024 * 1024, file))
        }
    } else {
        println!("Streaming dump from: {}", DUMP_URL);
        let resp = ureq::get(DUMP_URL)
            .set("User-Agent", USER_AGENT)
            .call()
            .context("Failed to initiate dump download")?;
        let decoder = zstd::stream::read::Decoder::new(resp.into_reader())?;
        Box::new(BufReader::with_capacity(1024 * 1024, decoder))
    };

    println!("Parsing series and compiling cross-service mappings...");
    let mut items: Vec<MangaMappingItem> = Vec::new();
    let mut total_records: usize = 0;

    for line_res in reader.lines() {
        let line = line_res?;
        if line.trim().is_empty() {
            continue;
        }
        total_records += 1;

        if let Ok(rec) = serde_json::from_str::<MangaBakaRecord>(&line) {
            let sources = rec.source.unwrap_or_default();

            let anilist_id = parse_i64(sources.get("anilist").and_then(|s| s.id.as_ref()));
            let mal_id = parse_i64(sources.get("my_anime_list").and_then(|s| s.id.as_ref()));
            let (mangaupdates_id, mangaupdates_slug) = parse_base36(sources.get("manga_updates").and_then(|s| s.id.as_ref()));
            let kitsu_id = parse_i64(sources.get("kitsu").and_then(|s| s.id.as_ref()));
            let anime_planet_id = parse_string(sources.get("anime_planet").and_then(|s| s.id.as_ref()));
            let anime_news_network_id = parse_i64(sources.get("anime_news_network").and_then(|s| s.id.as_ref()));
            let shikimori_id = parse_i64(sources.get("shikimori").and_then(|s| s.id.as_ref()));

            let has_external = anilist_id.is_some()
                || mal_id.is_some()
                || mangaupdates_id.is_some()
                || kitsu_id.is_some()
                || anime_planet_id.is_some()
                || anime_news_network_id.is_some()
                || shikimori_id.is_some();

            if has_external {
                items.push(MangaMappingItem {
                    mangabaka_id: rec.id,
                    title: rec.title,
                    media_type: rec.media_type,
                    anilist_id,
                    mal_id,
                    mangaupdates_id,
                    mangaupdates_slug,
                    kitsu_id,
                    anime_planet_id,
                    anime_news_network_id,
                    shikimori_id,
                });
            }
        }

        if let Some(lim) = limit {
            if items.len() >= lim {
                break;
            }
        }

        if total_records % 50_000 == 0 {
            println!("Processed {} records... (mapped: {})", total_records, items.len());
        }
    }

    println!("Total processed: {}, Mapped series: {}", total_records, items.len());

    // Write manga-list-mini.json
    let mini_path = output_dir.join("manga-list-mini.json");
    println!("Writing {:?}...", mini_path);
    let mini_file = File::create(&mini_path)?;
    serde_json::to_writer(&mini_file, &items)?;

    // Write manga-list-mini.json.gz
    let gz_path = output_dir.join("manga-list-mini.json.gz");
    println!("Writing {:?}...", gz_path);
    let gz_file = File::create(&gz_path)?;
    let mut encoder = GzEncoder::new(gz_file, Compression::default());
    serde_json::to_writer(&mut encoder, &items)?;
    encoder.finish()?;

    // Write compact flat table: manga-mapping-flat.json
    let flat_path = output_dir.join("manga-mapping-flat.json");
    println!("Writing {:?}...", flat_path);
    let columns = vec![
        "mangabaka_id".to_string(),
        "anilist_id".to_string(),
        "mal_id".to_string(),
        "mangaupdates_id".to_string(),
        "kitsu_id".to_string(),
        "anime_news_network_id".to_string(),
        "shikimori_id".to_string(),
    ];
    let rows: Vec<Vec<i64>> = items
        .iter()
        .map(|item| {
            vec![
                item.mangabaka_id,
                item.anilist_id.unwrap_or(0),
                item.mal_id.unwrap_or(0),
                item.mangaupdates_id.unwrap_or(0),
                item.kitsu_id.unwrap_or(0),
                item.anime_news_network_id.unwrap_or(0),
                item.shikimori_id.unwrap_or(0),
            ]
        })
        .collect();

    let flat = FlatMapping { columns, rows };
    let flat_file = File::create(&flat_path)?;
    serde_json::to_writer(&flat_file, &flat)?;

    // Save timestamp
    if !remote_timestamp.is_empty() {
        fs::write(&local_timestamp_path, format!("{}\n", remote_timestamp))?;
    }

    println!("Manga mappings generation completed successfully!");
    Ok(())
}
