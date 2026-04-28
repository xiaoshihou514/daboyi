//! 空间哈希：加速刷子查找省份
//!
//! 将地图划分为网格，每个网格存储包含的省份 ID。
//! 刷子只需检查附近网格，从 O(n) 降到 O(1)。

use bevy::prelude::*;
use std::collections::HashMap;

thread_local! {
    static SEEN_CACHE: std::cell::RefCell<std::collections::HashSet<u32>> = 
        std::cell::RefCell::new(std::collections::HashSet::new());
}

#[derive(Resource, Default)]
pub struct SpatialHash {
    cell_size: f32,
    cells: HashMap<(i32, i32), Vec<u32>>,
}

impl SpatialHash {
    pub fn build(provinces: &[shared::map::MapProvince]) -> Self {
        let cell_size = 2.0;
        let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();

        for prov in provinces {
            if prov.centroid[0] == 0.0 && prov.centroid[1] == 0.0 {
                continue;
            }

            let (gx, gy) = Self::grid_coords(prov.centroid[0], prov.centroid[1], cell_size);
            cells.entry((gx, gy)).or_default().push(prov.id);
        }

        bevy::log::info!(
            target: "daboyi::startup",
            "SpatialHash: {} provinces, {} cells",
            provinces.len(),
            cells.len()
        );

        Self { cell_size, cells }
    }

    fn grid_coords(x: f32, y: f32, cell_size: f32) -> (i32, i32) {
        (
            (x / cell_size).floor() as i32,
            (y / cell_size).floor() as i32,
        )
    }

    pub fn find_in_radius(&self, pos: [f32; 2], radius: f32) -> Vec<u32> {
        let mut result = Vec::new();
        self.find_in_radius_into(pos, radius, &mut result);
        result
    }

    pub fn find_in_radius_into(&self, pos: [f32; 2], radius: f32, result: &mut Vec<u32>) {
        SEEN_CACHE.with(|cache| {
            let mut seen = cache.borrow_mut();
            seen.clear();
            result.clear();

            let min_gx = ((pos[0] - radius) / self.cell_size).floor() as i32;
            let max_gx = ((pos[0] + radius) / self.cell_size).floor() as i32;
            let min_gy = ((pos[1] - radius) / self.cell_size).floor() as i32;
            let max_gy = ((pos[1] + radius) / self.cell_size).floor() as i32;

            for gx in min_gx..=max_gx {
                for gy in min_gy..=max_gy {
                    if let Some(prov_ids) = self.cells.get(&(gx, gy)) {
                        for &prov_id in prov_ids {
                            if seen.insert(prov_id) {
                                result.push(prov_id);
                            }
                        }
                    }
                }
            }
        });
    }
}
