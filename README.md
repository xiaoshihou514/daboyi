# Daboyi

A grand strategy game set in 1337, in the style of EU4/Victoria, built with Rust.

- **Backend**: actix-web + RocksDB
- **Frontend**: Bevy (2D)
- **Communication**: WebSocket (JSON)
- **Map data**: EU5toGIS GeoPackage dataset (28,573 provinces worldwide)

## Prerequisites

- Rust toolchain (stable, 2021 edition or later)
- `clang` / `clang++` (required by RocksDB)
- `mold` linker (configured in `.cargo/config.toml`)
- EU5toGIS datasets — see [this Paradox forum post](https://forum.paradoxplaza.com/forum/threads/georeferenced-eu5-dataset-for-map-modding-via-gis.1903895/#post-31141035)
  - You need the `datasets/` directory containing `locations.gpkg` and `ports.gpkg`
- A EU5 text save file (`.eu5` starting with `SAV`, not a ZIP) for province ownership data

## Setup

### 1. Extract province ownership from a save file

```bash
cargo run -p parse_save -- /path/to/your.eu5 assets/ownership.tsv
```

This reads the save's `compatibility.locations` section to map province IDs to names, then reads the `locations.locations` section for ownership, and writes a TSV to `assets/ownership.tsv`.

See `doc/province-ownership-extraction.md` for details on the save file format.

### 2. Generate map assets

```bash
cargo run --release -p mapgen -- /path/to/EU5toGIS/datasets
```

Reads `locations.gpkg` and `ports.gpkg` from the given directory, triangulates province polygons, and writes:
- `assets/map.bin` — playable province geometry + metadata (~80 MB)
- `assets/terrain.bin` — terrain/water polygons for background rendering (~40 MB)

Run with `--release` — debug mode is significantly slower.

### 3. Start the server

```bash
cargo run -p server
```

Listens on `ws://127.0.0.1:8080/ws`. On first run, loads `assets/map.bin` and `assets/ownership.tsv` to generate the world state, then persists it in `daboyi.db/` (RocksDB). Subsequent runs load from the database.

### 4. Start the client

```bash
cargo run -p client
```

Connects to the server and renders the map. Open a second terminal while the server is running.

## Controls

| Input | Action |
|-------|--------|
| Right-click drag | Pan camera |
| Scroll wheel | Zoom |
| Left-click | Select province |
| `1` | Province map mode (EU5 identification colors) |
| `2` | Population map mode |
| `3` | Production map mode |
| `4` | Terrain map mode |
| `5` | Political map mode (country ownership) |
| Space | Pause / unpause simulation |

## Project Structure

```
daboyi/
├── shared/          # Common types (GameState, Province, Pop, Good, MapData)
├── server/          # actix-web server, RocksDB persistence, game simulation
├── client/          # Bevy frontend, WebSocket client, map rendering
├── assets/          # Generated assets (map.bin, terrain.bin, ownership.tsv)
├── doc/             # Design and technical documentation
└── tools/
    ├── mapgen/      # GeoPackage → assets/map.bin + assets/terrain.bin
    └── parse_save/  # EU5 text save → assets/ownership.tsv
```
