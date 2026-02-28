# Province Ownership Extraction

Province ownership data is extracted from an EU5 text save file (`ti.eu5`) using the tool at `tools/parse_save`.

## How to Re-Extract

Edit the path constants at the top of `tools/parse_save/src/main.rs` if your save is in a different location, then run:

```sh
cargo run -p parse_save
```

This regenerates `assets/ownership.tsv`, which is read by the server at startup to assign province owners.

## ti.eu5 Format

`ti.eu5` is a plain-text EU5 save file (starts with the `SAV` header, not a ZIP archive). It contains three sections relevant to province ownership:

### 1. `compatibility.locations` — Province Name List

Located near the top of the file (around line 42):

```
compatibility={
    version=1
    locations_hash=...
    locations={stockholm norrtalje enkoping ... wuxian ...}
}
```

All 28,573 province names are listed space-separated in a single line. The position in this list (0-indexed) corresponds to the province's **GPKG_id** (1-based integer):

```
compat_names[0]  = "stockholm"   →  GPKG_id 1
compat_names[1]  = "norrtalje"   →  GPKG_id 2
...
compat_names[8618] = "wuxian"    →  GPKG_id 8619
```

These names exactly match the `tag` field in `datasets/locations.gpkg` and the `fid` (feature ID) in that GeoPackage.

### 2. `countries.tags` — Country ID → Tag

```
countries={
    tags={
        0=DUMMY
        1=PIR
        2=MER
        3=SWE
        ...
    }
    ...
}
```

Maps integer country IDs to 3-letter country tags (e.g. `SWE`, `ENG`, `CHI`).

### 3. `locations.locations` — Province → Owner

```
locations={
    locations={
        1={
            owner=3
            controller=3
            ...
        }
        2={
            owner=3
            ...
        }
        ...
    }
}
```

Each integer key is a **GPKG_id** (same as the position in the compat list above). The `owner` field is the integer country ID from the `countries.tags` section.

Only provinces with explicit ownership records appear here (~13,588 of 28,573 total). Provinces without a record are unowned wilderness.

## ID Alignment

All province ID systems are consistent:

| Layer | Province identifier |
|-------|-------------------|
| `datasets/locations.gpkg` | `fid` (integer, 1-based) = GPKG_id |
| `datasets/locations.gpkg` | `tag` (string, e.g. `"stockholm"`) |
| `ti.eu5` compat section | position i (0-indexed) → name; GPKG_id = i+1 |
| `ti.eu5` locations section | integer key = GPKG_id |
| `assets/ownership.tsv` | province `tag` string → 3-letter owner code |
| server `MapProvince.tag` | string, looked up in ownership map |

## Output Format

`assets/ownership.tsv` is a tab-separated file:

```
location_tag	owner_tag
aabenraa	SLV
aachen	AAC
...
stockholm	SWE
wuxian	CHI
```

The server reads this at startup in `server/src/game/data.rs → load_eu5_ownership()` and assigns province owners. Provinces not in the TSV are left unowned.
