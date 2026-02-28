#!/usr/bin/env python3
"""
Extract vector river polylines from EU5's rivers.png and write rivers.bin.

EU5 rivers.png is a 16384×8192 palettized image where:
  - palette indices 3-12 (13 values per rivers.txt NUM_WIDTH_PIXEL_VALUES)
    represent river pixels (blue shades = actual rivers)
  - index 0 = land background, 1 = red marker, 2 = yellow marker, 15 = green
  - The map uses Gall Stereographic projection (same as rivers.tif)
  - equator_y = 3340 (from default.map), which matches Gall Stereo bounds

Output: assets/rivers.bin (bincode-encoded RiverData with RiverPolyline list)

Run: python3 tools/extract_rivers_vector.py
"""

import math
import struct
import sys
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage
from skimage.morphology import skeletonize

# ── Paths ─────────────────────────────────────────────────────────────────────
EU5_RIVERS = Path("/home/xiaoshihou/.local/share/Steam/steamapps/common/Europa Universalis V/game/in_game/map_data/rivers.png")
OUTPUT = Path("assets/rivers.bin")

# ── Gall Stereographic projection parameters ──────────────────────────────────
# Rivers.tif (same projection) bounds in metres (ESRI:54016):
X_MIN = -13303740.0
X_MAX =  15033576.0
Y_MIN =  -5789437.0
Y_MAX =   8379220.0

R = 6371007.2          # Earth radius used by Gall Stereo (metres)
COS45 = math.cos(math.radians(45))

IMG_W = 16384
IMG_H = 8192

# Work at half resolution for speed: 8192×4096
SCALE = 2   # divide pixel coords by this

WORK_W = IMG_W // SCALE
WORK_H = IMG_H // SCALE

# River palette indices: 3–15 encode different river widths.
# Width class: indices 3-6 = large rivers (class 2), 7-10 = medium (class 1), 11-15 = small (class 0)
RIVER_INDEX_MIN = 3
RIVER_INDEX_MAX = 15


def pixel_to_lonlat(col: float, row: float, w: int, h: int) -> tuple[float, float]:
    """Convert working-resolution pixel (col, row) to WGS84 (lon, lat) degrees."""
    # Scale to full-resolution pixel center
    full_col = col * SCALE + SCALE / 2
    full_row = row * SCALE + SCALE / 2
    # Map to Gall Stereo metres
    x = X_MIN + (full_col / IMG_W) * (X_MAX - X_MIN)
    y = Y_MAX - (full_row / IMG_H) * (Y_MAX - Y_MIN)  # row 0 = top = max y
    # Inverse Gall Stereo
    lon = math.degrees(x / (R * COS45))
    lat = math.degrees(2 * math.atan(y / (R * (1 + COS45))))
    return lon, lat


def width_class_for_index(idx: int) -> int:
    """Map palette index to width class 0 (thin) / 1 (medium) / 2 (wide)."""
    if idx <= 6:
        return 2   # wide (large rivers like Amazon, Yangtze)
    if idx <= 10:
        return 1   # medium
    return 0       # thin (tributaries)


def trace_polylines(skeleton: np.ndarray, index_map: np.ndarray) -> list[dict]:
    """
    Trace a binary skeleton into ordered polylines.
    Returns list of {points: [(col, row), ...], width_class: int}.
    """
    # Label connected components — each component is one river segment
    labeled, n_labels = ndimage.label(skeleton, structure=np.ones((3, 3), dtype=bool))
    print(f"  {n_labels} connected river segments")

    polylines = []
    # Build coordinate arrays per label for speed
    ys, xs = np.where(skeleton)

    from collections import defaultdict
    by_label = defaultdict(list)
    for y, x in zip(ys, xs):
        by_label[labeled[y, x]].append((x, y))

    for label_id in range(1, n_labels + 1):
        pts = by_label[label_id]
        if len(pts) < 3:
            continue  # skip isolated tiny fragments

        # For each component, order pixels by building a path from an endpoint
        # (pixel with only 1 neighbor) or any start pixel
        pts_set = set(pts)
        # Find degree of each pixel (number of 8-connected skeleton neighbors)
        adj = {p: [] for p in pts}
        for (x, y) in pts:
            for dx in [-1, 0, 1]:
                for dy in [-1, 0, 1]:
                    if dx == 0 and dy == 0:
                        continue
                    nb = (x + dx, y + dy)
                    if nb in pts_set:
                        adj[(x, y)].append(nb)
        # Endpoints have 1 neighbor (or 0 for isolated), branches have 3+
        endpoints = [p for p, nbrs in adj.items() if len(nbrs) <= 1]
        start = endpoints[0] if endpoints else pts[0]

        # Walk the path greedily
        path = [start]
        visited = {start}
        current = start
        while True:
            nbrs = [n for n in adj[current] if n not in visited]
            if not nbrs:
                break
            # Pick neighbor with fewest unvisited neighbors (prefer continuing straight)
            current = nbrs[0]
            visited.add(current)
            path.append(current)

        if len(path) < 3:
            continue

        # Determine width class from majority palette index along path
        indices_along = [index_map[y, x] for (x, y) in path]
        wc_counts = [0, 0, 0]
        for idx in indices_along:
            wc_counts[width_class_for_index(idx)] += 1
        wc = wc_counts.index(max(wc_counts))

        polylines.append({"points": path, "width_class": wc})

    return polylines


def douglas_peucker(points: list[tuple], epsilon: float) -> list[tuple]:
    """Simplify a polyline using Douglas-Peucker algorithm."""
    if len(points) <= 2:
        return points
    # Find point with max distance from line (start -> end)
    start, end = np.array(points[0]), np.array(points[-1])
    line = end - start
    line_len = np.linalg.norm(line)
    if line_len == 0:
        dists = [np.linalg.norm(np.array(p) - start) for p in points[1:-1]]
    else:
        unit = line / line_len
        dists = []
        for p in points[1:-1]:
            v = np.array(p) - start
            proj = np.dot(v, unit)
            closest = start + proj * unit
            dists.append(np.linalg.norm(np.array(p) - closest))
    max_dist = max(dists)
    max_idx = dists.index(max_dist) + 1
    if max_dist > epsilon:
        left = douglas_peucker(points[:max_idx + 1], epsilon)
        right = douglas_peucker(points[max_idx:], epsilon)
        return left[:-1] + right
    else:
        return [points[0], points[-1]]


def write_rivers_bin(polylines_lonlat: list[dict], output_path: Path):
    """
    Write bincode-compatible rivers.bin.
    Format (matches RiverData in shared/src/map.rs):
      bincode encoding of RiverData { rivers: Vec<RiverPolyline> }
      Each RiverPolyline: { points: Vec<[f32;2]>, width_class: u8 }

    bincode uses little-endian, u64 for Vec lengths, f32 values.
    """
    import io
    buf = io.BytesIO()

    def write_u64(v: int):
        buf.write(struct.pack('<Q', v))

    def write_u8(v: int):
        buf.write(struct.pack('B', v))

    def write_f32(v: float):
        buf.write(struct.pack('<f', v))

    # Vec<RiverPolyline> length
    write_u64(len(polylines_lonlat))
    for pl in polylines_lonlat:
        pts = pl["points"]
        # Vec<[f32;2]> — bincode encodes [f32;2] as two f32s, Vec has u64 length
        write_u64(len(pts))
        for lon, lat in pts:
            write_f32(lon)
            write_f32(lat)
        # width_class: u8
        write_u8(pl["width_class"])

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_bytes(buf.getvalue())
    print(f"Wrote {len(buf.getvalue()):,} bytes → {output_path}")


def main():
    print("Loading EU5 rivers.png...")
    img = Image.open(EU5_RIVERS)
    arr = np.array(img)  # palette indices, shape (8192, 16384)

    print(f"Image: {arr.shape[1]}×{arr.shape[0]}, palette mode: {img.mode}")
    print(f"Unique palette indices: {np.unique(arr)}")

    # Downsample to working resolution
    print(f"Downsampling to {WORK_W}×{WORK_H}...")
    arr_small = arr[::SCALE, ::SCALE]  # simple stride-based downsample

    # Build binary river mask (indices 3-15 = rivers)
    river_mask = (arr_small >= RIVER_INDEX_MIN) & (arr_small <= RIVER_INDEX_MAX)
    print(f"River pixels at working res: {river_mask.sum():,}")

    # Skeletonize to get 1px-wide centerlines
    print("Skeletonizing...")
    skeleton = skeletonize(river_mask)
    print(f"Skeleton pixels: {skeleton.sum():,}")

    # Trace skeleton into polylines
    print("Tracing polylines...")
    raw_polylines = trace_polylines(skeleton, arr_small)
    print(f"  Raw polylines: {len(raw_polylines)}")

    # Convert pixel coords to lon/lat and simplify with Douglas-Peucker
    # Epsilon = 2 working pixels ≈ ~0.04°, keeps enough detail
    SIMPLIFY_EPSILON = 2.0
    lonlat_polylines = []
    for pl in raw_polylines:
        if len(pl["points"]) < 3:
            continue
        # Simplify in pixel space first
        simplified = douglas_peucker(pl["points"], SIMPLIFY_EPSILON)
        if len(simplified) < 2:
            continue
        # Convert to lon/lat
        lonlat_pts = []
        for (col, row) in simplified:
            lon, lat = pixel_to_lonlat(col, row, WORK_W, WORK_H)
            # Clamp to valid range
            lon = max(-180.0, min(180.0, lon))
            lat = max(-90.0, min(90.0, lat))
            lonlat_pts.append((lon, lat))
        lonlat_polylines.append({"points": lonlat_pts, "width_class": pl["width_class"]})

    print(f"Final polylines after simplification: {len(lonlat_polylines)}")
    total_pts = sum(len(pl["points"]) for pl in lonlat_polylines)
    print(f"Total lon/lat points: {total_pts:,}")

    # Write binary output
    print("Writing rivers.bin...")
    write_rivers_bin(lonlat_polylines, OUTPUT)
    print("Done.")


if __name__ == "__main__":
    main()
