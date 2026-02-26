#!/usr/bin/env python3
"""
Download GADM 4.1 GeoJSON files at the deepest available admin level per country.
Outputs to raw_gadm/ directory in the project root.

Usage:
    python3 tools/download_gadm.py              # download all countries
    python3 tools/download_gadm.py DEU FRA CHN  # download specific countries only

The country list and max levels are parsed from the GADM website.
"""

import os
import re
import sys
import time
import urllib.request
import zipfile
import io

BASE_URL = "https://geodata.ucdavis.edu/gadm/gadm4.1/json"
OUTPUT_DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "raw_gadm")

# Parsed from https://gadm.org/download_country.html
# Format: (ISO3, max_level)
COUNTRIES = [
    ("AFG", 3), ("XAD", 2), ("ALA", 3), ("ALB", 4), ("DZA", 3), ("ASM", 4),
    ("AND", 2), ("AGO", 4), ("AIA", 2), ("ATA", 1), ("ATG", 2), ("ARG", 3),
    ("ARM", 2), ("ABW", 1), ("AUS", 3), ("AUT", 5), ("AZE", 3), ("BHS", 2),
    ("BHR", 2), ("BGD", 5), ("BRB", 2), ("BLR", 3), ("BEL", 5), ("BLZ", 2),
    ("BEN", 4), ("BMU", 2), ("BTN", 3), ("BOL", 4), ("BES", 2), ("BIH", 4),
    ("BWA", 3), ("BVT", 1), ("BRA", 3), ("IOT", 1), ("VGB", 2), ("BRN", 3),
    ("BGR", 3), ("BFA", 4), ("BDI", 5), ("CPV", 2), ("KHM", 4), ("CMR", 4),
    ("CAN", 4), ("XCA", 1), ("CYM", 2), ("CAF", 3), ("TCD", 4), ("CHL", 4),
    ("CHN", 4), ("CXR", 1), ("XCL", 1), ("CCK", 1), ("COL", 3), ("COM", 2),
    ("COG", 3), ("COK", 2), ("CRI", 4), ("CIV", 5), ("HRV", 3), ("CUB", 3),
    ("CUW", 1), ("CYP", 2), ("CZE", 3), ("COD", 3), ("DNK", 3), ("DJI", 3),
    ("DMA", 2), ("DOM", 3), ("TLS", 4), ("ECU", 4), ("EGY", 3), ("SLV", 3),
    ("GNQ", 3), ("ERI", 3), ("EST", 4), ("SWZ", 3), ("ETH", 4), ("FLK", 1),
    ("FRO", 3), ("FJI", 3), ("FIN", 5), ("FRA", 5), ("GUF", 3), ("PYF", 2),
    ("ATF", 2), ("GAB", 3), ("GMB", 3), ("GEO", 3), ("DEU", 5), ("GHA", 3),
    ("GIB", 1), ("GRC", 4), ("GRL", 2), ("GRD", 2), ("GLP", 3), ("GUM", 2),
    ("GTM", 3), ("GGY", 2), ("GIN", 4), ("GNB", 3), ("GUY", 3), ("HTI", 5),
    ("HMD", 1), ("HND", 3), ("HUN", 3), ("ISL", 3), ("IND", 4), ("IDN", 5),
    ("IRN", 3), ("IRQ", 3), ("IRL", 3), ("IMN", 2), ("ISR", 2), ("ITA", 4),
    ("JAM", 2), ("JPN", 3), ("JEY", 2), ("JOR", 3), ("KAZ", 3), ("KEN", 4),
    ("KIR", 1), ("XKO", 3), ("KWT", 2), ("KGZ", 3), ("LAO", 3), ("LVA", 3),
    ("LBN", 4), ("LSO", 2), ("LBR", 4), ("LBY", 2), ("LIE", 2), ("LTU", 3),
    ("LUX", 5), ("MKD", 2), ("MDG", 5), ("MWI", 4), ("MYS", 3), ("MDV", 1),
    ("MLI", 5), ("MLT", 3), ("MHL", 2), ("MTQ", 3), ("MRT", 3), ("MUS", 2),
    ("MYT", 2), ("MEX", 3), ("FSM", 3), ("MDA", 2), ("MCO", 1), ("MNG", 3),
    ("MNE", 2), ("MSR", 2), ("MAR", 5), ("MOZ", 4), ("MMR", 4), ("NAM", 3),
    ("NRU", 2), ("NPL", 5), ("NLD", 3), ("NCL", 3), ("NZL", 3), ("NIC", 3),
    ("NER", 4), ("NGA", 3), ("NIU", 1), ("NFK", 1), ("PRK", 3), ("ZNC", 2),
    ("MNP", 2), ("NOR", 3), ("OMN", 3), ("PAK", 4), ("PLW", 2), ("PSE", 3),
    ("PAN", 4), ("PNG", 3), ("XPI", 1), ("PRY", 3), ("PER", 4), ("PHL", 4),
    ("PCN", 1), ("POL", 4), ("PRT", 4), ("PRI", 2), ("QAT", 2), ("REU", 3),
    ("ROU", 3), ("RUS", 4), ("RWA", 5), ("BLM", 3), ("MAF", 1), ("SHN", 3),
    ("KNA", 2), ("LCA", 2), ("SPM", 2), ("VCT", 2), ("WSM", 3), ("SMR", 2),
    ("STP", 3), ("SAU", 3), ("SEN", 5), ("SRB", 3), ("SYC", 2), ("SLE", 4),
    ("SGP", 2), ("SXM", 1), ("SVK", 3), ("SVN", 3), ("SLB", 3), ("SOM", 3),
    ("ZAF", 5), ("SGS", 1), ("KOR", 4), ("SSD", 4), ("ESP", 5), ("XSP", 1),
    ("LKA", 3), ("SDN", 4), ("SUR", 3), ("SJM", 2), ("SWE", 3), ("CHE", 4),
    ("SYR", 3), ("TWN", 3), ("TJK", 4), ("TZA", 4), ("THA", 4), ("TGO", 4),
    ("TKL", 2), ("TON", 3), ("TTO", 2), ("TUN", 3), ("TUR", 3), ("TKM", 3),
    ("TCA", 2), ("TUV", 2), ("UGA", 5), ("UKR", 3), ("ARE", 4), ("GBR", 5),
    ("USA", 3), ("UMI", 2), ("URY", 3), ("UZB", 3), ("VUT", 3), ("VAT", 1),
    ("VEN", 3), ("VNM", 4), ("VIR", 3), ("WLF", 3), ("ESH", 2), ("YEM", 3),
    ("ZMB", 3), ("ZWE", 4),
]


def download_country(iso3: str, max_level: int) -> None:
    """Download the deepest-level GeoJSON for a country."""
    # Level 0 is uncompressed; levels 1+ are zipped
    level = max_level - 1  # GADM levels are 0-indexed, max_level is count
    if level == 0:
        url = f"{BASE_URL}/gadm41_{iso3}_0.json"
        out_path = os.path.join(OUTPUT_DIR, f"gadm41_{iso3}_0.json")
    else:
        url = f"{BASE_URL}/gadm41_{iso3}_{level}.json.zip"
        out_path = os.path.join(OUTPUT_DIR, f"gadm41_{iso3}_{level}.json")

    if os.path.exists(out_path):
        print(f"  [skip] {out_path} already exists")
        return

    print(f"  Downloading {url} ...")
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "daboyi-mapgen/1.0"})
        resp = urllib.request.urlopen(req, timeout=120)
        data = resp.read()

        if url.endswith(".zip"):
            with zipfile.ZipFile(io.BytesIO(data)) as zf:
                # Extract the .json file inside
                json_names = [n for n in zf.namelist() if n.endswith(".json")]
                if not json_names:
                    print(f"  [warn] No .json in zip for {iso3}")
                    return
                json_data = zf.read(json_names[0])
                with open(out_path, "wb") as f:
                    f.write(json_data)
        else:
            with open(out_path, "wb") as f:
                f.write(data)

        print(f"  [ok] {out_path} ({os.path.getsize(out_path) / 1024:.0f} KB)")
    except Exception as e:
        print(f"  [error] {iso3}: {e}")


def main():
    os.makedirs(OUTPUT_DIR, exist_ok=True)

    # Filter to specific countries if given as CLI args
    filter_isos = set(arg.upper() for arg in sys.argv[1:])

    countries = COUNTRIES
    if filter_isos:
        countries = [(iso, lvl) for iso, lvl in COUNTRIES if iso in filter_isos]
        if not countries:
            print(f"No matching countries for: {filter_isos}")
            sys.exit(1)

    total = len(countries)
    for i, (iso3, max_level) in enumerate(countries, 1):
        print(f"[{i}/{total}] {iso3} (level {max_level - 1})")
        download_country(iso3, max_level)
        time.sleep(0.5)  # Be polite to the server

    print(f"\nDone. Files in {OUTPUT_DIR}/")


if __name__ == "__main__":
    main()
