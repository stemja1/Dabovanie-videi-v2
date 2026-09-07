use super::common::require_path;
use crate::{config::AppConfig, engine::ProcessSpec, types::PipelineState};
use std::path::Path;
use thiserror::Error;

/// FFmpeg final render adapter.
#[derive(Debug, Clone, Copy)]
pub struct FfmpegModule<'a> {
    config: &'a AppConfig,
}

impl<'a> FfmpegModule<'a> {
    pub fn new(config: &'a AppConfig) -> Self {
        Self { config }
    }

    pub fn build_spec(
        &self,
        lip_synced_video: &Path,
        synthesized_audio: &Path,
        output_video: &Path,
    ) -> Result<ProcessSpec, FfmpegError> {
        require_path(lip_synced_video, "FFmpeg video input")
            .map_err(FfmpegError::InvalidConfiguration)?;
        require_path(synthesized_audio, "FFmpeg audio input")
            .map_err(FfmpegError::InvalidConfiguration)?;
        require_path(output_video, "FFmpeg output").map_err(FfmpegError::InvalidConfiguration)?;
        require_path(&self.config.paths.ffmpeg_binary, "paths.ffmpeg_binary")
            .map_err(FfmpegError::InvalidConfiguration)?;

        let mut spec = ProcessSpec::new("ffmpeg-render", self.config.paths.ffmpeg_binary.clone())
            .state(PipelineState::Rendering)
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("warning")
            .arg("-y")
            .arg("-i")
            .arg(lip_synced_video)
            .arg("-i")
            .arg(synthesized_audio)
            .arg("-map")
            .arg("0:v:0")
            .arg("-map")
            .arg("1:a:0")
            .arg("-c:v")
            .arg("copy")
            .arg("-c:a")
            .arg("aac")
            .arg("-shortest")
            .arg("-movflags")
            .arg("+faststart")
            .arg(output_video);

        for (key, value) in self.config.rocm.environment() {
            spec = spec.env(key, value);
        }
        Ok(spec)
    }
}

#[derive(Debug, Error)]
pub enum FfmpegError {
    #[error("neplatná FFmpeg konfigurácia: {0}")]
    InvalidConfiguration(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_video_and_audio_streams() {
        let config = AppConfig::default();
        let spec = FfmpegModule::new(&config)
            .build_spec(
                Path::new("lip_synced.mp4"),
                Path::new("cinsky_dabbing.wav"),
                Path::new("final.mp4"),
            )
            .unwrap();
        let args: Vec<_> = spec
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        assert!(args.windows(2).any(|pair| pair == ["-map", "0:v:0"]));
        assert!(args.windows(2).any(|pair| pair == ["-map", "1:a:0"]));
        assert!(args.windows(2).any(|pair| pair == ["-c:v", "copy"]));
    }
}
