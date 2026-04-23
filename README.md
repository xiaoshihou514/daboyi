# Daboyi Map Editor

A standalone **alternative-history map editor** built with Bevy and Rust. Paint EU5 provinces with custom countries and administrative areas, then export your coloring as JSON.

## Tech Stack

| Layer | Choice | Notes |
|-------|--------|-------|
| Engine | Bevy 0.15 | ECS game engine, 2D rendering |
| Language | Rust stable (2021 edition) | |
| Linker | mold + clang | configured in `.cargo/config.toml` |
| Serialization | serde + JSON / bincode | coloring files in JSON; map geometry in bincode |
| Font | NotoSansCJKsc-Regular.otf | Chinese text rendering |
| Map data | EU5toGIS GeoPackage | ~22,688 provinces worldwide |

## Prerequisites

- Rust toolchain (stable, 2021 edition or later)
- `clang` / `clang++` (required for some dependencies)
- `mold` linker (configured in `.cargo/config.toml`)
- [Trunk](https://trunkrs.dev/) (for the local/browser build)
- [EU5toGIS dataset](https://forum.paradoxplaza.com/forum/threads/georeferenced-eu5-dataset-for-map-modding-via-gis.1903895/) — provides `datasets/locations.gpkg` and `ports.gpkg`
- An EU5 **plain-text** save file (`.eu5`, starts with `SAV`, not a ZIP archive) — for extracting province ownership and country colors

## Asset Setup

Assets are generated offline and are not committed to the repository.

Python helper scripts under `tools/` are managed with **uv**:

```bash
uv sync --project tools
```

### 1. Generate map geometry

```bash
cargo run --release -p mapgen
```

Reads `locations.gpkg` and `ports.gpkg` from the EU5toGIS `datasets/` directory, triangulates province polygons, and writes:

- `assets/map.bin` — playable province geometry (~80 MB)
- `assets/terrain.bin` — wasteland/ocean background polygons (~40 MB)

> Use `--release`; debug mode is very slow for geometry processing.

### 1.5. Precompute terrain adjacency cache

```bash
cargo run --release -p terrain_cache
```

Builds `assets/terrain_adjacency.bin`, the deterministic terrain/province adjacency cache used by the editor for wasteland ownership tinting and terrain-border rendering.

### 2. Extract EU5 save data

```bash
cargo run -p parse_save -- /path/to/your.eu5 \
    assets/ownership.tsv \
    assets/vassals.tsv \
    assets/merchandize.tsv \
    assets/country_colors.tsv
```

Outputs:

- `assets/ownership.tsv` — province tag → owner country tag
- `assets/vassals.tsv` — vassal tag → overlord tag
- `assets/merchandize.tsv` — country tag + goods + output
- `assets/country_colors.tsv` — country tag → RGB color (0–255)

### 3. (Optional) Copy population data

```bash
cp /path/to/EU5toGIS/06_pops_totals.txt assets/pops.tsv
```

Population data is loaded but not actively used in the current editor version.

### 4. Generate Python-managed helper assets

```bash
uv run --project tools python tools/extract_translations.py
uv run --project tools python tools/extract_rivers_vector.py
```

## Running the Editor

```bash
cargo run -p client
```

The editor opens directly; no server is needed.

Common shortcuts are also available through the root `Makefile`:

```bash
make help
make client
make test
make web-build
```

## Running the Web Build Locally

Install the wasm target and Trunk once:

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
```

Then launch the browser version from the dedicated frontend folder:

```bash
make web-serve
```

Trunk serves the `web/` frontend, compiles `client` to `wasm32-unknown-unknown`, and exposes the generated `assets/` directory to the browser. This expects `assets/map.bin`, `assets/terrain.bin`, and other generated files to already exist locally.

You can also produce a distributable bundle without starting a dev server:

```bash
make web-build
```

## Editor Usage

**Map display:**

The editor now uses a single blended map view that combines terrain shading with political ownership. Province borders fade in smoothly as you zoom, while country borders remain visually stronger than sub-country borders.

**Controls:**

| Input | Action |
|-------|--------|
| Middle-click drag | Pan camera |
| Scroll wheel | Zoom |
| Left-click | Select province |
| Left-click drag | Brush-paint provinces |

**Painting workflow:**

1. Select a country or administrative area in the left panel.
2. Left-click or drag on the map to paint provinces.
3. Click **保存** (Save) to write `coloring.json`; click **加载** (Load) to reload.

**Administrative areas:**

Countries support an unlimited hierarchy of sub-areas (ADM1 → ADM2 → ADM3 …). Each area can have its own color or inherit from its parent. Painting a province to an area overrides the country-level assignment for rendering.

**Browser build note:**

The web build renders the same map data and uses the same wasm client binary, but local file save/load dialogs are currently desktop-only. Browser testing is intended for rendering and editor interaction against served `assets/`.

## Coloring File Format

The editor saves to `coloring.json`:

```json
{
  "countries": [
    { "tag": "MNG", "name": "蒙古", "color": [0.8, 0.2, 0.1, 1.0], "capital_province": 1234 }
  ],
  "assignments": { "1234": "MNG", "5678": "MNG" },
  "admin_areas": [
    { "id": 1, "name": "漠北", "country_tag": "MNG", "parent_id": null, "color": null }
  ],
  "admin_assignments": { "9999": 1 }
}
```

- `assignments` — maps province ID → country tag
- `admin_assignments` — maps province ID → admin area ID (overrides country color in rendering)
- `admin_areas` — supports `parent_id` for nested hierarchies; `color: null` inherits from parent/country

## Project Structure

```
daboyi/
├── shared/          # Shared types: EditorCountry, AdminArea, ColoringFile, MapData
├── client/          # Bevy editor application
│   └── src/
│       ├── main.rs         # App entry, resource initialization
│       ├── state.rs        # AppState enum (reserved for future use)
│       ├── editor.rs       # MapColoring, EditorCountries, AdminAreas resources + save/load
│       ├── terrain.rs      # TerrainPlugin: wasteland/ocean rendering
│       ├── capitals.rs     # CapitalsPlugin: capital star markers
│       ├── map/
│       │   ├── mod.rs      # MapPlugin: province mesh rendering, coloring logic
│       │   ├── borders.rs  # BordersPlugin: borders between owners/areas
│       │   └── interact.rs # Camera pan/zoom, province selection, brush paint
│       └── ui/
│           └── mod.rs      # UiPlugin: left panel, right info panel, toolbar
├── assets/          # Generated/data assets (see table below)
├── doc/             # Technical documentation (Chinese)
└── tools/
    ├── mapgen/      # GeoPackage → assets/map.bin + assets/terrain.bin
    └── parse_save/  # EU5 text save → TSV asset files
```

### `assets/` Reference

| File | Source | Purpose |
|------|--------|---------|
| `map.bin` | `cargo run -p mapgen` | Province geometry (~80 MB, gitignored) |
| `terrain.bin` | `cargo run -p mapgen` | Wasteland/ocean geometry (~40 MB, gitignored) |
| `terrain_adjacency.bin` | `cargo run -p terrain_cache` | Terrain/province adjacency cache for editor rendering |
| `ownership.tsv` | `cargo run -p parse_save` | Province tag → owner country tag |
| `vassals.tsv` | `cargo run -p parse_save` | Vassal tag → overlord tag |
| `merchandize.tsv` | `cargo run -p parse_save` | Country tag + goods output |
| `country_colors.tsv` | `cargo run -p parse_save` | Country tag → RGB color |
| `pops.tsv` | EU5toGIS `06_pops_totals.txt` | Province population (future use) |
| `province_names.tsv` | extracted from EU5 localisation | Province tag → Chinese name |
| `country_names.tsv` | extracted from EU5 localisation | Country tag → Chinese name |
| `fonts/NotoSansCJKsc-Regular.otf` | system font | Chinese text rendering |
