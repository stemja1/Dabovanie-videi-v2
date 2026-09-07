use super::{
    ffmpeg::FfmpegModule, latentsync::LatentSyncModule, translation::TranslationModule,
    tts::TtsModule, whisper::WhisperModule,
};
use crate::{
    config::AppConfig,
    engine::{PipelinePlan, PipelinePlanner, PlannerError},
    types::{CleanupPlan, PipelineArtifacts, PipelineRequest},
};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

/// Planner spájajúci konfiguráciu s konkrétnymi subprocess modulmi.
#[derive(Debug, Clone)]
pub struct ConfiguredPlanner {
    config: AppConfig,
}

impl ConfiguredPlanner {
    pub fn new(config: AppConfig) -> Result<Self, PlannerError> {
        config
            .validate()
            .map_err(|error| PlannerError::Invalid(error.to_string()))?;
        Ok(Self { config })
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    pub fn artifacts_for(
        &self,
        request: &PipelineRequest,
    ) -> Result<(PipelineArtifacts, CleanupPlan), PlannerError> {
        validate_request(request)?;

        let workspace = self.workspace_for(request);
        fs::create_dir_all(&workspace).map_err(PlannerError::Io)?;
        if let Some(parent) = request.output_video.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(PlannerError::Io)?;
            }
        }

        let artifacts = PipelineArtifacts {
            utterance_metadata: workspace.join("utterance_metadata.json"),
            translated_metadata: workspace.join("translated_metadata.json"),
            synthesized_audio: workspace.join("cinsky_dabbing.wav"),
            lip_synced_video: workspace.join("lip_synced.mp4"),
            final_video: request.output_video.clone(),
        };
        let cleanup = CleanupPlan {
            allowed_root: self.config.paths.temp_dir.clone(),
            workspace,
            on_success: self.config.pipeline.cleanup_on_success,
            on_failure: self.config.pipeline.cleanup_on_failure,
        };

        Ok((artifacts, cleanup))
    }

    fn workspace_for(&self, request: &PipelineRequest) -> PathBuf {
        let requested_id = request
            .job_id
            .as_deref()
            .or_else(|| {
                request
                    .output_video
                    .file_stem()
                    .and_then(|stem| stem.to_str())
            })
            .unwrap_or("job");
        let identifier = sanitize_component(requested_id);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        self.config
            .paths
            .temp_dir
            .join(format!("{identifier}-{}-{timestamp}", std::process::id()))
    }
}

impl PipelinePlanner for ConfiguredPlanner {
    fn build_plan(&self, request: &PipelineRequest) -> Result<PipelinePlan, PlannerError> {
        let (artifacts, cleanup) = self.artifacts_for(request)?;
        let whisper = WhisperModule::new(&self.config)
            .build_spec(&request.input_video, &artifacts.utterance_metadata)
            .map_err(module_error("whisper-sk"))?;
        let translation = TranslationModule::new(&self.config)
            .build_spec(
                &artifacts.utterance_metadata,
                &artifacts.translated_metadata,
            )
            .map_err(module_error("nllb-translation"))?;
        let tts = TtsModule::new(&self.config)
            .build_spec(
                &artifacts.translated_metadata,
                &artifacts.synthesized_audio,
                request.voice_reference.as_deref(),
            )
            .map_err(module_error("coqui-xtts"))?;
        let latentsync = LatentSyncModule::new(&self.config)
            .build_spec(
                &request.input_video,
                &artifacts.synthesized_audio,
                &artifacts.lip_synced_video,
            )
            .map_err(module_error("latentsync"))?;
        let ffmpeg = FfmpegModule::new(&self.config)
            .build_spec(
                &artifacts.lip_synced_video,
                &artifacts.synthesized_audio,
                &artifacts.final_video,
            )
            .map_err(module_error("ffmpeg-render"))?;

        Ok(
            PipelinePlan::new(vec![whisper, translation, tts, latentsync, ffmpeg])
                .with_cleanup(cleanup),
        )
    }
}

fn validate_request(request: &PipelineRequest) -> Result<(), PlannerError> {
    if request.input_video.as_os_str().is_empty() {
        return Err(PlannerError::Invalid(
            "input video cesta nesmie byť prázdna".to_owned(),
        ));
    }
    if request.output_video.as_os_str().is_empty() {
        return Err(PlannerError::Invalid(
            "output video cesta nesmie byť prázdna".to_owned(),
        ));
    }
    if request.input_video == request.output_video {
        return Err(PlannerError::Invalid(
            "input a output video nesmú byť rovnaký súbor".to_owned(),
        ));
    }
    Ok(())
}

fn sanitize_component(value: &str) -> String {
    let mut component: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    if component.is_empty() {
        component.push_str("job");
    }
    component
}

fn module_error<E>(module: &'static str) -> impl FnOnce(E) -> PlannerError
where
    E: std::fmt::Display,
{
    move |error| PlannerError::Module {
        module: module.to_owned(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PipelineState;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_config() -> AppConfig {
        let root = std::env::temp_dir().join(format!(
            "dabovanie-planner-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut config = AppConfig::default();
        config.paths.temp_dir = root.join("tmp");
        config.paths.output_dir = root.join("output");
        config
    }

    #[test]
    fn builds_all_pipeline_stages_and_cleanup_plan() {
        let planner = ConfiguredPlanner::new(test_config()).unwrap();
        let request = PipelineRequest {
            input_video: PathBuf::from("input.mp4"),
            output_video: PathBuf::from("output/final.mp4"),
            voice_reference: None,
            job_id: Some("demo job".to_owned()),
        };

        let plan = planner.build_plan(&request).unwrap();
        assert_eq!(plan.stages.len(), 5);
        assert_eq!(plan.stages[0].state, PipelineState::Transcribing);
        assert_eq!(plan.stages[4].state, PipelineState::Rendering);
        assert!(plan.cleanup.is_some());

        let cleanup = plan.cleanup.unwrap();
        assert!(cleanup.workspace.starts_with(&cleanup.allowed_root));
        let _ = fs::remove_dir_all(cleanup.allowed_root);
    }
}
