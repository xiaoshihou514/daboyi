#!/usr/bin/env python3
"""
Convert rivers.tif (Gall Stereographic) to an equirectangular RGBA PNG.

River pixels (blue channel dominant) are extracted at source resolution first,
then the binary mask is reprojected with nearest-neighbor to preserve thin lines.

Run once to produce assets/rivers.png:
    python3 tools/convert_rivers.py

Requires: gdal Python bindings (osgeo.gdal) and Pillow.
"""

import os
import sys
import tempfile
from pathlib import Path

try:
    from osgeo import gdal, osr
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
# Output resolution: 4096×2048 equirectangular (2× previous, needed for thin rivers).
OUT_W, OUT_H = 4096, 2048

def main():
    if not SRC_TIF.exists():
        print(f"Error: {SRC_TIF} not found.")
        sys.exit(1)

    OUT_PNG.parent.mkdir(parents=True, exist_ok=True)

    src_ds = gdal.Open(str(SRC_TIF))
    if src_ds is None:
        print("Error: could not open rivers.tif with gdal.")
        sys.exit(1)

    W = src_ds.RasterXSize
    H = src_ds.RasterYSize
    print(f"Source: {W}×{H}")

    print("Reading source bands ...")
    band_r = src_ds.GetRasterBand(1).ReadAsArray()
    band_g = src_ds.GetRasterBand(2).ReadAsArray()
    band_b = src_ds.GetRasterBand(3).ReadAsArray()
    src_gt = src_ds.GetGeoTransform()
    src_srs_wkt = src_ds.GetProjectionRef()

    # River pixels: blue channel clearly dominant over red.
    river_mask = (
        (band_b.astype(np.int16) > band_r.astype(np.int16) + 40)
        & (band_b > 80)
    )
    print(f"River pixels at source res: {river_mask.sum()} / {W*H}")

    # Build a single-band uint8 in-memory raster: 255=river, 0=other.
    print("Creating river mask raster ...")
    with tempfile.NamedTemporaryFile(suffix="_mask.tif", delete=False) as tmp:
        mask_path = tmp.name
    with tempfile.NamedTemporaryFile(suffix="_warped.tif", delete=False) as tmp2:
        warped_path = tmp2.name

    try:
        driver = gdal.GetDriverByName("GTiff")
        mask_ds = driver.Create(mask_path, W, H, 1, gdal.GDT_Byte)
        mask_ds.SetGeoTransform(src_gt)
        mask_ds.SetProjection(src_srs_wkt)
        mask_arr = river_mask.astype(np.uint8) * 255
        mask_ds.GetRasterBand(1).WriteArray(mask_arr)
        mask_ds.FlushCache()
        mask_ds = None

        print(f"Reprojecting mask to equirectangular {OUT_W}×{OUT_H} (nearest-neighbor) ...")
        result = gdal.Warp(
            warped_path,
            mask_path,
            format="GTiff",
            dstSRS="EPSG:4326",
            width=OUT_W,
            height=OUT_H,
            outputBounds=(-180.0, -90.0, 180.0, 90.0),
            resampleAlg="near",  # nearest-neighbor preserves thin 1-pixel rivers
        )
        if result is None:
            print("Error: gdal.Warp failed.")
            sys.exit(1)
        result = None

        print("Converting to RGBA PNG ...")
        warped_ds = gdal.Open(warped_path)
        warped_arr = warped_ds.GetRasterBand(1).ReadAsArray()
        warped_ds = None

        out = np.zeros((OUT_H, OUT_W, 4), dtype=np.uint8)
        river_pixels = warped_arr > 127
        out[river_pixels, 0] = 30
        out[river_pixels, 1] = 140
        out[river_pixels, 2] = 220
        out[river_pixels, 3] = 200

        river_count = int(river_pixels.sum())
        print(f"River pixels in output: {river_count} / {OUT_W*OUT_H} ({100.0*river_count/(OUT_W*OUT_H):.2f}%)")

        Image.fromarray(out, "RGBA").save(str(OUT_PNG))
        print(f"Saved {OUT_PNG} ({OUT_PNG.stat().st_size / 1024:.1f} KB)")

    finally:
        for p in (mask_path, warped_path):
            if os.path.exists(p):
                os.unlink(p)


if __name__ == "__main__":
    main()

