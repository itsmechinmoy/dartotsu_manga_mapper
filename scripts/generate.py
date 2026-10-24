#!/usr/bin/env python3
"""
Manga & Light Novel cross-service mapping generator for Dartotsu.
Streams daily database dumps from MangaBaka (https://mangabaka.org),
extracts cross-service identifiers, and outputs optimized JSON mapping files.
"""

import argparse
import gzip
import json
import os
import sys
import time
import requests
import zstandard

TIMESTAMP_URL = "https://api.mangabaka.org/v1/database/series.timestamp.txt"
DUMP_URL = "https://api.mangabaka.org/v1/database/series.jsonl.zst"
USER_AGENT = "Dartotsu/1.0 (dartotsu_manga_mapper; +https://github.com/itsmechinmoy/dartotsu_manga_mapper)"

def parse_base36(val):
    if not val:
        return None, None
    if isinstance(val, int):
        # convert int to base36 slug
        digits = "0123456789abcdefghijklmnopqrstuvwxyz"
        n = val
        if n == 0:
            return 0, "0"
        res = []
        while n > 0:
            res.append(digits[n % 36])
            n //= 36
        return val, "".join(reversed(res))
    if isinstance(val, str) and val.strip():
        slug = val.strip().lower()
        try:
            return int(slug, 36), slug
        except ValueError:
            return None, slug
    return None, None

def parse_int(val):
    if val is None:
        return None
    try:
        return int(val)
    except (ValueError, TypeError):
        return None

def parse_str(val):
    if val is None:
        return None
    s = str(val).strip()
    return s if s else None

def main():
    parser = argparse.ArgumentParser(description="Generate MangaBaka cross-service mappings")
    parser.add_argument("--force", "-f", action="store_true", help="Force rebuild even if timestamp matches")
    parser.add_argument("--input", "-i", type=str, default=None, help="Local dump file (.jsonl or .jsonl.zst)")
    parser.add_argument("--output-dir", "-o", type=str, default=".", help="Output directory")
    parser.add_argument("--limit", "-l", type=int, default=None, help="Limit number of mapped entries (for testing)")
    args = parser.parse_args()

    os.makedirs(args.output_dir, exist_ok=True)
    local_ts_path = os.path.join(args.output_dir, "timestamp.txt")

    print("Checking MangaBaka database timestamp...")
    try:
        ts_resp = requests.get(TIMESTAMP_URL, headers={"User-Agent": USER_AGENT}, timeout=15)
        remote_ts = ts_resp.text.strip()
    except Exception as e:
        print(f"Warning: Could not fetch remote timestamp: {e}")
        remote_ts = ""

    local_ts = ""
    if os.path.exists(local_ts_path):
        with open(local_ts_path, "r", encoding="utf-8") as f:
            local_ts = f.read().strip()

    if not args.force and not args.input and remote_ts and remote_ts == local_ts:
        print(f"Database is already up-to-date (timestamp: {remote_ts}). Use --force to regenerate.")
        return

    print(f"Remote dump timestamp: {remote_ts}")
    start_time = time.time()

    if args.input:
        print(f"Reading from local file: {args.input}")
        raw_file = open(args.input, "rb")
        if args.input.endswith(".zst"):
            dctx = zstandard.ZstdDecompressor()
            stream = dctx.stream_reader(raw_file)
        else:
            stream = raw_file
    else:
        print(f"Streaming dump from {DUMP_URL}...")
        resp = requests.get(DUMP_URL, headers={"User-Agent": USER_AGENT}, stream=True, timeout=60)
        resp.raise_for_status()
        dctx = zstandard.ZstdDecompressor()
        stream = dctx.stream_reader(resp.raw)

    items = []
    total = 0
    buffer = b""

    try:
        while True:
            chunk = stream.read(262144)
            if not chunk:
                break
            buffer += chunk
            while b"\n" in buffer:
                line, buffer = buffer.split(b"\n", 1)
                line = line.strip()
                if not line:
                    continue
                total += 1
                try:
                    rec = json.loads(line.decode("utf-8"))
                except Exception:
                    continue

                sources = rec.get("source") or {}

                anilist_id = parse_int(sources.get("anilist", {}).get("id"))
                mal_id = parse_int(sources.get("my_anime_list", {}).get("id"))
                mu_id, mu_slug = parse_base36(sources.get("manga_updates", {}).get("id"))
                kitsu_id = parse_int(sources.get("kitsu", {}).get("id"))
                ap_id = parse_str(sources.get("anime_planet", {}).get("id"))
                ann_id = parse_int(sources.get("anime_news_network", {}).get("id"))
                shikimori_id = parse_int(sources.get("shikimori", {}).get("id"))

                has_external = any(x is not None for x in [
                    anilist_id, mal_id, mu_id, kitsu_id, ap_id, ann_id, shikimori_id
                ])

                if has_external:
                    entry = {
                        "mangabaka_id": rec.get("id"),
                    }
                    if anilist_id is not None:
                        entry["anilist_id"] = anilist_id
                    if mal_id is not None:
                        entry["mal_id"] = mal_id
                    if mu_id is not None:
                        entry["mangaupdates_id"] = mu_id
                    if mu_slug is not None:
                        entry["mangaupdates_slug"] = mu_slug
                    if kitsu_id is not None:
                        entry["kitsu_id"] = kitsu_id
                    if ap_id is not None:
                        entry["anime_planet_id"] = ap_id
                    if ann_id is not None:
                        entry["anime_news_network_id"] = ann_id
                    if shikimori_id is not None:
                        entry["shikimori_id"] = shikimori_id
                    if rec.get("title"):
                        entry["title"] = rec["title"]
                    if rec.get("type"):
                        entry["type"] = rec["type"]

                    items.append(entry)

                if args.limit and len(items) >= args.limit:
                    break

                if total % 50000 == 0:
                    print(f"Processed {total} records... (mapped: {len(items)})")
            if args.limit and len(items) >= args.limit:
                break
    finally:
        try:
            stream.close()
        except Exception:
            pass

    elapsed = time.time() - start_time
    print(f"Finished parsing in {elapsed:.2f}s. Total: {total}, Mapped: {len(items)}")

    # Write manga-list-mini.json
    mini_path = os.path.join(args.output_dir, "manga-list-mini.json")
    print(f"Writing {mini_path}...")
    with open(mini_path, "w", encoding="utf-8") as f:
        json.dump(items, f, separators=(",", ":"))

    # Write manga-list-mini.json.gz
    gz_path = os.path.join(args.output_dir, "manga-list-mini.json.gz")
    print(f"Writing {gz_path}...")
    with gzip.open(gz_path, "wt", encoding="utf-8") as f:
        json.dump(items, f, separators=(",", ":"))

    # Write compact flat format: manga-mapping-flat.json
    flat_path = os.path.join(args.output_dir, "manga-mapping-flat.json")
    print(f"Writing {flat_path}...")
    columns = [
        "mangabaka_id",
        "anilist_id",
        "mal_id",
        "mangaupdates_id",
        "kitsu_id",
        "anime_news_network_id",
        "shikimori_id",
    ]
    rows = []
    for item in items:
        rows.append([
            item.get("mangabaka_id", 0),
            item.get("anilist_id", 0),
            item.get("mal_id", 0),
            item.get("mangaupdates_id", 0),
            item.get("kitsu_id", 0),
            item.get("anime_news_network_id", 0),
            item.get("shikimori_id", 0),
        ])

    with open(flat_path, "w", encoding="utf-8") as f:
        json.dump({"columns": columns, "rows": rows}, f, separators=(",", ":"))

    # Write timestamp
    if remote_ts:
        with open(local_ts_path, "w", encoding="utf-8") as f:
            f.write(f"{remote_ts}\n")

    print("Generation complete!")

if __name__ == "__main__":
    main()
