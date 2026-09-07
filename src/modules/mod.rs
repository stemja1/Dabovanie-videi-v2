//! Adaptéry jednotlivých pipeline modulov.
//!
//! Každý modul iba zostaví `ProcessSpec`; modely sa spúšťajú až neskôr cez
//! spoločný Tokio subprocess runner. Žiadny Python kód sa neimportuje cez
//! PyO3.

mod common;

pub mod cleanup;
pub mod ffmpeg;
pub mod latentsync;
pub mod planner;
pub mod translation;
pub mod tts;
pub mod whisper;

pub use planner::ConfiguredPlanner;
