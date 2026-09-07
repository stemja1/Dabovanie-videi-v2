use crate::types::PipelineState;
use std::{io, path::PathBuf};
use thiserror::Error;

/// Chyby načítania, validácie a zápisu konfiguračného súboru.
///
/// Konfiguračný modul vracia túto konkrétnu chybu. Až aplikačná hranica
/// (v budúcnosti orchestration engine alebo `main`) ju môže zabaliť do
/// `anyhow::Error` pre diagnostiku s kontextom.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("konfiguračný súbor `{path}` sa nepodarilo načítať: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("konfiguračný súbor `{path}` obsahuje neplatný TOML: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("konfiguráciu sa nepodarilo serializovať do TOML: {source}")]
    Serialize {
        #[source]
        source: toml::ser::Error,
    },

    #[error("nepodarilo sa vytvoriť nadradený adresár pre `{path}`: {source}")]
    CreateParent {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("konfiguráciu sa nepodarilo zapísať do `{path}`: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("neplatná hodnota konfigurácie `{field}`: {message}")]
    InvalidField { field: String, message: String },
}

impl ConfigError {
    pub fn invalid(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InvalidField {
            field: field.into(),
            message: message.into(),
        }
    }
}

/// Aplikačné chyby zdieľané medzi doménovými modulmi.
///
/// Konkrétne adaptéry (Whisper, NLLB, XTTS, LatentSync a FFmpeg) budú mať
/// vlastné `thiserror` enumy a na tejto hranici sa budú konvertovať do
/// `AppError` alebo do `anyhow::Error`.
#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error("I/O operácia zlyhala: {0}")]
    Io(#[from] io::Error),

    #[error("JSON operácia zlyhala: {0}")]
    Json(#[from] serde_json::Error),

    #[error("neplatný prechod stavu pipeline: {from:?} -> {to:?}")]
    InvalidStateTransition {
        from: PipelineState,
        to: PipelineState,
    },

    #[error("modul `{module}` zlyhal: {message}")]
    Module { module: String, message: String },

    #[error("spracovanie pipeline bolo zrušené v module `{module}`")]
    Cancelled { module: String },
}

pub type AppResult<T> = std::result::Result<T, AppError>;
