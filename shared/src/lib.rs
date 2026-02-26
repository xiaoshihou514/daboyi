use serde::{Deserialize, Serialize};

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

// ── World entities ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Province {
    pub id: u32,
    pub name: String,
    pub owner: Option<String>,
    pub population: u32,
}

// ── Top-level game state (server-authoritative) ──────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GameState {
    pub tick: u64,
    pub date: GameDate,
    pub provinces: Vec<Province>,
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
#[serde(tag = "type", content = "payload")]
pub enum ClientMsg {
    /// Advance simulation by one tick and return updated state.
    Tick,
    /// Queue a player order; applied on the next Tick.
    IssueOrder(Order),
}

/// Messages sent from server → client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ServerMsg {
    /// Full game state snapshot, returned after every Tick.
    StateSnapshot(GameState),
    /// Acknowledgement for IssueOrder.
    Ack,
}
