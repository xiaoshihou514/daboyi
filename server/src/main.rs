use actix_web::{web, App, HttpServer};
use shared::map::MapData;
use shared::{GameState, Order};
use std::sync::Mutex;

mod db;
mod game;
mod ws;

const MAP_BIN_PATH: &str = "assets/map.bin";

pub struct AppState {
    pub game_state: Mutex<GameState>,
    pub command_queue: Mutex<Vec<Order>>,
    pub db: Mutex<db::GameDb>,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let db = db::GameDb::open("./daboyi.db").expect("Failed to open RocksDB");

    let initial_state = db
        .load_state()
        .expect("Failed to load game state")
        .unwrap_or_else(|| {
            if std::path::Path::new(MAP_BIN_PATH).exists() {
                println!("Loading map from {MAP_BIN_PATH}...");
                let map_data = MapData::load(MAP_BIN_PATH).expect("Failed to load map.bin");
                println!("Map loaded: {} provinces", map_data.provinces.len());
                game::data::generate_world(&map_data)
            } else {
                eprintln!("Warning: {MAP_BIN_PATH} not found. Run `python3 tools/download_gadm.py` then `cargo run -p mapgen` to generate it.");
                eprintln!("Starting with an empty world.");
                GameState::default()
            }
        });

    let state = web::Data::new(AppState {
        game_state: Mutex::new(initial_state),
        command_queue: Mutex::new(Vec::new()),
        db: Mutex::new(db),
    });

    println!("Server listening on ws://127.0.0.1:8080/ws");

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .route("/ws", web::get().to(ws::ws_handler))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
