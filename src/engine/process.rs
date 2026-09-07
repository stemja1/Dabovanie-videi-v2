use super::progress::ProgressParser;
use crate::types::{EngineEvent, LogEntry, LogLevel, PipelineState};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    io,
    path::PathBuf,
    process::{ExitStatus, Stdio},
};
use thiserror::Error;
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, BufReader},
    process::Command,
    sync::mpsc,
};
use tokio_util::sync::CancellationToken;

/// Popis jedného externého podprocesu.
///
/// Štruktúra je zámerne nezávislá od konkrétneho modelu. Fáza 3 ju použije
/// pre Whisper, NLLB, XTTS, LatentSync aj FFmpeg.
#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub module: String,
    pub state: PipelineState,
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub current_dir: Option<PathBuf>,
    pub environment: BTreeMap<OsString, OsString>,
    pub progress_parser: Option<ProgressParser>,
}

impl ProcessSpec {
    pub fn new(module: impl Into<String>, program: impl Into<PathBuf>) -> Self {
        Self {
            module: module.into(),
            state: PipelineState::Idle,
            program: program.into(),
            args: Vec::new(),
            current_dir: None,
            environment: BTreeMap::new(),
            progress_parser: None,
        }
    }

    pub fn state(mut self, state: PipelineState) -> Self {
        self.state = state;
        self
    }

    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|arg| arg.as_ref().to_os_string()));
        self
    }

    pub fn current_dir(mut self, current_dir: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(current_dir.into());
        self
    }

    pub fn env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.environment
            .insert(key.as_ref().to_os_string(), value.as_ref().to_os_string());
        self
    }

    pub fn with_progress_parser(mut self, parser: ProgressParser) -> Self {
        self.progress_parser = Some(parser);
        self
    }
}

/// Výsledok úspešne ukončeného procesu.
#[derive(Debug)]
pub struct ProcessResult {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

impl ProcessResult {
    pub fn success(&self) -> bool {
        self.status.success()
    }
}

/// Chyby subprocess runnera.
#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("modul `{module}` sa nepodarilo spustiť: {source}")]
    Spawn {
        module: String,
        #[source]
        source: io::Error,
    },

    #[error("modul `{module}` nemá dostupný {stream} pipe")]
    MissingPipe {
        module: String,
        stream: OutputStream,
    },

    #[error("čítanie {stream} modulu `{module}` zlyhalo: {source}")]
    Read {
        module: String,
        stream: OutputStream,
        #[source]
        source: io::Error,
    },

    #[error("čakanie na modul `{module}` zlyhalo: {source}")]
    Wait {
        module: String,
        #[source]
        source: io::Error,
    },

    #[error("ukončenie modulu `{module}` po zrušení zlyhalo: {source}")]
    Kill {
        module: String,
        #[source]
        source: io::Error,
    },

    #[error("spracovanie modulu `{module}` bolo zrušené")]
    Cancelled { module: String },

    #[error("event channel pre modul `{module}` bol zatvorený")]
    EventChannelClosed { module: String },

    #[error("modul `{module}` skončil s kódom {status}; stdout: {stdout}; stderr: {stderr}")]
    Failed {
        module: String,
        status: ExitStatus,
        stdout: String,
        stderr: String,
    },
}

/// Stream, z ktorého pochádza riadok podprocesu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

impl std::fmt::Display for OutputStream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stdout => formatter.write_str("stdout"),
            Self::Stderr => formatter.write_str("stderr"),
        }
    }
}

/// Asynchrónny runner externých procesov.
#[derive(Debug, Clone, Copy)]
pub struct ProcessRunner {
    max_captured_output_bytes: usize,
}

impl Default for ProcessRunner {
    fn default() -> Self {
        Self {
            max_captured_output_bytes: 1024 * 1024,
        }
    }
}

impl ProcessRunner {
    pub fn new(max_captured_output_bytes: usize) -> Self {
        Self {
            max_captured_output_bytes,
        }
    }

    /// Spustí proces, súčasne číta stdout aj stderr a streamuje eventy do GUI.
    pub async fn run(
        &self,
        spec: ProcessSpec,
        event_sender: mpsc::Sender<EngineEvent>,
        cancellation: CancellationToken,
    ) -> Result<ProcessResult, ProcessError> {
        let module = spec.module.clone();
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(current_dir) = &spec.current_dir {
            command.current_dir(current_dir);
        }
        for (key, value) in &spec.environment {
            command.env(key, value);
        }

        let mut child = command.spawn().map_err(|source| ProcessError::Spawn {
            module: module.clone(),
            source,
        })?;

        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                let _ = child.kill().await;
                return Err(ProcessError::MissingPipe {
                    module,
                    stream: OutputStream::Stdout,
                });
            }
        };
        let stderr = match child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                let _ = child.kill().await;
                return Err(ProcessError::MissingPipe {
                    module,
                    stream: OutputStream::Stderr,
                });
            }
        };

        let (line_sender, mut line_receiver) = mpsc::channel(64);
        let stdout_task = tokio::spawn(forward_lines(
            BufReader::new(stdout),
            OutputStream::Stdout,
            line_sender.clone(),
        ));
        let stderr_task = tokio::spawn(forward_lines(
            BufReader::new(stderr),
            OutputStream::Stderr,
            line_sender,
        ));

        let mut status = None;
        let mut stdout_closed = false;
        let mut stderr_closed = false;
        let mut stdout_buffer = String::new();
        let mut stderr_buffer = String::new();
        let mut stdout_truncated = false;
        let mut stderr_truncated = false;

        while status.is_none() || !stdout_closed || !stderr_closed {
            tokio::select! {
                _ = cancellation.cancelled() => {
                    child.kill().await.map_err(|source| ProcessError::Kill {
                        module: module.clone(),
                        source,
                    })?;
                    stdout_task.abort();
                    stderr_task.abort();
                    return Err(ProcessError::Cancelled { module });
                }
                process_status = child.wait(), if status.is_none() => {
                    status = Some(process_status.map_err(|source| ProcessError::Wait {
                        module: module.clone(),
                        source,
                    })?);
                }
                output_event = line_receiver.recv() => {
                    match output_event {
                        Some(OutputEvent::Line { stream, line }) => {
                            append_bounded(
                                match stream {
                                    OutputStream::Stdout => &mut stdout_buffer,
                                    OutputStream::Stderr => &mut stderr_buffer,
                                },
                                &line,
                                self.max_captured_output_bytes,
                                match stream {
                                    OutputStream::Stdout => &mut stdout_truncated,
                                    OutputStream::Stderr => &mut stderr_truncated,
                                },
                            );

                            if let Err(error) = self
                                .emit_line_events(
                                    &module,
                                    stream,
                                    &line,
                                    spec.progress_parser.as_ref(),
                                    &event_sender,
                                )
                                .await
                            {
                                let _ = child.kill().await;
                                stdout_task.abort();
                                stderr_task.abort();
                                return Err(error);
                            }
                        }
                        Some(OutputEvent::Closed(stream)) => match stream {
                            OutputStream::Stdout => stdout_closed = true,
                            OutputStream::Stderr => stderr_closed = true,
                        },
                        Some(OutputEvent::ReadError { stream, source }) => {
                            let _ = child.kill().await;
                            stdout_task.abort();
                            stderr_task.abort();
                            return Err(ProcessError::Read {
                                module,
                                stream,
                                source,
                            });
                        }
                        None => {
                            stdout_closed = true;
                            stderr_closed = true;
                        }
                    }
                }
            }
        }

        let _ = stdout_task.await;
        let _ = stderr_task.await;

        if cancellation.is_cancelled() {
            return Err(ProcessError::Cancelled { module });
        }

        let status = status.expect("child.wait() must produce a status before completion");
        let result = ProcessResult {
            status,
            stdout: stdout_buffer,
            stderr: stderr_buffer,
        };

        if !result.success() {
            return Err(ProcessError::Failed {
                module,
                status: result.status,
                stdout: result.stdout,
                stderr: result.stderr,
            });
        }

        Ok(result)
    }

    async fn emit_line_events(
        &self,
        module: &str,
        stream: OutputStream,
        line: &str,
        progress_parser: Option<&ProgressParser>,
        event_sender: &mpsc::Sender<EngineEvent>,
    ) -> Result<(), ProcessError> {
        if let Some(parser) = progress_parser {
            if let Some(progress) = parser.parse_line(line) {
                event_sender
                    .send(EngineEvent::Progress(progress))
                    .await
                    .map_err(|_| ProcessError::EventChannelClosed {
                        module: module.to_owned(),
                    })?;
            }
        }

        let level = match stream {
            OutputStream::Stdout => LogLevel::Debug,
            OutputStream::Stderr => LogLevel::Warn,
        };
        event_sender
            .send(EngineEvent::Log(LogEntry {
                level,
                message: line.to_owned(),
                module: Some(module.to_owned()),
            }))
            .await
            .map_err(|_| ProcessError::EventChannelClosed {
                module: module.to_owned(),
            })
    }
}

#[derive(Debug)]
enum OutputEvent {
    Line {
        stream: OutputStream,
        line: String,
    },
    Closed(OutputStream),
    ReadError {
        stream: OutputStream,
        source: io::Error,
    },
}

async fn forward_lines<R>(reader: R, stream: OutputStream, sender: mpsc::Sender<OutputEvent>)
where
    R: AsyncBufRead + Unpin,
{
    let mut lines = reader.lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if sender
                    .send(OutputEvent::Line { stream, line })
                    .await
                    .is_err()
                {
                    return;
                }
            }
            Ok(None) => break,
            Err(source) => {
                let _ = sender.send(OutputEvent::ReadError { stream, source }).await;
                break;
            }
        }
    }
    let _ = sender.send(OutputEvent::Closed(stream)).await;
}

fn append_bounded(buffer: &mut String, line: &str, limit: usize, truncated: &mut bool) {
    if *truncated || limit == 0 {
        *truncated = true;
        return;
    }

    let separator_len = usize::from(!buffer.is_empty());
    let available = limit.saturating_sub(buffer.len() + separator_len);
    if line.len() <= available {
        if separator_len != 0 {
            buffer.push('\n');
        }
        buffer.push_str(line);
        return;
    }

    if separator_len != 0 && available != 0 {
        buffer.push('\n');
    }
    let max_end = available.saturating_sub(separator_len);
    let end = line
        .char_indices()
        .take_while(|(index, character)| index + character.len_utf8() <= max_end)
        .map(|(index, character)| index + character.len_utf8())
        .last()
        .unwrap_or(0);
    buffer.push_str(&line[..end]);
    *truncated = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::progress::ProgressParser;
    use std::{path::PathBuf, time::Duration};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn streams_stdout_stderr_and_progress() {
        let parser = ProgressParser::percentage(PipelineState::Transcribing).unwrap();
        let spec = ProcessSpec::new("test-process", PathBuf::from("sh"))
            .state(PipelineState::Transcribing)
            .args(["-c", "printf 'progress: 25%%\\n'; printf 'warning\\n' >&2"])
            .with_progress_parser(parser);
        let (event_sender, mut event_receiver) = mpsc::channel(16);

        let result = ProcessRunner::default()
            .run(spec, event_sender, CancellationToken::new())
            .await
            .unwrap();

        assert!(result.success());
        assert!(result.stdout.contains("progress: 25%"));
        assert!(result.stderr.contains("warning"));

        let mut saw_progress = false;
        let mut saw_stderr = false;
        while let Ok(event) = event_receiver.try_recv() {
            match event {
                EngineEvent::Progress(update) => {
                    saw_progress = (update.fraction - 0.25).abs() < 0.001;
                }
                EngineEvent::Log(entry) if entry.message == "warning" => saw_stderr = true,
                _ => {}
            }
        }
        assert!(saw_progress);
        assert!(saw_stderr);
    }

    #[tokio::test]
    async fn cancellation_stops_a_running_process() {
        let spec =
            ProcessSpec::new("cancellable-process", PathBuf::from("sh")).args(["-c", "sleep 30"]);
        let (event_sender, _event_receiver) = mpsc::channel(4);
        let cancellation = CancellationToken::new();
        let worker_cancellation = cancellation.clone();

        let worker = tokio::spawn(async move {
            ProcessRunner::default()
                .run(spec, event_sender, worker_cancellation)
                .await
        });
        tokio::time::sleep(Duration::from_millis(30)).await;
        cancellation.cancel();

        let result = worker.await.unwrap();
        assert!(matches!(result, Err(ProcessError::Cancelled { .. })));
    }
}
