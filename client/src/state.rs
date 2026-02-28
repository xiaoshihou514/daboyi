use bevy::prelude::*;

/// Top-level game flow state.
#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppState {
    /// Full-screen title / splash screen.
    #[default]
    StartScreen,
    /// Political map shown; player picks which country to play.
    CountrySelection,
    /// Normal gameplay.
    Playing,
}

/// The country tag the player chose to control.
#[derive(Resource, Default)]
pub struct PlayerCountry(pub Option<String>);
