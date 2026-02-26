use rocksdb::{Options, DB};
use shared::GameState;
use std::path::Path;

pub struct GameDb {
    db: DB,
}

impl GameDb {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, rocksdb::Error> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        Ok(Self { db: DB::open(&opts, path)? })
    }

    pub fn save_state(&self, state: &GameState) -> Result<(), Box<dyn std::error::Error>> {
        let bytes = serde_json::to_vec(state)?;
        self.db.put(b"game_state", bytes)?;
        Ok(())
    }

    pub fn load_state(&self) -> Result<Option<GameState>, Box<dyn std::error::Error>> {
        match self.db.get(b"game_state")? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }
}
