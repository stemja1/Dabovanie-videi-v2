use crate::{
    config::{AppConfig, PythonVenv},
    engine::{ProcessSpec, ProgressParser},
    types::PipelineState,
};
use std::path::Path;

pub(crate) fn python_spec(
    module: &str,
    state: PipelineState,
    venv: &PythonVenv,
    script: &Path,
    config: &AppConfig,
) -> ProcessSpec {
    let mut spec = ProcessSpec::new(module, venv.python_path())
        .state(state)
        .arg("-u")
        .arg(script)
        .env("PYTHONUNBUFFERED", "1");

    for (key, value) in config.rocm.environment() {
        spec = spec.env(key, value);
    }
    spec
}

pub(crate) fn require_path(path: &Path, field: &str) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err(format!("{field} cesta nesmie byť prázdna"));
    }
    Ok(())
}

pub(crate) fn require_string(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} hodnota nesmie byť prázdna"));
    }
    Ok(())
}

pub(crate) fn percentage_parser(
    state: PipelineState,
) -> Result<ProgressParser, crate::engine::ProgressParserError> {
    ProgressParser::percentage(state)
}
