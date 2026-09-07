use super::common::{percentage_parser, python_spec, require_path, require_string};
use crate::{
    config::AppConfig,
    engine::{ProcessSpec, ProgressParserError},
    types::PipelineState,
};
use std::path::Path;
use thiserror::Error;

/// Rust adapter pre Whisper-SK spúšťaný v izolovanom venv.
#[derive(Debug, Clone, Copy)]
pub struct WhisperModule<'a> {
    config: &'a AppConfig,
}

impl<'a> WhisperModule<'a> {
    pub fn new(config: &'a AppConfig) -> Self {
        Self { config }
    }

    pub fn build_spec(
        &self,
        input_video: &Path,
        output_metadata: &Path,
    ) -> Result<ProcessSpec, WhisperError> {
        require_path(input_video, "Whisper input").map_err(WhisperError::InvalidConfiguration)?;
        require_path(output_metadata, "Whisper output")
            .map_err(WhisperError::InvalidConfiguration)?;
        require_string(&self.config.models.whisper_sk, "models.whisper_sk")
            .map_err(WhisperError::InvalidConfiguration)?;
        require_path(&self.config.scripts.whisper, "scripts.whisper")
            .map_err(WhisperError::InvalidConfiguration)?;

        let parser = percentage_parser(PipelineState::Transcribing)?;
        Ok(python_spec(
            "whisper-sk",
            PipelineState::Transcribing,
            &self.config.python.whisper_sk,
            &self.config.scripts.whisper,
            self.config,
        )
        .arg("--input")
        .arg(input_video)
        .arg("--output")
        .arg(output_metadata)
        .arg("--model")
        .arg(&self.config.models.whisper_sk)
        .arg("--language")
        .arg(&self.config.pipeline.source_language)
        .arg("--device")
        .arg("auto")
        .with_progress_parser(parser))
    }
}

#[derive(Debug, Error)]
pub enum WhisperError {
    #[error("neplatná Whisper konfigurácia: {0}")]
    InvalidConfiguration(String),

    #[error(transparent)]
    ProgressParser(#[from] ProgressParserError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn builds_isolated_whisper_command() {
        let config = AppConfig::default();
        let spec = WhisperModule::new(&config)
            .build_spec(
                Path::new("input.mp4"),
                Path::new("work/utterance_metadata.json"),
            )
            .unwrap();

        assert_eq!(spec.state, PipelineState::Transcribing);
        assert_eq!(spec.program, PathBuf::from("venvs/whisper-sk/bin/python"));
        assert!(spec.args.iter().any(|arg| arg == "--model"));
        assert!(spec
            .args
            .iter()
            .any(|arg| arg == "NaiveNeuron/whisper-large-v3-sk"));
    }
}
