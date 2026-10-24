# dartotsu_manga_mapper

Cross-service manga, manhwa, manhua, and light novel ID mapping generator for [Dartotsu](https://github.com/aayush2622/Dartotsu), powered by daily database dumps from [MangaBaka](https://mangabaka.org).

Analogous to [Fribb/anime-lists](https://github.com/Fribb/anime-lists) for anime, this repository compiles and maintains mapping datasets across all major tracking and catalog services for manga and light novels.

---

## 🎯 Supported Providers

Each entry connects the series across:

| Provider | Key in JSON | ID Type | Description / Conversion |
| :--- | :--- | :--- | :--- |
| **MangaBaka** | `mangabaka_id` | `int` | MangaBaka canonical series ID |
| **AniList** | `anilist_id` | `int` | AniList manga/novel ID |
| **MyAnimeList** | `mal_id` | `int` | MyAnimeList manga ID |
| **MangaUpdates** | `mangaupdates_id` | `int64` | Numeric v1 API series ID (e.g. `60085561294`) |
| **MangaUpdates (Slug)**| `mangaupdates_slug`| `string` | Base36 URL slug (e.g. `"rlpe3ta"` $\rightarrow 60085561294$) |
| **Kitsu** | `kitsu_id` | `int` | Kitsu media ID |
| **Anime-Planet** | `anime_planet_id` | `string` | Anime-Planet URL slug |
| **Anime News Network** | `anime_news_network_id` | `int` | ANN encyclopedia ID |
| **Shikimori** | `shikimori_id` | `int` | Shikimori media ID |

---

## 📦 Generated Datasets

### 1. `manga-list-mini.json`
Compact JSON array containing all indexed series with at least two mapped services.
* **Direct URL:**
  ```text
  https://raw.githubusercontent.com/itsmechinmoy/dartotsu_manga_mapper/main/manga-list-mini.json
  ```
* **Sample Entry:**
  ```json
  {
    "mangabaka_id": 1,
    "anilist_id": 85208,
    "mal_id": 147443,
    "mangaupdates_id": 60085561294,
    "mangaupdates_slug": "rlpe3ta",
    "kitsu_id": 35879,
    "anime_planet_id": "dice-the-cube-that-changes-everything",
    "shikimori_id": 147443,
    "title": "DICE",
    "type": "manhwa"
  }
  ```

### 2. `manga-mapping-flat.json`
Fast flat matrix representation designed for ultra-low memory footprints and instant zero-overhead parsing on mobile devices.
* **Direct URL:**
  ```text
  https://raw.githubusercontent.com/itsmechinmoy/dartotsu_manga_mapper/main/manga-mapping-flat.json
  ```

---

## ⚡ Automated Updates

A GitHub Actions cron job runs daily at **03:00 UTC** (shortly after MangaBaka publishes its daily database dump):
1. Verifies the dump timestamp against `timestamp.txt`.
2. Downloads and decompresses `series.jsonl.zst`.
3. Compiles the cross-service mappings in Rust.
4. Automatically commits and updates the datasets on the `main` branch.

---

## 🛠 Local Usage

### Running with Rust
```bash
cargo run --release
```

Options:
* `--force` (`-f`): Rebuild even if the remote timestamp matches `timestamp.txt`.
* `--input <path>` (`-i`): Use a local dump file (`.jsonl` or `.jsonl.zst`).
* `--limit <n>` (`-l`): Limit to the first $N$ entries for testing.

### Running with Python
```bash
python scripts/generate.py
```
