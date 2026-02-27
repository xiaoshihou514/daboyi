use shared::conv::{f32_to_i64, u32_to_f32, usize_to_f32};
use shared::Province;

use super::data::pop_needs;

/// Base growth rate per tick when needs are well met.
const BASE_GROWTH_RATE: f32 = 0.001;
/// Base decline rate per tick when needs are poorly met.
const BASE_DECLINE_RATE: f32 = 0.002;

/// Consume goods for all pops in a province, then update needs_satisfaction.
pub fn consume(province: &mut Province) {
    for pop in &mut province.pops {
        if pop.size == 0 {
            pop.needs_satisfaction = 0.0;
            continue;
        }

        let needs = pop_needs(pop.class);
        if needs.is_empty() {
            pop.needs_satisfaction = 1.0;
            continue;
        }

        // For each needed good, try to consume from stockpile.
        // Track the fraction of each need that was satisfied.
        let mut total_satisfaction = 0.0f32;
        let need_count = usize_to_f32(needs.len());

        for &(good, per_capita) in &needs {
            let wanted = per_capita * u32_to_f32(pop.size);
            if wanted <= 0.0 {
                total_satisfaction += 1.0;
                continue;
            }
            let available = province.stockpile.get(&good).copied().unwrap_or(0.0);
            let consumed = wanted.min(available);
            let stock = province.stockpile.entry(good).or_insert(0.0);
            *stock = (*stock - consumed).max(0.0);
            total_satisfaction += consumed / wanted;
        }

        pop.needs_satisfaction = (total_satisfaction / need_count).clamp(0.0, 1.0);
    }
}

/// Grow or shrink pops based on their needs_satisfaction.
pub fn grow(province: &mut Province) {
    for pop in &mut province.pops {
        if pop.size == 0 {
            continue;
        }

        let delta = if pop.needs_satisfaction > 0.5 {
            // Growth scales with how far above 0.5.
            let factor = (pop.needs_satisfaction - 0.5) * 2.0; // 0.0–1.0
            f32_to_i64((u32_to_f32(pop.size) * BASE_GROWTH_RATE * factor).ceil())
        } else if pop.needs_satisfaction < 0.3 {
            // Decline scales with how far below 0.3.
            let factor = (0.3 - pop.needs_satisfaction) / 0.3; // 0.0–1.0
            -f32_to_i64((u32_to_f32(pop.size) * BASE_DECLINE_RATE * factor).ceil())
        } else {
            0
        };

        pop.size = u32::try_from((i64::from(pop.size) + delta).max(0)).unwrap_or(0);
    }
}

/// Run consumption then growth for all provinces.
pub fn consume_and_grow_all(provinces: &mut [Province]) {
    for province in provinces.iter_mut() {
        consume(province);
        grow(province);
    }
}
