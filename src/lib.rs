//! Doména aplikácie a zdieľané dátové typy.
//!
//! GUI, Tokio runtime a jednotlivé Python/FFmpeg adaptéry budú doplnené
//! v nasledujúcich fázach. Modulová hranica je zámerne oddelená už teraz,
//! aby GUI nebolo závislé od implementácie pipeline.

pub mod config;
pub mod error;
pub mod types;
