use super::common::{percentage_parser, python_spec, require_path, require_string};
use crate::{
    config::AppConfig,
    engine::{ProcessSpec, ProgressParserError},
    types::PipelineState,
};
use std::path::Path;
use thiserror::Error;

/// Rust adapter pre NLLB-200 slovensko → čínsky preklad.
#[derive(Debug, Clone, Copy)]
pub struct TranslationModule<'a> {
    config: &'a AppConfig,
}

impl<'a> TranslationModule<'a> {
    pub fn new(config: &'a AppConfig) -> Self {
        Self { config }
    }

    pub fn build_spec(
        &self,
        input_metadata: &Path,
        output_metadata: &Path,
    ) -> Result<ProcessSpec, TranslationError> {
        require_path(input_metadata, "translation input")
            .map_err(TranslationError::InvalidConfiguration)?;
        require_path(output_metadata, "translation output")
            .map_err(TranslationError::InvalidConfiguration)?;
        require_string(&self.config.models.nllb_200, "models.nllb_200")
            .map_err(TranslationError::InvalidConfiguration)?;
        require_string(
            &self.config.pipeline.nllb_source_language,
            "pipeline.nllb_source_language",
        )
        .map_err(TranslationError::InvalidConfiguration)?;
        require_string(
            &self.config.pipeline.nllb_target_language,
            "pipeline.nllb_target_language",
        )
        .map_err(TranslationError::InvalidConfiguration)?;
        require_path(&self.config.scripts.translation, "scripts.translation")
            .map_err(TranslationError::InvalidConfiguration)?;

        let parser = percentage_parser(PipelineState::Translating)?;
        Ok(python_spec(
            "nllb-translation",
            PipelineState::Translating,
            &self.config.python.translation,
            &self.config.scripts.translation,
            self.config,
        )
        .arg("--input")
        .arg(input_metadata)
        .arg("--output")
        .arg(output_metadata)
        .arg("--model")
        .arg(&self.config.models.nllb_200)
        .arg("--source-lang")
        .arg(&self.config.pipeline.nllb_source_language)
        .arg("--target-lang")
        .arg(&self.config.pipeline.nllb_target_language)
        .arg("--device")
        .arg("auto")
        .with_progress_parser(parser))
    }
}

#[derive(Debug, Error)]
pub enum TranslationError {
    #[error("neplatná translation konfigurácia: {0}")]
    InvalidConfiguration(String),

    #[error(transparent)]
    ProgressParser(#[from] ProgressParserError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_nllb_language_codes_in_command() {
        let config = AppConfig::default();
        let spec = TranslationModule::new(&config)
            .build_spec(
                Path::new("utterance_metadata.json"),
                Path::new("translated_metadata.json"),
            )
            .unwrap();
        let args: Vec<_> = spec
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        assert!(args
            .windows(2)
            .any(|pair| pair == ["--source-lang", "slk_Latn"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--target-lang", "zho_Hans"]));
    }
}
