# Daboyi

A grand strategy game in the style of EU4/Victoria, built with Rust.

- **Backend**: actix-web + RocksDB
- **Frontend**: Bevy (2D)
- **Communication**: WebSocket (JSON)
- **Map data**: Prepackaged GeoJSON (CN ADM3 + World ADM2, ~50K provinces)

## Prerequisites

- Rust toolchain (rustc 1.89+)
- clang / clang++ (for RocksDB)
- mold linker
- Prepackaged GeoJSON data at `/home/xiaoshihou/Playground/shared/geojson/`

## Quick Start

### 1. Generate map asset

```bash
cargo run -p mapgen
# or specify a custom geojson directory:
cargo run -p mapgen -- /path/to/geojson assets/map.bin
```

Reads `cn_adm3.geojson` (China, prioritized) and `world_adm2.geojson` (rest of world), simplifies/triangulates polygons, and writes `assets/map.bin`.

### 2. Start the server

```bash
cargo run -p server
```

Listens on `ws://127.0.0.1:8080/ws`. Loads `assets/map.bin` for world generation on first run, then persists state in `daboyi.db` (RocksDB).

### 3. Start the client (separate terminal)

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
    └── mapgen/      # Rust binary — GeoJSON → assets/map.bin
```
