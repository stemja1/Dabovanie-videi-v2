use super::process::{ProcessError, ProcessRunner, ProcessSpec};
use crate::{
    modules::cleanup::{cleanup_workspace, CleanupError},
    types::{
        CleanupPlan, EngineCommand, EngineEvent, LogEntry, LogLevel, PipelineRequest, PipelineState,
    },
};
use std::sync::Arc;
use thiserror::Error;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;

/// Plán pipeline dodaný konkrétnou implementáciou modulov.
#[derive(Debug, Clone, Default)]
pub struct PipelinePlan {
    pub stages: Vec<ProcessSpec>,
    pub cleanup: Option<CleanupPlan>,
}

impl PipelinePlan {
    pub fn new(stages: Vec<ProcessSpec>) -> Self {
        Self {
            stages,
            cleanup: None,
        }
    }

    pub fn with_cleanup(mut self, cleanup: CleanupPlan) -> Self {
        self.cleanup = Some(cleanup);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }
}

/// Rozhranie medzi orchestration engine a modulmi pipeline.
///
/// Fáza 2 poskytuje engine a subprocess runner. Fáza 3 dodá planner, ktorý
/// z `PipelineRequest` vytvorí Whisper → Translation → TTS → LipSync → FFmpeg
/// procesné špecifikácie.
pub trait PipelinePlanner: Send + Sync + 'static {
    fn build_plan(&self, request: &PipelineRequest) -> Result<PipelinePlan, PlannerError>;
}

/// Planner používaný dovtedy, kým nie sú zapojené moduly z Fázy 3.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopPlanner;

impl PipelinePlanner for NoopPlanner {
    fn build_plan(&self, _request: &PipelineRequest) -> Result<PipelinePlan, PlannerError> {
        Err(PlannerError::NotConfigured)
    }
}

#[derive(Debug, Error)]
pub enum PlannerError {
    #[error("pipeline planner ešte nie je nakonfigurovaný")]
    NotConfigured,

    #[error("pipeline plán neobsahuje žiadny proces")]
    EmptyPlan,

    #[error("modul `{module}` sa nepodarilo nakonfigurovať: {message}")]
    Module { module: String, message: String },

    #[error("pipeline planner I/O operácia zlyhala: {0}")]
    Io(#[from] std::io::Error),

    #[error("pipeline plán je neplatný: {0}")]
    Invalid(String),
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error(transparent)]
    Planner(#[from] PlannerError),

    #[error(transparent)]
    Process(#[from] ProcessError),

    #[error(transparent)]
    Cleanup(#[from] CleanupError),

    #[error("pipeline zlyhala ({pipeline_error}) a cleanup tiež zlyhal ({cleanup_error})")]
    PipelineAndCleanup {
        pipeline_error: String,
        cleanup_error: String,
    },

    #[error("neplatný prechod stavu pipeline: {from:?} -> {to:?}")]
    InvalidStateTransition {
        from: PipelineState,
        to: PipelineState,
    },

    #[error("event channel bol zatvorený")]
    EventChannelClosed,
}

/// Nastavenia kapacity kanálov a limitu zachytávaného výstupu.
#[derive(Debug, Clone, Copy)]
pub struct EngineConfig {
    pub command_capacity: usize,
    pub event_capacity: usize,
    pub max_captured_output_bytes: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            command_capacity: 32,
            event_capacity: 256,
            max_captured_output_bytes: 1024 * 1024,
        }
    }
}

/// Handle, ktorý vlastní GUI thread. Runtime a engine task bežia oddelene.
pub struct EngineHandle {
    command_sender: mpsc::Sender<EngineCommand>,
    event_receiver: mpsc::Receiver<EngineEvent>,
    shutdown: CancellationToken,
    _task: JoinHandle<()>,
}

impl EngineHandle {
    pub fn command_sender(&self) -> mpsc::Sender<EngineCommand> {
        self.command_sender.clone()
    }

    pub async fn send(
        &self,
        command: EngineCommand,
    ) -> Result<(), mpsc::error::SendError<EngineCommand>> {
        self.command_sender.send(command).await
    }

    pub async fn recv(&mut self) -> Option<EngineEvent> {
        self.event_receiver.recv().await
    }

    pub fn try_recv(&mut self) -> Result<EngineEvent, mpsc::error::TryRecvError> {
        self.event_receiver.try_recv()
    }
}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        // Najprv zrušíme aktívny job. JoinHandle zámerne neabortujeme,
        // aby engine stihol doručiť cancellation do subprocess runnera.
        self.shutdown.cancel();
    }
}

/// Spustí orchestration task v aktuálnom Tokio runtime.
///
/// GUI môže vlastniť iba `EngineHandle`; nesmie volať Python ani FFmpeg
/// priamo. Všetky príkazy a eventy idú cez Tokio mpsc kanály.
pub fn spawn_engine<P>(planner: P, config: EngineConfig) -> EngineHandle
where
    P: PipelinePlanner,
{
    let (command_sender, command_receiver) = mpsc::channel(config.command_capacity);
    let (event_sender, event_receiver) = mpsc::channel(config.event_capacity);
    let planner = Arc::new(planner);
    let shutdown = CancellationToken::new();
    let engine_shutdown = shutdown.clone();
    let task = tokio::spawn(run_engine(
        planner,
        config,
        command_receiver,
        event_sender,
        engine_shutdown,
    ));

    EngineHandle {
        command_sender,
        event_receiver,
        shutdown,
        _task: task,
    }
}

struct ActiveJob {
    cancellation: CancellationToken,
    completion_receiver: mpsc::Receiver<Result<PipelineRequest, EngineError>>,
}

async fn run_engine<P>(
    planner: Arc<P>,
    config: EngineConfig,
    mut command_receiver: mpsc::Receiver<EngineCommand>,
    event_sender: mpsc::Sender<EngineEvent>,
    shutdown: CancellationToken,
) where
    P: PipelinePlanner,
{
    let runner = ProcessRunner::new(config.max_captured_output_bytes);
    let mut active_job: Option<ActiveJob> = None;

    loop {
        if let Some(job) = active_job.as_mut() {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    job.cancellation.cancel();
                    let _ = job.completion_receiver.recv().await;
                    return;
                }
                command = command_receiver.recv() => {
                    match command {
                        Some(EngineCommand::Cancel) => job.cancellation.cancel(),
                        Some(EngineCommand::Shutdown) | None => {
                            job.cancellation.cancel();
                            let _ = job.completion_receiver.recv().await;
                            return;
                        }
                        Some(EngineCommand::Start(_)) => {
                            let _ = event_sender.send(EngineEvent::Log(LogEntry {
                                level: LogLevel::Warn,
                                message: "Pipeline už beží; nový Start bol ignorovaný.".to_owned(),
                                module: Some("engine".to_owned()),
                            })).await;
                        }
                        Some(EngineCommand::UpdateTranslation { .. }) => {
                            let _ = event_sender.send(EngineEvent::Log(LogEntry {
                                level: LogLevel::Debug,
                                message: "Úprava prekladu bude spracovaná v ďalšej fáze pipeline.".to_owned(),
                                module: Some("engine".to_owned()),
                            })).await;
                        }
                    }
                }
                result = job.completion_receiver.recv() => {
                    active_job = None;
                    match result {
                        Some(Ok(request)) => {
                            let _ = event_sender.send(EngineEvent::Completed {
                                output_video: request.output_video,
                            }).await;
                        }
                        Some(Err(error)) => {
                            let _ = event_sender.send(EngineEvent::StateChanged {
                                state: PipelineState::Failed,
                            }).await;
                            let _ = event_sender.send(EngineEvent::Failed {
                                message: error.to_string(),
                            }).await;
                        }
                        None => {
                            let _ = event_sender.send(EngineEvent::StateChanged {
                                state: PipelineState::Failed,
                            }).await;
                            let _ = event_sender.send(EngineEvent::Failed {
                                message: "Engine worker skončil bez výsledku.".to_owned(),
                            }).await;
                        }
                    }
                }
            }
        } else {
            let command = tokio::select! {
                _ = shutdown.cancelled() => return,
                command = command_receiver.recv() => command,
            };
            match command {
                Some(EngineCommand::Start(request)) => {
                    let cancellation = CancellationToken::new();
                    let worker_cancellation = cancellation.clone();
                    let (completion_sender, completion_receiver) = mpsc::channel(1);
                    let worker_planner = Arc::clone(&planner);
                    let worker_event_sender = event_sender.clone();
                    let worker_runner = runner;

                    tokio::spawn(async move {
                        let result = run_pipeline(
                            request,
                            worker_planner,
                            worker_runner,
                            worker_event_sender,
                            worker_cancellation,
                        )
                        .await;
                        let _ = completion_sender.send(result).await;
                    });

                    active_job = Some(ActiveJob {
                        cancellation,
                        completion_receiver,
                    });
                }
                Some(EngineCommand::Cancel) => {
                    let _ = event_sender
                        .send(EngineEvent::Log(LogEntry {
                            level: LogLevel::Debug,
                            message: "Cancel ignorovaný: pipeline je nečinná.".to_owned(),
                            module: Some("engine".to_owned()),
                        }))
                        .await;
                }
                Some(EngineCommand::UpdateTranslation { .. }) => {
                    let _ = event_sender
                        .send(EngineEvent::Log(LogEntry {
                            level: LogLevel::Debug,
                            message: "Úprava prekladu ignorovaná: pipeline je nečinná.".to_owned(),
                            module: Some("engine".to_owned()),
                        }))
                        .await;
                }
                Some(EngineCommand::Shutdown) | None => return,
            }
        }
    }
}

async fn run_pipeline<P>(
    request: PipelineRequest,
    planner: Arc<P>,
    runner: ProcessRunner,
    event_sender: mpsc::Sender<EngineEvent>,
    cancellation: CancellationToken,
) -> Result<PipelineRequest, EngineError>
where
    P: PipelinePlanner,
{
    let plan = planner.build_plan(&request)?;
    if plan.is_empty() {
        return Err(PlannerError::EmptyPlan.into());
    }

    let cleanup_plan = plan.cleanup.clone();
    let execution = execute_stages(plan.stages, runner, event_sender.clone(), cancellation).await;

    match execution {
        Ok(_) => {
            if let Some(cleanup_plan) = cleanup_plan.as_ref() {
                cleanup_workspace(cleanup_plan, true).await?;
            }

            event_sender
                .send(EngineEvent::StateChanged {
                    state: PipelineState::Completed,
                })
                .await
                .map_err(|_| EngineError::EventChannelClosed)?;

            Ok(request)
        }
        Err(pipeline_error) => {
            if let Some(cleanup_plan) = cleanup_plan.as_ref() {
                if let Err(cleanup_error) = cleanup_workspace(cleanup_plan, false).await {
                    return Err(EngineError::PipelineAndCleanup {
                        pipeline_error: pipeline_error.to_string(),
                        cleanup_error: cleanup_error.to_string(),
                    });
                }
            }
            Err(pipeline_error)
        }
    }
}

async fn execute_stages(
    stages: Vec<ProcessSpec>,
    runner: ProcessRunner,
    event_sender: mpsc::Sender<EngineEvent>,
    cancellation: CancellationToken,
) -> Result<PipelineState, EngineError> {
    let mut current_state = PipelineState::Idle;
    for stage in stages {
        if !current_state.can_transition_to(stage.state) {
            return Err(EngineError::InvalidStateTransition {
                from: current_state,
                to: stage.state,
            });
        }

        let stage_state = stage.state;
        event_sender
            .send(EngineEvent::StateChanged { state: stage_state })
            .await
            .map_err(|_| EngineError::EventChannelClosed)?;

        runner
            .run(stage, event_sender.clone(), cancellation.clone())
            .await?;
        current_state = stage_state;
    }

    if !current_state.can_transition_to(PipelineState::Completed) {
        return Err(EngineError::InvalidStateTransition {
            from: current_state,
            to: PipelineState::Completed,
        });
    }

    Ok(current_state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::progress::ProgressParser;
    use std::path::PathBuf;

    #[derive(Debug, Default, Clone, Copy)]
    struct TestPlanner;

    impl PipelinePlanner for TestPlanner {
        fn build_plan(&self, _request: &PipelineRequest) -> Result<PipelinePlan, PlannerError> {
            let parser = ProgressParser::percentage(PipelineState::Transcribing)
                .map_err(|error| PlannerError::Invalid(error.to_string()))?;
            let states = [
                PipelineState::Transcribing,
                PipelineState::Translating,
                PipelineState::Synthesizing,
                PipelineState::LipSyncing,
                PipelineState::Rendering,
            ];
            let stages = states
                .into_iter()
                .enumerate()
                .map(|(index, state)| {
                    let mut spec = ProcessSpec::new(format!("test-{index}"), PathBuf::from("sh"))
                        .state(state)
                        .args(["-c", "printf 'stage done\\n'"]);
                    if state == PipelineState::Transcribing {
                        spec = spec.with_progress_parser(parser.clone());
                    }
                    spec
                })
                .collect();

            Ok(PipelinePlan::new(stages))
        }
    }

    #[tokio::test]
    async fn engine_runs_plan_and_emits_completion() {
        let mut handle = spawn_engine(TestPlanner, EngineConfig::default());
        handle
            .send(EngineCommand::Start(PipelineRequest {
                input_video: PathBuf::from("input.mp4"),
                output_video: PathBuf::from("output.mp4"),
                voice_reference: None,
                job_id: None,
            }))
            .await
            .unwrap();

        let mut saw_completed = false;
        for _ in 0..20 {
            if let Some(event) = handle.recv().await {
                if matches!(event, EngineEvent::Completed { .. }) {
                    saw_completed = true;
                    break;
                }
            }
        }
        assert!(saw_completed);
    }

    #[tokio::test]
    async fn noop_planner_reports_failure() {
        let mut handle = spawn_engine(NoopPlanner, EngineConfig::default());
        handle
            .send(EngineCommand::Start(PipelineRequest {
                input_video: PathBuf::from("input.mp4"),
                output_video: PathBuf::from("output.mp4"),
                voice_reference: None,
                job_id: None,
            }))
            .await
            .unwrap();

        let mut saw_failure = false;
        for _ in 0..10 {
            if let Some(event) = handle.recv().await {
                if matches!(event, EngineEvent::Failed { .. }) {
                    saw_failure = true;
                    break;
                }
            }
        }
        assert!(saw_failure);
    }
}
