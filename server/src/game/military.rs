use shared::{Army, GameState};

use crate::game::params::{COMBAT_ATTRITION, RAISE_COST_PER_SOLDIER};
use shared::conv::f32_to_u32;

/// Raise a new army in `province_id` for the player country `owner`.
/// Costs `size * RAISE_COST_PER_SOLDIER` treasury. Silently rejects if province
/// is not owned by `owner` or if treasury is insufficient.
pub fn raise_army(state: &mut GameState, owner: &str, province_id: u32, size: u32) {
    let pid = province_id as usize;
    if pid >= state.provinces.len() {
        return;
    }
    if state.provinces[pid].owner.as_deref() != Some(owner) {
        return;
    }

    let cost = (size as f32) * RAISE_COST_PER_SOLDIER;
    let Some(country) = state.countries.iter_mut().find(|c| c.tag == owner) else {
        return;
    };
    if country.treasury < cost {
        return;
    }
    country.treasury -= cost;

    let new_id = state.armies.iter().map(|a| a.id).max().unwrap_or(0) + 1;
    state.armies.push(Army {
        id: new_id,
        owner: owner.to_string(),
        province_id,
        size,
    });
}

/// Move an army to a target province (instant). Rejects if army doesn't belong to `owner`.
pub fn move_army(state: &mut GameState, owner: &str, army_id: u64, target_province_id: u32) {
    if target_province_id as usize >= state.provinces.len() {
        return;
    }
    if let Some(army) = state.armies.iter_mut().find(|a| a.id == army_id) {
        if army.owner == owner {
            army.province_id = target_province_id;
        }
    }
}

/// Disband an army, returning half its treasury cost.
pub fn disband_army(state: &mut GameState, owner: &str, army_id: u64) {
    let Some(idx) = state.armies.iter().position(|a| a.id == army_id && a.owner == owner) else {
        return;
    };
    let army = state.armies.remove(idx);
    let refund = (army.size as f32) * RAISE_COST_PER_SOLDIER * 0.5;
    if let Some(country) = state.countries.iter_mut().find(|c| c.tag == owner) {
        country.treasury += refund;
    }
}

/// Resolve combat for all provinces where armies of different owners are present.
/// Both sides take `COMBAT_ATTRITION` casualties per call.
/// When a side is eliminated, province ownership transfers to the surviving owner.
pub fn resolve_combat(state: &mut GameState) {
    use std::collections::{HashMap, HashSet};

    // Group army indices by province.
    let mut prov_armies: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, army) in state.armies.iter().enumerate() {
        prov_armies.entry(army.province_id).or_default().push(i);
    }

    // Collect which army ids are destroyed and which provinces are conquered.
    let mut destroyed: HashSet<u64> = HashSet::new();
    let mut conquests: Vec<(u32, String)> = Vec::new(); // (province_id, new_owner)

    for (pid, indices) in &prov_armies {
        let distinct_owners: HashSet<&str> =
            indices.iter().map(|&i| state.armies[i].owner.as_str()).collect();
        if distinct_owners.len() < 2 {
            continue;
        }

        // Apply attrition to all armies in a contested province.
        for &i in indices {
            let casualties = f32_to_u32((state.armies[i].size as f32 * COMBAT_ATTRITION).ceil());
            if state.armies[i].size <= casualties {
                destroyed.insert(state.armies[i].id);
                state.armies[i].size = 0;
            } else {
                state.armies[i].size -= casualties;
            }
        }

        // After attrition: if only one owner's armies survive, they conquer the province.
        let surviving: Vec<&str> = indices
            .iter()
            .filter(|&&i| !destroyed.contains(&state.armies[i].id))
            .map(|&i| state.armies[i].owner.as_str())
            .collect();
        let surviving_owners: HashSet<&str> = surviving.iter().copied().collect();
        if surviving_owners.len() == 1 {
            let winner = surviving[0].to_string();
            let current_owner = state.provinces[*pid as usize].owner.as_deref();
            if current_owner != Some(winner.as_str()) {
                conquests.push((*pid, winner));
            }
        }
    }

    // Apply province conquests.
    for (pid, new_owner) in conquests {
        state.provinces[pid as usize].owner = Some(new_owner);
    }

    // Remove destroyed armies.
    state.armies.retain(|a| !destroyed.contains(&a.id));
}
