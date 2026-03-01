/// Game world parameters for 1356 CE starting conditions.
/// Centralizes magic numbers so tuning is done in one place.

use shared::{Good, PopClass};

// ── Economy tick ──────────────────────────────────────────────────────────────

/// Number of game-days between economy simulation ticks (population + production).
pub const ECONOMY_TICK_INTERVAL: u64 = 100;

// ── Tax ───────────────────────────────────────────────────────────────────────

/// Fraction of each province's production value collected as tax each economy tick.
pub const TAX_RATE: f32 = 0.20;

/// Gold price per unit of each good (for computing production value for tax).
pub const GOOD_PRICES: &[(Good, f32)] = &[
    (Good::Grain, 1.0),
    (Good::Clothing, 3.0),
    (Good::Fuel, 1.0),
    (Good::Tools, 5.0),
    (Good::Luxuries, 10.0),
    (Good::Metal, 4.0),
    (Good::BuildingMaterials, 2.0),
];

// ── Military ──────────────────────────────────────────────────────────────────

/// Treasury cost per soldier when raising an army.
pub const RAISE_COST_PER_SOLDIER: f32 = 0.01;

/// Fraction of army size lost by each side per combat tick (both attacker and defender).
pub const COMBAT_ATTRITION: f32 = 0.10;

// ── Population density ────────────────────────────────────────────────────────

/// Base population density (people per degree²) by topography.
pub fn topo_density(topography: &str) -> f64 {
    match topography {
        "flatland" => 80_000.0,
        "hills" => 40_000.0,
        "plateau" => 25_000.0,
        "wetlands" => 20_000.0,
        "mountains" => 8_000.0,
        _ => 5_000.0,
    }
}

/// Vegetation population multiplier.
pub fn veg_multiplier(vegetation: &str) -> f64 {
    match vegetation {
        "farmland" => 3.0,
        "grasslands" => 1.5,
        "woods" => 0.8,
        "forest" => 0.5,
        "sparse" => 0.3,
        "jungle" => 0.4,
        "desert" => 0.05,
        _ => 0.5,
    }
}

/// Climate population multiplier.
pub fn climate_multiplier(climate: &str) -> f64 {
    match climate {
        "subtropical" => 1.8,
        "tropical" => 1.2,
        "mediterranean" => 1.6,
        "oceanic" => 1.3,
        "continental" => 1.0,
        "arid" => 0.2,
        "cold_arid" => 0.15,
        "arctic" => 0.05,
        _ => 0.5,
    }
}

/// Minimum province population (floor for terrain estimate).
pub const MIN_PROVINCE_POP: f64 = 10.0;

// ── Medieval class distribution ratios (1356 CE) ──────────────────────────────
/// Overwhelmingly agricultural; small merchant/artisan class.
pub const CLASS_RATIOS: &[(PopClass, f64)] = &[
    (PopClass::TenantFarmer,   0.500),
    (PopClass::Yeoman,         0.250),
    (PopClass::PetitBourgeois, 0.060),
    (PopClass::Soldier,        0.040),
    (PopClass::Landlord,       0.040),
    (PopClass::Clergy,         0.040),
    (PopClass::Bureaucrat,     0.030),
    (PopClass::Intelligentsia, 0.020),
    (PopClass::Nobility,       0.015),
    (PopClass::Capitalist,     0.005),
];

// ── Building level formulae ───────────────────────────────────────────────────

/// Starting farm level = population / this value (min 1).
pub const FARM_POP_PER_LEVEL: u32 = 1500;

/// Starting charcoal kiln level = population / this value (min 1).
pub const KILN_POP_PER_LEVEL: u32 = 5000;

// ── Initial stockpile ─────────────────────────────────────────────────────────

pub const INIT_GRAIN: f32 = 20.0;
pub const INIT_CLOTHING: f32 = 5.0;
pub const INIT_FUEL: f32 = 3.0;
/// Bonus goods added when a province has the matching raw_material.
pub const RAW_MATERIAL_BONUS: f32 = 15.0;

// ── Pop needs (per capita per economy tick) ───────────────────────────────────

/// Goods each pop class needs per capita per tick.
pub fn pop_needs(class: PopClass) -> Vec<(Good, f32)> {
    match class {
        PopClass::TenantFarmer => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.0005),
        ],
        PopClass::Yeoman => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.0005),
            (Good::Tools, 0.0003),
        ],
        PopClass::Landlord => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.001),
            (Good::Luxuries, 0.001),
        ],
        PopClass::Capitalist => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.001),
            (Good::Luxuries, 0.002),
        ],
        PopClass::PetitBourgeois => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.0008),
            (Good::Tools, 0.0005),
        ],
        PopClass::Clergy => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.001),
            (Good::Luxuries, 0.0005),
        ],
        PopClass::Bureaucrat => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.001),
            (Good::Luxuries, 0.0005),
        ],
        PopClass::Nobility => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.001),
            (Good::Luxuries, 0.003),
        ],
        PopClass::Soldier => vec![
            (Good::Grain, 0.003),
            (Good::Clothing, 0.001),
            (Good::Fuel, 0.0005),
        ],
        PopClass::Intelligentsia => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.001),
            (Good::Luxuries, 0.001),
        ],
    }
}
