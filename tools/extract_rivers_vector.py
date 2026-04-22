#!/usr/bin/env python3
"""
Extract vector river polylines from EU5's rivers.png and write rivers.bin.

EU5 rivers.png is a 16384×8192 palettized image where:
  - palette indices 3-15 (13 values per rivers.txt NUM_WIDTH_PIXEL_VALUES)
    represent river pixels; index 0 = land, 1 = inlet marker, 2 = source
  - The map uses Gall Stereographic projection (same as rivers.tif)
  - equator_y = 3340 (from default.map), consistent with Gall Stereo bounds

Output: assets/rivers.bin (bincode-encoded RiverData with explicit nodes + edges)

Managed by: uv (see tools/pyproject.toml)

Run from repo root:
    uv run --project tools python tools/extract_rivers_vector.py
"""

import math
import struct
import sys
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage
from skimage.morphology import skeletonize, closing, disk
from skimage.measure import block_reduce

# ── Paths ─────────────────────────────────────────────────────────────────────
# rivers.png is copied from the EU5 Steam installation into EU5toGIS.
# Expected layout: this repo is at ../daboyi relative to EU5toGIS.
REPO_ROOT = Path(__file__).resolve().parent.parent
EU5_RIVERS = REPO_ROOT.parent / "EU5toGIS" / "datasets" / "rivers.png"
OUTPUT = REPO_ROOT / "assets" / "rivers.bin"

# ── Gall Stereographic projection parameters ──────────────────────────────────
# Bounds in metres (ESRI:54016), verified against equator_y=3340 in default.map:
X_MIN = -13303740.0
X_MAX =  15033576.0
Y_MIN =  -5789437.0
Y_MAX =   8379220.0

# Match the map pipeline's inverse Gall Stereographic parameters so the
# extracted river coordinates line up with province geometry.
R = 6378137.0
COS45 = math.cos(math.radians(45))

IMG_W = 16384
IMG_H = 8192

# Work at half resolution (8192×4096) using max-pooling (not stride).
# Max-pooling preserves any river pixel present in each 2×2 block.
SCALE = 2
WORK_W = IMG_W // SCALE
WORK_H = IMG_H // SCALE

# River palette indices: 3–15 encode different river widths.
# Lower indices render thinner channels; higher indices render wider channels.
# Width classes in rivers.bin follow shared/src/map.rs:
#   0 = thin, 1 = medium, 2 = wide
RIVER_INDEX_MIN = 3
RIVER_INDEX_MAX = 15

# Douglas-Peucker simplification tolerance in working-resolution pixels.
# 0.5 px ≈ 0.022° ≈ 2.4 km; keeps river curves while removing collinear points.
SIMPLIFY_EPSILON = 0.5

# Minimum path length to emit (shorter paths are noise / crossing artefacts)
MIN_PATH_PX = 4


def pixel_to_lonlat(col: float, row: float) -> tuple[float, float]:
    """Convert working-resolution pixel (col, row) to WGS84 (lon, lat) degrees."""
    full_col = col * SCALE + SCALE / 2.0
    full_row = row * SCALE + SCALE / 2.0
    x = X_MIN + (full_col / IMG_W) * (X_MAX - X_MIN)
    y = Y_MAX - (full_row / IMG_H) * (Y_MAX - Y_MIN)   # row 0 = top = max y
    lon = math.degrees(x / (R * COS45))
    lat = math.degrees(2.0 * math.atan(y / (R * (1.0 + COS45))))
    return (max(-180.0, min(180.0, lon)), max(-90.0, min(90.0, lat)))


def width_class_for_index(idx: int) -> int:
    """Map palette index to width class 0 (thin) / 1 (medium) / 2 (wide)."""
    if idx <= 6:
        return 0
    if idx <= 10:
        return 1
    return 2


def pooled_width_index(block: np.ndarray, axis=None) -> np.ndarray:
    """
    Preserve the widest river class present in a pooled block.

    River palette indices are inverted with respect to width: lower indices are
    wider (3-6), higher indices are thinner (11-15). Using max-pooling here
    collapses mixed-width blocks toward thin rivers, which made every emitted
    edge end up as width class 0. We instead keep the minimum river index
    present in the block, falling back to 0 when no river pixel exists.
    """
    masked = np.where(
        (block >= RIVER_INDEX_MIN) & (block <= RIVER_INDEX_MAX),
        block,
        255,
    )
    reduced = masked.min(axis=axis)
    return np.where(reduced == 255, 0, reduced).astype(np.uint8)


def build_adjacency(skeleton: np.ndarray) -> dict:
    """
    Build an adjacency dict {(col, row): [(col, row), ...]} for all
    skeleton pixels using 8-connectivity.  Returns (adj, pixel_set).
    """
    ys, xs = np.where(skeleton)
    pixel_set = set(zip(xs.tolist(), ys.tolist()))  # (col, row)
    adj: dict[tuple, list] = {p: [] for p in pixel_set}
    for (col, row) in pixel_set:
        for dc in (-1, 0, 1):
            for dr in (-1, 0, 1):
                if dc == 0 and dr == 0:
                    continue
                nb = (col + dc, row + dr)
                if nb in pixel_set:
                    adj[(col, row)].append(nb)
    return adj, pixel_set


def collapse_special_nodes(adj: dict) -> tuple[list[dict], dict]:
    """
    Collapse each connected component of special pixels (degree != 2) into one
    logical river node. This prevents a single visual fork/merge blob from
    becoming several nearby nodes that render as disjoint pieces.
    """
    special = {p for p, nbrs in adj.items() if len(nbrs) != 2}
    remaining = set(special)
    components: list[list[tuple[int, int]]] = []

    while remaining:
        start = min(remaining)
        stack = [start]
        component: list[tuple[int, int]] = []
        remaining.remove(start)
        while stack:
            curr = stack.pop()
            component.append(curr)
            for nb in adj[curr]:
                if nb in remaining:
                    remaining.remove(nb)
                    stack.append(nb)
        component.sort()
        components.append(component)

    node_components: list[dict] = []
    pixel_to_node: dict[tuple[int, int], tuple[int, int]] = {}
    for component in sorted(components, key=lambda c: c[0]):
        rep = component[0]
        avg_col = sum(col for col, _ in component) / len(component)
        avg_row = sum(row for _, row in component) / len(component)
        node_components.append(
            {
                "rep": rep,
                "pixels": set(component),
                "center_pixel": (avg_col, avg_row),
            }
        )
        for pixel in component:
            pixel_to_node[pixel] = rep

    return node_components, pixel_to_node


def extract_graph_edges(adj: dict) -> tuple[list[dict], list[dict]]:
    """
    Extract river paths by tracing graph edges between junction/endpoint nodes.

    Junction = pixel with ≥ 3 neighbours (river fork/merge)
    Endpoint  = pixel with ≤ 1 neighbour (river source/outlet)
    Interior  = pixel with exactly 2 neighbours (straight run)

    Each edge between two non-interior nodes becomes one polyline with explicit
    start/end node identity. Interior pixels are strung together into the path
    between them.
    """
    node_components, pixel_to_node = collapse_special_nodes(adj)

    visited_edges: set[frozenset] = set()
    edges: list[dict] = []

    # Trace from each logical node outward along edges leaving its special-pixel blob.
    for node in node_components:
        start_rep = node["rep"]
        outgoing: list[tuple[tuple[int, int], tuple[int, int]]] = []
        for pixel in sorted(node["pixels"]):
            for nb in sorted(adj[pixel]):
                if nb not in node["pixels"]:
                    outgoing.append((pixel, nb))

        for start_pixel, nb in outgoing:
            edge_key = frozenset({start_pixel, nb})
            if edge_key in visited_edges:
                continue
            visited_edges.add(edge_key)

            # Walk: start-node boundary pixel → nb → ... until we hit another
            # special-pixel component.
            path = [start_pixel, nb]
            prev, curr = start_pixel, nb
            while curr not in pixel_to_node:
                nexts = [n for n in adj[curr] if n != prev]
                if not nexts:
                    break
                nxt = nexts[0]
                fk = frozenset({curr, nxt})
                visited_edges.add(fk)
                prev, curr = curr, nxt
                path.append(curr)

            end_rep = pixel_to_node.get(curr, start_rep)
            edges.append({"path": path, "start": start_rep, "end": end_rep})

    # Handle isolated loops (all interior: e.g. a perfectly circular lake outline).
    # Find any unvisited pixel and trace the loop once.
    visited_px = set()
    for edge in edges:
        visited_px.update(edge["path"])

    for start in sorted(adj):
        if start in visited_px:
            continue
        if not adj[start]:
            continue
        # Start a loop trace
        path = [start]
        visited_px.add(start)
        curr = sorted(adj[start])[0]
        while curr != start and curr not in visited_px:
            visited_px.add(curr)
            path.append(curr)
            nexts = sorted(n for n in adj[curr] if n not in visited_px)
            if not nexts:
                break
            curr = nexts[0]
        if len(path) >= MIN_PATH_PX:
            edges.append({"path": path, "start": start, "end": start})

    for edge in edges:
        for endpoint_key in ("start", "end"):
            endpoint = edge[endpoint_key]
            if endpoint not in pixel_to_node:
                pixel_to_node[endpoint] = endpoint
                node_components.append(
                    {
                        "rep": endpoint,
                        "pixels": {endpoint},
                        "center_pixel": (float(endpoint[0]), float(endpoint[1])),
                    }
                )
            edge[endpoint_key] = pixel_to_node[endpoint]

    node_components.sort(key=lambda node: node["rep"])
    return node_components, edges


def douglas_peucker(points: list[tuple], epsilon: float) -> list[tuple]:
    """Simplify a polyline in-place with Ramer-Douglas-Peucker."""
    if len(points) <= 2:
        return points
    start = np.array(points[0], dtype=float)
    end   = np.array(points[-1], dtype=float)
    seg   = end - start
    seg_len = np.linalg.norm(seg)

    if seg_len < 1e-9:
        dists = [np.linalg.norm(np.array(p, dtype=float) - start) for p in points[1:-1]]
    else:
        unit = seg / seg_len
        dists = []
        for p in points[1:-1]:
            v = np.array(p, dtype=float) - start
            proj = np.dot(v, unit)
            closest = start + proj * unit
            dists.append(float(np.linalg.norm(np.array(p, dtype=float) - closest)))

    max_dist = max(dists)
    max_idx  = dists.index(max_dist) + 1

    if max_dist > epsilon:
        left  = douglas_peucker(points[:max_idx + 1], epsilon)
        right = douglas_peucker(points[max_idx:],     epsilon)
        return left[:-1] + right
    return [points[0], points[-1]]


def write_rivers_bin(nodes: list[dict], edges: list[dict], output_path: Path) -> None:
    """
    Write bincode-compatible rivers.bin matching RiverData in shared/src/map.rs.

      RiverData  { nodes: Vec<RiverNode>, edges: Vec<RiverEdge> }
      RiverNode { position: [f32;2] }
      RiverEdge { points: Vec<[f32;2]>, width_class: u8, start_node: u32, end_node: u32 }

    bincode (v1 default): little-endian, Vec length as u64, array as raw bytes.
    """
    import io
    buf = io.BytesIO()

    def wu64(v): buf.write(struct.pack('<Q', int(v)))
    def wu8(v):  buf.write(struct.pack('B',  int(v)))
    def wf32(v): buf.write(struct.pack('<f', float(v)))

    wu64(len(nodes))
    for node in nodes:
        lon, lat = node["position"]
        wf32(lon)
        wf32(lat)

    wu64(len(edges))
    for edge in edges:
        pts = edge["points"]
        wu64(len(pts))
        for lon, lat in pts:
            wf32(lon)
            wf32(lat)
        wu8(edge["width_class"])
        buf.write(struct.pack('<I', int(edge["start_node"])))
        buf.write(struct.pack('<I', int(edge["end_node"])))

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_bytes(buf.getvalue())
    print(f"Wrote {len(buf.getvalue()):,} bytes → {output_path}")


def main() -> None:
    if not EU5_RIVERS.exists():
        print(f"Error: {EU5_RIVERS} not found.")
        print("Copy rivers.png from the EU5 Steam installation:")
        print("  <EU5>/game/in_game/map_data/rivers.png  →  EU5toGIS/datasets/rivers.png")
        sys.exit(1)

    # ── Load ──────────────────────────────────────────────────────────────────
    print(f"Loading {EU5_RIVERS}...")
    img = Image.open(EU5_RIVERS)
    arr = np.array(img)              # shape (8192, 16384), palette indices
    print(f"Image: {arr.shape[1]}×{arr.shape[0]}, unique indices: {np.unique(arr)}")

    # ── Binary river mask (full resolution) ───────────────────────────────────
    river_full = (arr >= RIVER_INDEX_MIN) & (arr <= RIVER_INDEX_MAX)
    print(f"River pixels (full res): {river_full.sum():,}")

    # ── Max-pool downsample (Bug C fix) ───────────────────────────────────────
    # Any river pixel present in a 2×2 block → preserved at working resolution.
    # Stride-based arr[::2,::2] would silently drop entire 1-pixel rivers.
    print(f"Max-pooling to {WORK_W}×{WORK_H}...")
    river_mask = block_reduce(river_full, block_size=(SCALE, SCALE), func=np.max)
    print(f"River pixels (working res): {river_mask.sum():,}")

    # Also downsample palette indices for width-class assignment.
    # Lower palette indices are wider rivers, so preserve the minimum river
    # index seen in each block rather than max-pooling toward thin classes.
    idx_small = block_reduce(arr, block_size=(SCALE, SCALE), func=pooled_width_index).astype(np.uint8)

    # ── Morphological closing (connect 1-pixel gaps) ─────────────────────────
    river_mask = closing(river_mask, disk(1))

    # ── Skeletonize ───────────────────────────────────────────────────────────
    print("Skeletonizing...")
    skeleton = skeletonize(river_mask)
    print(f"Skeleton pixels: {skeleton.sum():,}")

    # ── Graph-edge path extraction (Bug B fix) ────────────────────────────────
    # Build adjacency graph, then emit one polyline per edge between junction/
    # endpoint nodes.  This correctly handles river networks with forks/merges.
    print("Building adjacency graph...")
    adj, _ = build_adjacency(skeleton)
    print(f"Graph nodes: {len(adj):,}")

    print("Extracting graph edges as polylines...")
    node_components, raw_edges = extract_graph_edges(adj)
    print(f"Raw edges: {len(raw_edges):,}")

    # ── Simplify + convert to lon/lat (Bug A fix) ─────────────────────────────
    # Epsilon = 0.5 px (≈ 2.4 km) — keeps river curves, removes collinear pts.
    # Previously epsilon=2.0 px was collapsing entire rivers to 2 endpoints.
    node_ids = {node["rep"]: idx for idx, node in enumerate(node_components)}
    lonlat_nodes = [
        {"position": pixel_to_lonlat(col, row)}
        for node in node_components
        for (col, row) in [node["center_pixel"]]
    ]

    lonlat_edges: list[dict] = []
    for edge in raw_edges:
        path = edge["path"]
        if len(path) < MIN_PATH_PX:
            continue
        simplified = douglas_peucker(path, SIMPLIFY_EPSILON)
        if len(simplified) < 2:
            continue
        # Width class: majority vote along the path
        wc_counts = [0, 0, 0]
        for (col, row) in path:
            idx = int(idx_small[row, col])
            if RIVER_INDEX_MIN <= idx <= RIVER_INDEX_MAX:
                wc_counts[width_class_for_index(idx)] += 1
        wc = int(wc_counts.index(max(wc_counts)))

        pts = [pixel_to_lonlat(col, row) for (col, row) in simplified]
        lonlat_edges.append(
            {
                "points": pts,
                "width_class": wc,
                "start_node": node_ids[edge["start"]],
                "end_node": node_ids[edge["end"]],
            }
        )

    total_pts = sum(len(edge["points"]) for edge in lonlat_edges)
    print(
        f"Final graph: {len(lonlat_nodes):,} nodes, {len(lonlat_edges):,} edges  |  total points: {total_pts:,}"
    )
    if lonlat_edges:
        avg = total_pts / len(lonlat_edges)
        print(f"Avg points/edge: {avg:.1f}")

    # ── Write ─────────────────────────────────────────────────────────────────
    write_rivers_bin(lonlat_nodes, lonlat_edges, OUTPUT)
    print("Done.")


if __name__ == "__main__":
    main()
