//! Tokio orchestration a subprocess vrstva.
//!
//! Táto vrstva neimportuje Python modely. Každý model bude v ďalších fázach
//! spustený ako samostatný proces cez `tokio::process::Command`.

mod orchestrator;
mod process;
mod progress;

pub use orchestrator::{
    spawn_engine, EngineConfig, EngineError, EngineHandle, NoopPlanner, PipelinePlan,
    PipelinePlanner, PlannerError,
};
pub use process::{OutputStream, ProcessError, ProcessResult, ProcessRunner, ProcessSpec};
pub use progress::{ProgressParser, ProgressParserError, ProgressValue};
