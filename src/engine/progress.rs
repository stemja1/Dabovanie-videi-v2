use crate::types::{PipelineState, ProgressUpdate};
use regex::Regex;
use thiserror::Error;

/// Spôsob interpretácie číselnej hodnoty zachytenej regexom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressValue {
    Percent,
    Fraction,
}

/// Chyby vytvorenia regex progress parsera.
#[derive(Debug, Error)]
pub enum ProgressParserError {
    #[error("neplatný progress regex: {0}")]
    InvalidRegex(#[from] regex::Error),

    #[error("capture group pre progress hodnotu musí byť väčšia ako nula")]
    InvalidCaptureGroup,
}

/// Regex parser progress hlásení z stdout alebo stderr podprocesu.
#[derive(Debug, Clone)]
pub struct ProgressParser {
    state: PipelineState,
    regex: Regex,
    value_group: usize,
    value_kind: ProgressValue,
}

impl ProgressParser {
    /// Vytvorí parser pre vlastný regex.
    ///
    /// Regex musí mať v zadanom capture groupe číselnú progress hodnotu.
    pub fn new(
        state: PipelineState,
        pattern: &str,
        value_group: usize,
        value_kind: ProgressValue,
    ) -> Result<Self, ProgressParserError> {
        if value_group == 0 {
            return Err(ProgressParserError::InvalidCaptureGroup);
        }

        Ok(Self {
            state,
            regex: Regex::new(pattern)?,
            value_group,
            value_kind,
        })
    }

    /// Parser bežného formátu napr. `progress: 42.5%` alebo `completed 80%`.
    pub fn percentage(state: PipelineState) -> Result<Self, ProgressParserError> {
        Self::new(
            state,
            r"(?i)(?:progress|percent|completed?|step)[^0-9]*([0-9]{1,3}(?:\.[0-9]+)?)\s*%",
            1,
            ProgressValue::Percent,
        )
    }

    /// Parser pre nástroje, ktoré reportujú normalizovaný priebeh `0.0..1.0`.
    pub fn fraction(state: PipelineState) -> Result<Self, ProgressParserError> {
        Self::new(
            state,
            r"(?i)(?:progress|fraction|ratio)[^0-9]*((?:0?\.)?[0-9]+)",
            1,
            ProgressValue::Fraction,
        )
    }

    pub fn state(&self) -> PipelineState {
        self.state
    }

    /// Spracuje jeden riadok a vráti progress event, ak regex nájde zhodu.
    pub fn parse_line(&self, line: &str) -> Option<ProgressUpdate> {
        let captures = self.regex.captures(line)?;
        let raw_value = captures.get(self.value_group)?.as_str();
        let value = raw_value.parse::<f32>().ok()?;
        let fraction = match self.value_kind {
            ProgressValue::Percent => value / 100.0,
            ProgressValue::Fraction => value,
        };

        Some(ProgressUpdate::new(self.state, fraction, line.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_percentage_from_a_log_line() {
        let parser = ProgressParser::percentage(PipelineState::Transcribing).unwrap();
        let update = parser.parse_line("[whisper] progress: 42.5%").unwrap();

        assert_eq!(update.state, PipelineState::Transcribing);
        assert!((update.fraction - 0.425).abs() < 0.001);
        assert_eq!(update.message, "[whisper] progress: 42.5%");
    }

    #[test]
    fn ignores_lines_without_progress() {
        let parser = ProgressParser::percentage(PipelineState::Transcribing).unwrap();
        assert!(parser.parse_line("loading model").is_none());
    }
}
