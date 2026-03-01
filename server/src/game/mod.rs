use shared::{GameDate, GameState, Order, OrderKind};

pub mod data;
pub mod load;
pub mod military;
pub mod params;
pub mod population;
pub mod production;

use params::{GOOD_PRICES, TAX_RATE};

/// Extension trait keeping game simulation logic server-side.
pub trait GameSimulation {
    fn apply_commands(&mut self, orders: Vec<Order>, player_country: Option<&str>);
    fn advance(&mut self);
}

impl GameSimulation for GameState {
    fn apply_commands(&mut self, orders: Vec<Order>, player_country: Option<&str>) {
        let Some(owner) = player_country else { return };
        for order in orders {
            match order.kind {
                OrderKind::BuildFarm { province_id } => {
                    let pid = province_id as usize;
                    if let Some(prov) = self.provinces.get_mut(pid) {
                        if let Some(b) = prov.buildings.iter_mut().find(|b| b.type_id == "farm") {
                            b.level += 1;
                        }
                    }
                }
                OrderKind::BuildKiln { province_id } => {
                    let pid = province_id as usize;
                    if let Some(prov) = self.provinces.get_mut(pid) {
                        if let Some(b) = prov.buildings.iter_mut().find(|b| b.type_id == "charcoal_kiln") {
                            b.level += 1;
                        }
                    }
                }
                OrderKind::RaiseArmy { province_id, size } => {
                    military::raise_army(self, owner, province_id, size);
                }
                OrderKind::MoveArmy { army_id, target_province_id } => {
                    military::move_army(self, owner, army_id, target_province_id);
                }
                OrderKind::DisbandArmy { army_id } => {
                    military::disband_army(self, owner, army_id);
                }
            }
        }
    }

    fn advance(&mut self) {
        self.tick += 1;
        advance_date(&mut self.date);

        // Economy ticks once per ~3 months (every 100 ticks/days).
        if self.tick % params::ECONOMY_TICK_INTERVAL == 0 {
            // Split borrow: provinces (mut) and building_types (shared) are disjoint fields.
            let (provinces, building_types) = (&mut self.provinces, &self.building_types);
            production::produce_all(provinces, building_types);
            population::consume_and_grow_all(provinces);
            collect_taxes(self);
        }

        military::resolve_combat(self);
    }
}

/// Collect taxes: each economy tick, each province pays TAX_RATE × stockpile_value to its owner.
/// Stockpile value = sum of quantities × their gold price.
fn collect_taxes(state: &mut GameState) {
    // Accumulate tax per country tag before mutating.
    let mut tax_map: std::collections::HashMap<String, f32> = std::collections::HashMap::new();

    for province in &state.provinces {
        let Some(owner) = province.owner.as_deref() else { continue };
        let value: f32 = province
            .stockpile
            .iter()
            .map(|(good, &qty)| {
                GOOD_PRICES.iter().find(|(g, _)| g == good).map(|(_, p)| *p).unwrap_or(1.0) * qty
            })
            .sum();
        *tax_map.entry(owner.to_string()).or_insert(0.0) += value * TAX_RATE;
    }

    for country in &mut state.countries {
        if let Some(&income) = tax_map.get(&country.tag) {
            country.treasury += income;
        }
    }
}

fn advance_date(date: &mut GameDate) {
    date.day += 1;
    if date.day > days_in_month(date.month, date.year) {
        date.day = 1;
        date.month += 1;
        if date.month > 12 {
            date.month = 1;
            date.year += 1;
        }
    }
}

fn days_in_month(month: u8, year: i32) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}
