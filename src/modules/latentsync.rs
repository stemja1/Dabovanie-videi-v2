use super::common::{percentage_parser, python_spec, require_path};
use crate::{
    config::AppConfig,
    engine::{ProcessSpec, ProgressParserError},
    types::PipelineState,
};
use std::path::Path;
use thiserror::Error;

/// Rust adapter pre LatentSync vrátane FFmpeg prípravy frames/audio.
#[derive(Debug, Clone, Copy)]
pub struct LatentSyncModule<'a> {
    config: &'a AppConfig,
}

impl<'a> LatentSyncModule<'a> {
    pub fn new(config: &'a AppConfig) -> Self {
        Self { config }
    }

    pub fn build_spec(
        &self,
        input_video: &Path,
        synthesized_audio: &Path,
        output_video: &Path,
    ) -> Result<ProcessSpec, LatentSyncError> {
        require_path(input_video, "LatentSync video input")
            .map_err(LatentSyncError::InvalidConfiguration)?;
        require_path(synthesized_audio, "LatentSync audio input")
            .map_err(LatentSyncError::InvalidConfiguration)?;
        require_path(output_video, "LatentSync output")
            .map_err(LatentSyncError::InvalidConfiguration)?;
        require_path(&self.config.models.latentsync, "models.latentsync")
            .map_err(LatentSyncError::InvalidConfiguration)?;
        require_path(&self.config.paths.ffmpeg_binary, "paths.ffmpeg_binary")
            .map_err(LatentSyncError::InvalidConfiguration)?;
        require_path(&self.config.scripts.latentsync, "scripts.latentsync")
            .map_err(LatentSyncError::InvalidConfiguration)?;

        let parser = percentage_parser(PipelineState::LipSyncing)?;
        Ok(python_spec(
            "latentsync",
            PipelineState::LipSyncing,
            &self.config.python.latentsync,
            &self.config.scripts.latentsync,
            self.config,
        )
        .arg("--video")
        .arg(input_video)
        .arg("--audio")
        .arg(synthesized_audio)
        .arg("--output")
        .arg(output_video)
        .arg("--latentsync-root")
        .arg(&self.config.models.latentsync)
        .arg("--ffmpeg")
        .arg(&self.config.paths.ffmpeg_binary)
        .arg("--device")
        .arg("auto")
        .with_progress_parser(parser))
    }
}

#[derive(Debug, Error)]
pub enum LatentSyncError {
    #[error("neplatná LatentSync konfigurácia: {0}")]
    InvalidConfiguration(String),

    #[error(transparent)]
    ProgressParser(#[from] ProgressParserError),
}
