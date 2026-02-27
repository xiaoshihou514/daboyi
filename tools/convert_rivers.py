#!/usr/bin/env python3
"""
Convert rivers.tif (Gall Stereographic) to an equirectangular RGBA PNG.

River pixels (blue channel dominant) are kept as semi-transparent blue.
All other pixels are made fully transparent.

Run once to produce assets/rivers.png:
    python3 tools/convert_rivers.py

Requires: gdal Python bindings (osgeo.gdal) and Pillow.
"""

import os
import sys
import tempfile
from pathlib import Path

try:
    from osgeo import gdal
except ImportError:
    print("Error: gdal Python bindings not available. Install python3-gdal.")
    sys.exit(1)

try:
    from PIL import Image
    import numpy as np
except ImportError:
    print("Error: Pillow and numpy required. Install python3-pillow python3-numpy.")
    sys.exit(1)

REPO_ROOT = Path(__file__).resolve().parent.parent
SRC_TIF = Path("/home/xiaoshihou/Playground/github/EU5toGIS/datasets/rivers.tif")
OUT_PNG = REPO_ROOT / "assets" / "rivers.png"
# Output resolution: 2048×1024 equirectangular (fast to load, good enough for rivers).
OUT_W, OUT_H = 2048, 1024

def main():
    if not SRC_TIF.exists():
        print(f"Error: {SRC_TIF} not found.")
        sys.exit(1)

    OUT_PNG.parent.mkdir(parents=True, exist_ok=True)

    print(f"Reprojecting {SRC_TIF.name} to equirectangular {OUT_W}×{OUT_H} ...")
    with tempfile.NamedTemporaryFile(suffix=".tif", delete=False) as tmp:
        tmp_path = tmp.name

    try:
        src_ds = gdal.Open(str(SRC_TIF))
        if src_ds is None:
            print("Error: could not open rivers.tif with gdal.")
            sys.exit(1)

        # Warp to WGS84 geographic (equirectangular) covering full world.
        result = gdal.Warp(
            tmp_path,
            src_ds,
            format="GTiff",
            dstSRS="EPSG:4326",
            width=OUT_W,
            height=OUT_H,
            outputBounds=(-180.0, -90.0, 180.0, 90.0),
            resampleAlg="bilinear",
        )
        if result is None:
            print("Error: gdal.Warp failed.")
            sys.exit(1)
        result = None  # close
        src_ds = None

        print("Extracting river pixels ...")
        warped = Image.open(tmp_path)
        arr = np.array(warped, dtype=np.uint8)  # shape (H, W, 3)

        r, g, b = arr[:, :, 0], arr[:, :, 1], arr[:, :, 2]
        # River pixels: blue channel clearly dominant over red, and meaningfully blue.
        river_mask = (b.astype(np.int16) > r.astype(np.int16) + 40) & (b > 80)

        # Build RGBA output: river pixels = semi-transparent river blue, else transparent.
        out = np.zeros((OUT_H, OUT_W, 4), dtype=np.uint8)
        out[river_mask, 0] = 40   # R
        out[river_mask, 1] = 150  # G
        out[river_mask, 2] = 210  # B
        out[river_mask, 3] = 180  # A (semi-transparent)

        river_count = int(river_mask.sum())
        total = OUT_W * OUT_H
        print(f"River pixels: {river_count} / {total} ({100.0*river_count/total:.2f}%)")

        Image.fromarray(out, "RGBA").save(str(OUT_PNG), optimize=True)
        print(f"Saved {OUT_PNG} ({OUT_PNG.stat().st_size / 1024:.1f} KB)")

    finally:
        if os.path.exists(tmp_path):
            os.unlink(tmp_path)


if __name__ == "__main__":
    main()
