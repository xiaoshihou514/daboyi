use bevy::prelude::*;

/// Top-level application state.
#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppState {
    /// Waiting for map.bin / terrain.bin to load.
    #[default]
    Loading,
    /// Map editor is active.
    Editing,
}
