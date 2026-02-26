use shared::*;
use std::collections::HashMap;

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

pub fn default_building_types() -> Vec<BuildingType> {
    vec![
        BuildingType {
            id: "farm".into(),
            name: "Farm".into(),
            worker_class: PopClass::TenantFarmer,
            workers_per_level: 1000,
            input: vec![],
            output: vec![(Good::Grain, 5.0)],
        },
        BuildingType {
            id: "yeoman_farm".into(),
            name: "Yeoman Farm".into(),
            worker_class: PopClass::Yeoman,
            workers_per_level: 500,
            input: vec![(Good::Tools, 0.2)],
            output: vec![(Good::Grain, 4.0)],
        },
        BuildingType {
            id: "textile_workshop".into(),
            name: "Textile Workshop".into(),
            worker_class: PopClass::PetitBourgeois,
            workers_per_level: 200,
            input: vec![(Good::Grain, 0.5)],
            output: vec![(Good::Clothing, 2.0)],
        },
        BuildingType {
            id: "mine".into(),
            name: "Mine".into(),
            worker_class: PopClass::TenantFarmer,
            workers_per_level: 500,
            input: vec![(Good::Tools, 0.3)],
            output: vec![(Good::Metal, 2.0)],
        },
        BuildingType {
            id: "charcoal_kiln".into(),
            name: "Charcoal Kiln".into(),
            worker_class: PopClass::TenantFarmer,
            workers_per_level: 300,
            input: vec![],
            output: vec![(Good::Fuel, 3.0)],
        },
        BuildingType {
            id: "smithy".into(),
            name: "Smithy".into(),
            worker_class: PopClass::PetitBourgeois,
            workers_per_level: 200,
            input: vec![(Good::Metal, 1.0), (Good::Fuel, 0.5)],
            output: vec![(Good::Tools, 1.5)],
        },
        BuildingType {
            id: "luxury_workshop".into(),
            name: "Luxury Workshop".into(),
            worker_class: PopClass::PetitBourgeois,
            workers_per_level: 100,
            input: vec![(Good::Metal, 0.5), (Good::Clothing, 0.5)],
            output: vec![(Good::Luxuries, 1.0)],
        },
        BuildingType {
            id: "sawmill".into(),
            name: "Sawmill".into(),
            worker_class: PopClass::TenantFarmer,
            workers_per_level: 400,
            input: vec![(Good::Tools, 0.2)],
            output: vec![(Good::BuildingMaterials, 2.0)],
        },
    ]
}

/// A small starter world for testing.
pub fn default_world() -> GameState {
    let building_types = default_building_types();

    let provinces = vec![
        Province {
            id: 1,
            name: "Zhongyuan".into(),
            owner: Some("Player".into()),
            pops: vec![
                Pop { class: PopClass::TenantFarmer, size: 5000, needs_satisfaction: 1.0 },
                Pop { class: PopClass::Yeoman, size: 2000, needs_satisfaction: 1.0 },
                Pop { class: PopClass::PetitBourgeois, size: 500, needs_satisfaction: 1.0 },
                Pop { class: PopClass::Landlord, size: 100, needs_satisfaction: 1.0 },
                Pop { class: PopClass::Nobility, size: 50, needs_satisfaction: 1.0 },
                Pop { class: PopClass::Bureaucrat, size: 80, needs_satisfaction: 1.0 },
                Pop { class: PopClass::Clergy, size: 60, needs_satisfaction: 1.0 },
                Pop { class: PopClass::Soldier, size: 200, needs_satisfaction: 1.0 },
                Pop { class: PopClass::Intelligentsia, size: 40, needs_satisfaction: 1.0 },
            ],
            buildings: vec![
                Building { type_id: "farm".into(), level: 3 },
                Building { type_id: "yeoman_farm".into(), level: 2 },
                Building { type_id: "textile_workshop".into(), level: 1 },
                Building { type_id: "charcoal_kiln".into(), level: 1 },
                Building { type_id: "smithy".into(), level: 1 },
            ],
            stockpile: HashMap::from([
                (Good::Grain, 50.0),
                (Good::Clothing, 10.0),
                (Good::Fuel, 5.0),
                (Good::Tools, 5.0),
                (Good::Metal, 3.0),
            ]),
        },
        Province {
            id: 2,
            name: "Jiangnan".into(),
            owner: Some("Player".into()),
            pops: vec![
                Pop { class: PopClass::TenantFarmer, size: 3000, needs_satisfaction: 1.0 },
                Pop { class: PopClass::Yeoman, size: 1500, needs_satisfaction: 1.0 },
                Pop { class: PopClass::PetitBourgeois, size: 800, needs_satisfaction: 1.0 },
                Pop { class: PopClass::Capitalist, size: 50, needs_satisfaction: 1.0 },
                Pop { class: PopClass::Landlord, size: 80, needs_satisfaction: 1.0 },
                Pop { class: PopClass::Nobility, size: 30, needs_satisfaction: 1.0 },
                Pop { class: PopClass::Clergy, size: 40, needs_satisfaction: 1.0 },
                Pop { class: PopClass::Soldier, size: 100, needs_satisfaction: 1.0 },
                Pop { class: PopClass::Intelligentsia, size: 60, needs_satisfaction: 1.0 },
            ],
            buildings: vec![
                Building { type_id: "farm".into(), level: 2 },
                Building { type_id: "yeoman_farm".into(), level: 2 },
                Building { type_id: "textile_workshop".into(), level: 2 },
                Building { type_id: "luxury_workshop".into(), level: 1 },
                Building { type_id: "mine".into(), level: 1 },
            ],
            stockpile: HashMap::from([
                (Good::Grain, 40.0),
                (Good::Clothing, 15.0),
                (Good::Metal, 5.0),
                (Good::Tools, 3.0),
                (Good::Luxuries, 2.0),
            ]),
        },
    ];

    GameState {
        tick: 0,
        date: GameDate::default(),
        provinces,
        building_types,
    }
}
