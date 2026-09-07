use super::common::{percentage_parser, python_spec, require_path, require_string};
use crate::{
    config::AppConfig,
    engine::{ProcessSpec, ProgressParserError},
    types::PipelineState,
};
use std::path::Path;
use thiserror::Error;

/// Rust adapter pre Coqui XTTS-v2.
#[derive(Debug, Clone, Copy)]
pub struct TtsModule<'a> {
    config: &'a AppConfig,
}

impl<'a> TtsModule<'a> {
    pub fn new(config: &'a AppConfig) -> Self {
        Self { config }
    }

    pub fn build_spec(
        &self,
        input_metadata: &Path,
        output_audio: &Path,
        voice_reference: Option<&Path>,
    ) -> Result<ProcessSpec, TtsError> {
        require_path(input_metadata, "TTS input").map_err(TtsError::InvalidConfiguration)?;
        require_path(output_audio, "TTS output").map_err(TtsError::InvalidConfiguration)?;
        require_string(&self.config.models.xtts_v2, "models.xtts_v2")
            .map_err(TtsError::InvalidConfiguration)?;
        require_string(
            &self.config.pipeline.target_language,
            "pipeline.target_language",
        )
        .map_err(TtsError::InvalidConfiguration)?;
        if self.config.pipeline.max_tts_chars == 0 {
            return Err(TtsError::InvalidConfiguration(
                "pipeline.max_tts_chars musí byť väčšie ako nula".to_owned(),
            ));
        }
        require_path(&self.config.scripts.tts, "scripts.tts")
            .map_err(TtsError::InvalidConfiguration)?;
        if let Some(reference) = voice_reference {
            require_path(reference, "TTS voice reference")
                .map_err(TtsError::InvalidConfiguration)?;
        }

        let parser = percentage_parser(PipelineState::Synthesizing)?;
        let mut spec = python_spec(
            "coqui-xtts",
            PipelineState::Synthesizing,
            &self.config.python.coqui_xtts,
            &self.config.scripts.tts,
            self.config,
        )
        .arg("--input")
        .arg(input_metadata)
        .arg("--output")
        .arg(output_audio)
        .arg("--model")
        .arg(&self.config.models.xtts_v2)
        .arg("--language")
        .arg(&self.config.pipeline.target_language)
        .arg("--max-chars")
        .arg(self.config.pipeline.max_tts_chars.to_string())
        .arg("--device")
        .arg("auto");

        if let Some(reference) = voice_reference {
            spec = spec.arg("--speaker-wav").arg(reference);
        }

        Ok(spec.with_progress_parser(parser))
    }
}

/// Odstráni riadiace znaky a zjednotí whitespace pred odovzdaním XTTS.
pub fn sanitize_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut previous_was_space = false;

    for character in input.chars() {
        if character.is_control() || character.is_whitespace() {
            if !previous_was_space {
                output.push(' ');
                previous_was_space = true;
            }
        } else {
            output.push(character);
            previous_was_space = false;
        }
    }

    output.trim().to_owned()
}

/// Rozdelí dlhý text na chunk-y s preferenciou prirodzených hraníc viet.
pub fn split_text(input: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 {
        return Vec::new();
    }

    let clean = sanitize_text(input);
    let characters: Vec<char> = clean.chars().collect();
    if characters.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    while start < characters.len() {
        let hard_end = (start + max_chars).min(characters.len());
        let mut end = hard_end;

        if hard_end < characters.len() {
            let preferred_start = start + max_chars / 2;
            for index in (preferred_start..hard_end).rev() {
                if is_sentence_boundary(characters[index]) {
                    end = index + 1;
                    break;
                }
            }
        }

        let chunk: String = characters[start..end].iter().collect();
        let chunk = chunk.trim();
        if !chunk.is_empty() {
            chunks.push(chunk.to_owned());
        }
        start = end;
    }

    chunks
}

fn is_sentence_boundary(character: char) -> bool {
    matches!(
        character,
        '.' | ',' | '!' | '?' | ';' | ':' | '。' | '，' | '！' | '？' | '；' | '：' | '、'
    )
}

#[derive(Debug, Error)]
pub enum TtsError {
    #[error("neplatná TTS konfigurácia: {0}")]
    InvalidConfiguration(String),

    #[error(transparent)]
    ProgressParser(#[from] ProgressParserError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_control_characters_and_whitespace() {
        assert_eq!(sanitize_text("  A\n\tB\r\n"), "A B");
    }

    #[test]
    fn splits_text_without_breaking_utf8_characters() {
        let chunks = split_text("Prvá veta. Druhá veta. 中文文本。", 12);
        assert!(chunks.len() >= 2);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 12));
    }
}
