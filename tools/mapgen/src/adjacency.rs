use shared::map::{CachedBorder, MapData, MapProvince};
use std::collections::HashMap;

type PairData = ([u32; 2], Vec<Vec<[f32; 2]>>);

pub fn build_adjacency(map: &MapData) -> Vec<CachedBorder> {
    let quantize = |v: f32| -> i32 { (v * 100.0).round() as i32 };
    let mut edge_map: HashMap<[(i32, i32); 2], u32> = HashMap::new();
    let mut pairs: Vec<[u32; 2]> = Vec::new();

    for province in &map.provinces {
        let pid = province.id;
        for ring in &province.boundary {
            let n = ring.len();
            for i in 0..n {
                let a = ring[i];
                let b = ring[(i + 1) % n];
                let qa = (quantize(a[0]), quantize(a[1]));
                let qb = (quantize(b[0]), quantize(b[1]));
                let key = if qa <= qb { [qa, qb] } else { [qb, qa] };
                if let Some(&other_pid) = edge_map.get(&key) {
                    if other_pid != pid {
                        pairs.push([other_pid.min(pid), other_pid.max(pid)]);
                    }
                } else {
                    edge_map.insert(key, pid);
                }
            }
        }
    }

    pairs.sort_unstable();
    pairs.dedup();

    let mut pair_data: Vec<PairData> = Vec::with_capacity(pairs.len());
    for pair in pairs {
        let ia = pair[0] as usize;
        let ib = pair[1] as usize;
        if ia >= map.provinces.len() || ib >= map.provinces.len() {
            continue;
        }
        let raw_chains = chain_polylines(shared_segments(&map.provinces[ia], &map.provinces[ib]));
        if raw_chains.is_empty() {
            continue;
        }
        let merged = merge_chains(raw_chains);
        pair_data.push((pair, merged));
    }

    weld_endpoints_global(&mut pair_data);

    let mut cached_borders = Vec::with_capacity(pair_data.len());
    for (pair, chains) in pair_data {
        let smoothed: Vec<Vec<[f32; 2]>> = chains
            .iter()
            .map(|c| chaikin_smooth(&chaikin_smooth(c)))
            .collect();
        cached_borders.push(CachedBorder {
            provinces: pair,
            chains: smoothed,
        });
    }
    cached_borders
}

fn border_quantize(v: f32) -> i32 {
    (v * 100.0).round() as i32
}

fn border_qpt(p: [f32; 2]) -> (i32, i32) {
    (border_quantize(p[0]), border_quantize(p[1]))
}

fn point_on_segment_t(point: [f32; 2], a: [f32; 2], b: [f32; 2], eps: f32) -> Option<f32> {
    let abx = b[0] - a[0];
    let aby = b[1] - a[1];
    let len2 = abx * abx + aby * aby;
    if len2 < 1e-12 {
        let dx = point[0] - a[0];
        let dy = point[1] - a[1];
        if (dx * dx + dy * dy).sqrt() <= eps {
            return Some(0.0);
        }
        return None;
    }

    let len = len2.sqrt();
    let apx = point[0] - a[0];
    let apy = point[1] - a[1];
    let cross = abx * apy - aby * apx;
    let dist = cross.abs() / len;
    if dist > eps {
        return None;
    }

    let dot = apx * abx + apy * aby;
    let t = dot / len2;
    let tol_t = eps / len;
    if t < -tol_t || t > 1.0 + tol_t {
        return None;
    }
    Some(t)
}

fn segment_midpoint(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}

fn shared_segments(a: &MapProvince, b: &MapProvince) -> Vec<[[f32; 2]; 2]> {
    const SPLIT_EPS: f32 = 0.01;

    let mut b_points: Vec<[f32; 2]> = Vec::new();
    let mut b_segments: Vec<[[f32; 2]; 2]> = Vec::new();
    for ring in &b.boundary {
        let n = ring.len();
        for i in 0..n {
            let p0 = ring[i];
            let p1 = ring[(i + 1) % n];
            b_points.push(p0);
            b_segments.push([p0, p1]);
        }
    }

    let mut result = Vec::new();
    for ring in &a.boundary {
        let n = ring.len();
        for i in 0..n {
            let p0 = ring[i];
            let p1 = ring[(i + 1) % n];

            let mut split_points = vec![(0.0_f32, p0), (1.0_f32, p1)];
            for &bp in &b_points {
                if let Some(t) = point_on_segment_t(bp, p0, p1, SPLIT_EPS) {
                    if t > 1e-4 && t < 1.0 - 1e-4 {
                        split_points.push((t, bp));
                    }
                }
            }
            split_points.sort_by(|lhs, rhs| {
                lhs.0
                    .partial_cmp(&rhs.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let mut deduped: Vec<[f32; 2]> = Vec::with_capacity(split_points.len());
            for (_, point) in split_points {
                if deduped
                    .last()
                    .map(|last| border_qpt(*last) == border_qpt(point))
                    .unwrap_or(false)
                {
                    continue;
                }
                deduped.push(point);
            }

            for window in deduped.windows(2) {
                let s0 = window[0];
                let s1 = window[1];
                if border_qpt(s0) == border_qpt(s1) {
                    continue;
                }
                let midpoint = segment_midpoint(s0, s1);
                if b_segments.iter().any(|segment| {
                    point_on_segment_t(midpoint, segment[0], segment[1], SPLIT_EPS).is_some()
                }) {
                    result.push([s0, s1]);
                }
            }
        }
    }
    result
}

fn chain_polylines(segments: Vec<[[f32; 2]; 2]>) -> Vec<Vec<[f32; 2]>> {
    if segments.is_empty() {
        return vec![];
    }
    let pts_eq =
        |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5;
    let mut chains: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut current: Vec<[f32; 2]> = vec![segments[0][0], segments[0][1]];

    for seg in segments.iter().skip(1) {
        let last = *current.last().unwrap();
        if pts_eq(last, seg[0]) {
            current.push(seg[1]);
        } else {
            chains.push(current);
            current = vec![seg[0], seg[1]];
        }
    }
    chains.push(current);
    chains
}

fn merge_chains(mut chains: Vec<Vec<[f32; 2]>>) -> Vec<Vec<[f32; 2]>> {
    let quantize = |v: f32| -> i32 { (v * 10.0).round() as i32 };
    let qpt = |p: [f32; 2]| -> (i32, i32) { (quantize(p[0]), quantize(p[1])) };

    'restart: loop {
        let n = chains.len();
        for i in 0..n {
            for j in 0..n {
                if i == j {
                    continue;
                }
                let qi_first = qpt(chains[i][0]);
                let qi_last = qpt(*chains[i].last().unwrap());
                let qj_first = qpt(chains[j][0]);
                let qj_last = qpt(*chains[j].last().unwrap());

                if qi_last == qj_first {
                    let tail = chains[j][1..].to_vec();
                    chains[i].extend(tail);
                    chains.remove(j);
                    continue 'restart;
                }
                if qi_last == qj_last {
                    let mut j_rev = chains[j].clone();
                    j_rev.reverse();
                    chains[i].extend_from_slice(&j_rev[1..]);
                    chains.remove(j);
                    continue 'restart;
                }
                if qi_first == qj_last {
                    let prefix_len = chains[j].len() - 1;
                    let prefix = chains[j][..prefix_len].to_vec();
                    let tail = std::mem::take(&mut chains[i]);
                    let mut new_chain = prefix;
                    new_chain.extend(tail);
                    chains[i] = new_chain;
                    chains.remove(j);
                    continue 'restart;
                }
                if qi_first == qj_first {
                    let mut j_rev = chains[j].clone();
                    j_rev.reverse();
                    let tail = std::mem::take(&mut chains[i]);
                    j_rev.extend_from_slice(&tail[1..]);
                    chains[i] = j_rev;
                    chains.remove(j);
                    continue 'restart;
                }
            }
        }
        break;
    }
    chains
}

fn weld_endpoints_global(pair_data: &mut [PairData]) {
    let quantize = |v: f32| -> i32 { (v * 10.0).round() as i32 };
    let qpt = |p: [f32; 2]| -> (i32, i32) { (quantize(p[0]), quantize(p[1])) };

    let mut bucket_sum: HashMap<(i32, i32), ([f32; 2], u32)> = HashMap::new();
    for (_, chains) in pair_data.iter() {
        for chain in chains {
            let n = chain.len();
            if n < 2 {
                continue;
            }
            for &pt in &[chain[0], chain[n - 1]] {
                let q = qpt(pt);
                let e = bucket_sum.entry(q).or_insert(([0.0, 0.0], 0));
                e.0[0] += pt[0];
                e.0[1] += pt[1];
                e.1 += 1;
            }
        }
    }
    let centroids: HashMap<(i32, i32), [f32; 2]> = bucket_sum
        .into_iter()
        .map(|(k, (sum, n))| (k, [sum[0] / n as f32, sum[1] / n as f32]))
        .collect();

    for (_, chains) in pair_data.iter_mut() {
        for chain in chains.iter_mut() {
            let n = chain.len();
            if n < 2 {
                continue;
            }
            if let Some(&c) = centroids.get(&qpt(chain[0])) {
                chain[0] = c;
            }
            if let Some(&c) = centroids.get(&qpt(chain[n - 1])) {
                chain[n - 1] = c;
            }
        }
    }
}

fn chaikin_smooth(pts: &[[f32; 2]]) -> Vec<[f32; 2]> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let n = pts.len();
    let mut result = Vec::with_capacity(n * 2);
    result.push(pts[0]);
    for i in 0..n - 1 {
        let [ax, ay] = pts[i];
        let [bx, by] = pts[i + 1];
        result.push([0.75 * ax + 0.25 * bx, 0.75 * ay + 0.25 * by]);
        result.push([0.25 * ax + 0.75 * bx, 0.25 * ay + 0.75 * by]);
    }
    result.push(pts[n - 1]);
    result
}
