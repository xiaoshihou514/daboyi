use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod conv;
pub mod map;

// ── Date ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl Default for GameDate {
    fn default() -> Self {
        // EU4-style start date
        Self { year: 1444, month: 11, day: 11 }
    }
}

// ── Goods ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Good {
    Grain,              // 粮食
    Clothing,           // 衣物
    Fuel,               // 燃料
    Tools,              // 工具
    Luxuries,           // 奢侈品
    Metal,              // 金属
    BuildingMaterials,  // 建筑材料
}

impl std::fmt::Display for Good {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Good::Grain => write!(f, "Grain"),
            Good::Clothing => write!(f, "Clothing"),
            Good::Fuel => write!(f, "Fuel"),
            Good::Tools => write!(f, "Tools"),
            Good::Luxuries => write!(f, "Luxuries"),
            Good::Metal => write!(f, "Metal"),
            Good::BuildingMaterials => write!(f, "Building Materials"),
        }
    }
}

/// Shorthand for a goods bundle (e.g. stockpile, recipe input/output).
pub type GoodsBundle = HashMap<Good, f32>;

// ── Population ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PopClass {
    TenantFarmer,   // 佃农
    Yeoman,         // 自耕农
    Landlord,       // 地主
    Capitalist,     // 资本家
    PetitBourgeois, // 小资产阶级
    Clergy,         // 宗教贵族
    Bureaucrat,     // 官僚
    Nobility,       // 世俗贵族
    Soldier,        // 军队
    Intelligentsia, // 知识分子
}

impl std::fmt::Display for PopClass {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PopClass::TenantFarmer => write!(f, "Tenant Farmer"),
            PopClass::Yeoman => write!(f, "Yeoman"),
            PopClass::Landlord => write!(f, "Landlord"),
            PopClass::Capitalist => write!(f, "Capitalist"),
            PopClass::PetitBourgeois => write!(f, "Petit Bourgeois"),
            PopClass::Clergy => write!(f, "Clergy"),
            PopClass::Bureaucrat => write!(f, "Bureaucrat"),
            PopClass::Nobility => write!(f, "Nobility"),
            PopClass::Soldier => write!(f, "Soldier"),
            PopClass::Intelligentsia => write!(f, "Intelligentsia"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pop {
    pub class: PopClass,
    pub size: u32,
    /// 0.0–1.0, how well this pop's needs are being met.
    pub needs_satisfaction: f32,
}

// ── Buildings ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingType {
    pub id: String,
    pub name: String,
    pub worker_class: PopClass,
    pub workers_per_level: u32,
    pub input: Vec<(Good, f32)>,
    pub output: Vec<(Good, f32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Building {
    pub type_id: String,
    pub level: u32,
}

// ── World entities ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Province {
    pub id: u32,
    pub name: String,
    pub owner: Option<String>,
    pub pops: Vec<Pop>,
    pub buildings: Vec<Building>,
    pub stockpile: GoodsBundle,
}

// ── Top-level game state (server-authoritative) ──────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GameState {
    pub tick: u64,
    pub date: GameDate,
    pub provinces: Vec<Province>,
    pub building_types: Vec<BuildingType>,
}

// ── Player commands ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub kind: String,
    pub target_province: Option<u32>,
}

// ── WebSocket messages ───────────────────────────────────────────────────────

/// Messages sent from client → server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMsg {
    /// Advance simulation by one tick and return updated state.
    Tick,
    /// Queue a player order; applied on the next Tick.
    IssueOrder(Order),
}

/// Messages sent from server → client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMsg {
    /// Full game state snapshot, returned after every Tick.
    StateSnapshot(GameState),
    /// Acknowledgement for IssueOrder.
    Ack,
}
