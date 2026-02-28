# Daboyi

大博弈 — A grand strategy game set in **1356** (late Yuan dynasty), in the style of EU4/Victoria, built with Rust.

- **Backend**: actix-web + RocksDB
- **Frontend**: Bevy 0.15 (2D)
- **Communication**: WebSocket (JSON commands / bincode snapshots)
- **Map data**: EU5toGIS GeoPackage dataset (28,573 provinces worldwide)
- **UI language**: Chinese-first (rust-i18n, Simplified Chinese font)

## Prerequisites

- Rust toolchain (stable, 2021 edition or later)
- `clang` / `clang++` (required by RocksDB)
- `mold` linker (configured in `.cargo/config.toml`)
- EU5toGIS datasets — see [this Paradox forum post](https://forum.paradoxplaza.com/forum/threads/georeferenced-eu5-dataset-for-map-modding-via-gis.1903895/#post-31141035)
  - `datasets/` directory containing `locations.gpkg`, `ports.gpkg`, and `rivers.tif`
  - `06_pops_totals.txt` from the same release (province population data)
- A EU5 **text** save file (`.eu5` starting with `SAV`, **not** a ZIP archive) for province ownership

## Setup

### 1. Copy population data

```bash
cp /path/to/EU5toGIS/06_pops_totals.txt assets/pops.tsv
```

### 2. Extract province ownership from save

```bash
cargo run -p parse_save -- /path/to/your.eu5 assets/ownership.tsv
```

Reads `compatibility.locations` (province name list) and `locations.locations` (province → owner ID mapping) from the save file, then writes `assets/ownership.tsv`. See `doc/province-ownership-extraction.md` for save file format details.

### 3. Generate map assets

```bash
cargo run --release -p mapgen
```

Reads `locations.gpkg` and `ports.gpkg` from the EU5toGIS `datasets/` directory (path hardcoded in `tools/mapgen/src/main.rs`), triangulates province polygons, and writes:

- `assets/map.bin` — playable province geometry + metadata (~80 MB)
- `assets/terrain.bin` — non-playable terrain/water polygons for background rendering (~40 MB)

Run with `--release` — debug mode is significantly slower.

### 4. Start the server

```bash
cargo run -p server
```

Listens on `ws://127.0.0.1:8080/ws`. On first run, loads `assets/map.bin`, `assets/ownership.tsv`, `assets/pops.tsv`, `assets/province_names.tsv`, and `assets/country_names.tsv` to generate the world state, then persists it in `daboyi.db/` (RocksDB). Subsequent runs load from the database.

### 5. Start the client

```bash
cargo run -p client
```

Connects to the server and renders the map. Run in a separate terminal while the server is running.

## Controls

| Input | Action |
|-------|--------|
| Right-click drag | Pan camera |
| Scroll wheel | Zoom |
| Left-click | Select province |
| `1` | Province map mode (EU5 identification colors) |
| `2` | Population map mode (heat map) |
| `3` | Production map mode (heat map) |
| `4` | Terrain map mode |
| `5` | Political map mode (country ownership) |
| Space | Pause / unpause simulation |

## Project Structure

```
daboyi/
├── shared/          # Common types (GameState, Province, Pop, Good, MapData)
├── server/          # actix-web server, RocksDB persistence, game simulation
├── client/          # Bevy frontend, WebSocket client, map + terrain rendering
├── assets/          # Generated/data assets (see below)
├── locales/         # rust-i18n locale files (zh.yml, en.yml)
├── doc/             # Design and technical documentation (Chinese)
└── tools/
    ├── mapgen/      # GeoPackage → assets/map.bin + assets/terrain.bin
    └── parse_save/  # EU5 text save → assets/ownership.tsv
```

### `assets/` directory

| File | Source | Purpose |
|------|--------|---------|
| `map.bin` | `cargo run -p mapgen` | Playable province geometry (~80 MB) |
| `terrain.bin` | `cargo run -p mapgen` | Non-playable terrain polygons (~40 MB) |
| `rivers.png` | EU5toGIS `datasets/rivers.tif` | River overlay sprite |
| `ownership.tsv` | `cargo run -p parse_save` | Province → owner tag mapping |
| `pops.tsv` | EU5toGIS `06_pops_totals.txt` | Province population counts |
| `province_names.tsv` | qwen batch translation | Province tag → Chinese name |
| `country_names.tsv` | qwen batch translation | Country tag → Chinese name |
| `fonts/NotoSansCJKsc-Regular.otf` | System font | Simplified Chinese rendering |
