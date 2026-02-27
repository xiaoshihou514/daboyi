use shared::conv::u32_to_f32;
use shared::{BuildingType, Province};

/// Run production for all buildings in a province for one tick.
/// Each building level requires `workers_per_level` workers of the right class.
/// Output is scaled by min(worker_ratio, input_ratio) where both are capped at 1.0.
pub fn produce(province: &mut Province, building_types: &[BuildingType]) {
    for building in &province.buildings {
        let Some(bt) = building_types.iter().find(|bt| bt.id == building.type_id) else {
            continue;
        };

        let workers_needed = u32_to_f32(bt.workers_per_level) * u32_to_f32(building.level);
        if workers_needed <= 0.0 {
            continue;
        }

        // How many workers of the right class are available?
        let available_workers: u32 = province
            .pops
            .iter()
            .filter(|p| p.class == bt.worker_class)
            .map(|p| p.size)
            .sum();

        let worker_ratio = (u32_to_f32(available_workers) / workers_needed).min(1.0);
        if worker_ratio <= 0.0 {
            continue;
        }

        // Check input availability and compute input ratio.
        let input_ratio = if bt.input.is_empty() {
            1.0
        } else {
            bt.input
                .iter()
                .map(|&(good, amount_per_level)| {
                    let needed = amount_per_level * u32_to_f32(building.level);
                    if needed <= 0.0 {
                        return 1.0;
                    }
                    let available = province.stockpile.get(&good).copied().unwrap_or(0.0);
                    (available / needed).min(1.0)
                })
                .fold(f32::MAX, f32::min)
        };

        let efficiency = worker_ratio * input_ratio;
        if efficiency <= 0.0 {
            continue;
        }

        // Consume inputs.
        for &(good, amount_per_level) in &bt.input {
            let consumed = amount_per_level * u32_to_f32(building.level) * efficiency;
            let stock = province.stockpile.entry(good).or_insert(0.0);
            *stock = (*stock - consumed).max(0.0);
        }

        // Produce outputs.
        for &(good, amount_per_level) in &bt.output {
            let produced = amount_per_level * u32_to_f32(building.level) * efficiency;
            *province.stockpile.entry(good).or_insert(0.0) += produced;
        }
    }
}

/// Run production across all provinces.
pub fn produce_all(provinces: &mut [Province], building_types: &[BuildingType]) {
    for province in provinces.iter_mut() {
        produce(province, building_types);
    }
}
