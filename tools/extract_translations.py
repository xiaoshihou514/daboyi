#!/usr/bin/env python3
"""
Extract Chinese (Simplified) translations from EU5 localisation files and
write assets/country_names.tsv and assets/province_names.tsv.

Data source: EU5toGIS/localization/simp_chinese/ (copied from the EU5 Steam
installation; see doc/数据提取.md for the full pipeline).

Run from repo root:
    python3 tools/extract_translations.py

Dependencies: standard library only (no third-party packages required).
"""

import re
import sys
from pathlib import Path

# ── Paths ─────────────────────────────────────────────────────────────────────
REPO_ROOT   = Path(__file__).resolve().parent.parent
EU5GIS      = REPO_ROOT.parent / "EU5toGIS"
LOC_DIR     = EU5GIS / "localization" / "simp_chinese"

COUNTRY_YML   = LOC_DIR / "country_names_l_simp_chinese.yml"
LOCATION_DIR  = LOC_DIR / "location_names"
LOCATION_MAIN = LOCATION_DIR / "location_names_l_simp_chinese.yml"

OUT_COUNTRY   = REPO_ROOT / "assets" / "country_names.tsv"
OUT_PROVINCE  = REPO_ROOT / "assets" / "province_names.tsv"

# ── Parser ────────────────────────────────────────────────────────────────────
KEY_RE = re.compile(r'^\s+([A-Za-z0-9_]+):\s+"([^"]*)"')

def parse_yml(path: Path) -> dict[str, str]:
    """
    Parse a Paradox localisation YML file and return {key: value} mapping.
    Lines starting with 'l_simp_chinese:' are skipped (header).
    Keys with _ADJ / _THE / _LONG suffixes are skipped.
    Empty or placeholder values are skipped.
    """
    entries: dict[str, str] = {}
    skip_suffixes = ("_ADJ", "_THE", "_LONG")
    skip_values   = {"", "（占位符）", "$common_string_prefix_article$"}
    try:
        text = path.read_text(encoding="utf-8-sig", errors="replace")
    except FileNotFoundError:
        return entries
    for line in text.splitlines():
        m = KEY_RE.match(line)
        if not m:
            continue
        key, value = m.group(1), m.group(2)
        if any(key.endswith(s) for s in skip_suffixes):
            continue
        if value in skip_values:
            continue
        entries[key] = value
    return entries


# ── Country names ─────────────────────────────────────────────────────────────

def extract_country_names() -> dict[str, str]:
    """
    Parse country_names_l_simp_chinese.yml.
    Keys are 3-letter country tags (e.g. SWE, CHN).
    """
    if not COUNTRY_YML.exists():
        print(f"Error: {COUNTRY_YML} not found.", file=sys.stderr)
        print("Run: copy <EU5>/game/main_menu/localization/simp_chinese/country_names_l_simp_chinese.yml"
              " → EU5toGIS/localization/simp_chinese/", file=sys.stderr)
        return {}
    raw = parse_yml(COUNTRY_YML)
    # Only keep keys that look like 3-letter tags (all-caps or all-caps+digits)
    return {k: v for k, v in raw.items() if re.match(r'^[A-Z0-9]{2,5}$', k)}


# ── Province / location names ─────────────────────────────────────────────────

def extract_province_names() -> dict[str, str]:
    """
    Parse all location_names_*_l_simp_chinese.yml files with priority:
      1. Other language-specific files (lowest priority)
      2. Main location_names_l_simp_chinese.yml
      3. Mandarin-specific file (highest — most accurate for Chinese regions)

    Province keys in the main file are bare tags (e.g. 'hangzhou').
    Language-specific files use 'tag.language_suffix' format; only the tag
    part is kept when merging.
    """
    if not LOCATION_DIR.exists():
        print(f"Error: {LOCATION_DIR} not found.", file=sys.stderr)
        return {}

    names: dict[str, str] = {}

    # 1. Load all language-specific files (lowest priority first)
    for yml_path in sorted(LOCATION_DIR.glob("*.yml")):
        if yml_path == LOCATION_MAIN:
            continue  # handled separately below
        raw = parse_yml(yml_path)
        for key, value in raw.items():
            # Strip language qualifier: 'hangzhou.mandarin_chinese' → 'hangzhou'
            tag = key.split(".")[0]
            names[tag] = value

    # 2. Load main file (overrides language-specific)
    if LOCATION_MAIN.exists():
        raw = parse_yml(LOCATION_MAIN)
        for key, value in raw.items():
            tag = key.split(".")[0]
            names[tag] = value

    # 3. Re-apply mandarin-specific file at highest priority
    mandarin_file = LOCATION_DIR / "location_names_mandarin_chinese_l_simp_chinese.yml"
    if mandarin_file.exists():
        raw = parse_yml(mandarin_file)
        for key, value in raw.items():
            tag = key.split(".")[0]
            names[tag] = value

    return names


# ── Write TSV ─────────────────────────────────────────────────────────────────

def write_tsv(path: Path, data: dict[str, str], label: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [f"{k}\t{v}" for k, v in sorted(data.items())]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"Wrote {len(lines):,} {label} → {path}")


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    # Check EU5toGIS exists
    if not EU5GIS.exists():
        print(f"Error: {EU5GIS} not found.", file=sys.stderr)
        print("Clone or create the EU5toGIS directory at the sibling path.", file=sys.stderr)
        sys.exit(1)

    # Country names
    countries = extract_country_names()
    if countries:
        write_tsv(OUT_COUNTRY, countries, "country names")
    else:
        print("Warning: no country names extracted.", file=sys.stderr)

    # Province names
    provinces = extract_province_names()
    if provinces:
        write_tsv(OUT_PROVINCE, provinces, "province names")
    else:
        print("Warning: no province names extracted.", file=sys.stderr)

    print("Done.")


if __name__ == "__main__":
    main()
