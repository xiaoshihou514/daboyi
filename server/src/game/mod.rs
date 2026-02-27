use shared::{GameDate, GameState, Order};

pub mod data;
pub mod population;
pub mod production;

/// Extension trait keeping game simulation logic server-side.
pub trait GameSimulation {
    fn apply_commands(&mut self, orders: Vec<Order>);
    fn advance(&mut self);
}

impl GameSimulation for GameState {
    fn apply_commands(&mut self, orders: Vec<Order>) {
        for order in orders {
            // TODO: dispatch order kinds (move_army, build, diplomacy, …)
            eprintln!("Applying order: {:?}", order);
        }
    }

    fn advance(&mut self) {
        self.tick += 1;
        advance_date(&mut self.date);

        // Economy ticks once per ~3 months (every 100 ticks/days).
        if self.tick % 100 == 0 {
            // SAFETY: building_types and provinces are disjoint fields — use a
            // pointer cast to avoid cloning the entire Vec<BuildingType> every tick.
            let bt_ptr = &self.building_types as *const Vec<shared::BuildingType>;
            let bt_ref = unsafe { &*bt_ptr };
            production::produce_all(&mut self.provinces, bt_ref);
            population::consume_and_grow_all(&mut self.provinces);
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
