use serde::{Deserialize, Serialize};
use std::{fmt, path::PathBuf};

/// Stabilný identifikátor segmentu naprieč ASR, prekladom a TTS.
pub type SegmentId = u64;

/// Stavy hlavnej pipeline.
///
/// Enum je zámerne bez dátových polí. Detail chyby, správa a percento
/// postupu sa prenášajú samostatnými eventmi, takže GUI môže stav bezpečne
/// serializovať aj zobraziť bez väzby na konkrétny backend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineState {
    #[default]
    Idle,
    Transcribing,
    Translating,
    Synthesizing,
    LipSyncing,
    Rendering,
    Failed,
    Completed,
}

impl PipelineState {
    /// Určuje, či je stav koncový pre aktuálnu úlohu.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Failed | Self::Completed)
    }

    /// Povolené prechody stavov na hranici orchestration engine.
    ///
    /// Opakovaný prechod do rovnakého stavu je povolený, pretože jeden stav
    /// môže produkovať viacero progress eventov.
    pub const fn can_transition_to(self, next: Self) -> bool {
        // Progress eventy nesmú meniť stav, ale môžu sa opakovať.
        if matches!(
            (self, next),
            (Self::Idle, Self::Idle)
                | (Self::Transcribing, Self::Transcribing)
                | (Self::Translating, Self::Translating)
                | (Self::Synthesizing, Self::Synthesizing)
                | (Self::LipSyncing, Self::LipSyncing)
                | (Self::Rendering, Self::Rendering)
                | (Self::Failed, Self::Failed)
                | (Self::Completed, Self::Completed)
        ) {
            return true;
        }

        matches!(
            (self, next),
            (Self::Idle, Self::Transcribing | Self::Failed)
                | (Self::Transcribing, Self::Translating | Self::Failed)
                | (Self::Translating, Self::Synthesizing | Self::Failed)
                | (Self::Synthesizing, Self::LipSyncing | Self::Failed)
                | (Self::LipSyncing, Self::Rendering | Self::Failed)
                | (Self::Rendering, Self::Completed | Self::Failed)
                | (Self::Failed | Self::Completed, Self::Idle)
        )
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Transcribing => "Transcribing",
            Self::Translating => "Translating",
            Self::Synthesizing => "Synthesizing",
            Self::LipSyncing => "Lip-syncing",
            Self::Rendering => "Rendering",
            Self::Failed => "Failed",
            Self::Completed => "Completed",
        }
    }
}

impl fmt::Display for PipelineState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// Úroveň logovacej správy zobraziteľná v GUI.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
}

/// Progress správa pre GUI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressUpdate {
    pub state: PipelineState,
    /// Normalizovaný priebeh v intervale 0.0 až 1.0.
    pub fraction: f32,
    #[serde(default)]
    pub message: String,
}

impl Default for ProgressUpdate {
    fn default() -> Self {
        Self {
            state: PipelineState::Idle,
            fraction: 0.0,
            message: String::new(),
        }
    }
}

impl ProgressUpdate {
    pub fn new(state: PipelineState, fraction: f32, message: impl Into<String>) -> Self {
        let fraction = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };

        Self {
            state,
            fraction,
            message: message.into(),
        }
    }
}

/// Vstupná požiadavka jedného behu pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineRequest {
    pub input_video: PathBuf,
    pub output_video: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_reference: Option<PathBuf>,
}

/// Cesty k medzivýsledkom jedného behu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineArtifacts {
    pub utterance_metadata: PathBuf,
    pub translated_metadata: PathBuf,
    pub synthesized_audio: PathBuf,
    pub lip_synced_video: PathBuf,
    pub final_video: PathBuf,
}

/// Jeden časovo zarovnaný ASR segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Utterance {
    pub id: SegmentId,
    pub start: f64,
    pub end: f64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translated_text: Option<String>,
}

impl Utterance {
    pub fn duration(&self) -> f64 {
        (self.end - self.start).max(0.0)
    }

    /// Ľahká doménová kontrola pred zápisom metadata JSON.
    pub fn is_valid(&self) -> bool {
        self.start.is_finite()
            && self.end.is_finite()
            && self.start >= 0.0
            && self.end >= self.start
            && !self.text.trim().is_empty()
    }
}

/// Obsah súboru `utterance_metadata.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UtteranceMetadata {
    #[serde(default)]
    pub utterances: Vec<Utterance>,
}

impl UtteranceMetadata {
    pub fn new(utterances: Vec<Utterance>) -> Self {
        Self { utterances }
    }

    pub fn is_valid(&self) -> bool {
        self.utterances.iter().all(Utterance::is_valid)
    }
}

/// Príkazy od GUI smerom k orchestration engine.
///
/// Samotný prenos bude v ďalšej fáze realizovaný cez
/// `tokio::sync::mpsc::Sender<EngineCommand>`.
#[derive(Debug, Clone)]
pub enum EngineCommand {
    Start(PipelineRequest),
    Cancel,
    UpdateTranslation {
        segment_id: SegmentId,
        translated_text: String,
    },
    Shutdown,
}

/// Eventy od pracovného runtime smerom ku GUI.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    StateChanged { state: PipelineState },
    Progress(ProgressUpdate),
    Log(LogEntry),
    SegmentUpdated(Utterance),
    Completed { output_video: PathBuf },
    Failed { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_state_accepts_only_ordered_transitions() {
        assert!(PipelineState::Idle.can_transition_to(PipelineState::Transcribing));
        assert!(PipelineState::Rendering.can_transition_to(PipelineState::Completed));
        assert!(PipelineState::Translating.can_transition_to(PipelineState::Failed));
        assert!(PipelineState::Completed.can_transition_to(PipelineState::Idle));
        assert!(!PipelineState::Idle.can_transition_to(PipelineState::Rendering));
        assert!(!PipelineState::Completed.can_transition_to(PipelineState::Transcribing));
    }

    #[test]
    fn progress_is_normalized() {
        let progress = ProgressUpdate::new(PipelineState::Rendering, 5.0, "test");
        assert_eq!(progress.fraction, 1.0);

        let progress = ProgressUpdate::new(PipelineState::Rendering, f32::NAN, "test");
        assert_eq!(progress.fraction, 0.0);
    }
}
