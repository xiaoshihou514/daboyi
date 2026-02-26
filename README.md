# Daboyi

A grand strategy game in the style of EU4/Victoria, built with Rust.

- **Backend**: actix-web + RocksDB
- **Frontend**: Bevy (2D)
- **Communication**: WebSocket (JSON)
- **Map data**: GADM 4.1 administrative boundaries (~42K provinces)

## Prerequisites

- Rust toolchain (rustc 1.89+)
- clang / clang++ (for RocksDB)
- mold linker
- Python 3.12+ with [uv](https://github.com/astral-sh/uv)

## Quick Start

### 1. Download map data

```bash
cd tools/download_gadm
uv run python download_gadm.py          # all 254 countries (~500 MB)
# or pick specific countries:
uv run python download_gadm.py DEU FRA CHN USA GBR JPN RUS IND BRA AUS
```

### 2. Generate map asset

```bash
cargo run -p mapgen
```

This reads `raw_gadm/*.json`, simplifies/triangulates polygons, and writes `assets/map.bin`.

### 3. Start the server

```bash
cargo run -p server
```

Listens on `ws://127.0.0.1:8080/ws`. Loads `assets/map.bin` for world generation on first run, then persists state in `daboyi.db` (RocksDB).

### 4. Start the client (separate terminal)

```bash
cargo run -p client
```

Connects to the server and renders the map.

## Controls

| Input | Action |
|---|---|
| Right-click drag | Pan camera |
| Scroll wheel | Zoom |
| Left-click | Select province |
| `1` | Political map mode |
| `2` | Population map mode |
| `3` | Production map mode |

## Project Structure

```
daboyi/
├── shared/          # Common types (GameState, Province, Pop, Good, MapData)
├── server/          # actix-web server, RocksDB persistence, game simulation
├── client/          # Bevy frontend, WebSocket client, map rendering
└── tools/
    ├── download_gadm/   # uv project — downloads GADM GeoJSON files
    └── mapgen/          # Rust binary — GeoJSON → assets/map.bin
```
