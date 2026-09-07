use crate::error::ConfigError;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// Cesta k izolovanému Python virtual environmentu jedného modulu.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PythonVenv {
    pub path: PathBuf,
    /// Voliteľná explicitná cesta k interpreteru. Ak chýba, použije sa
    /// `<venv>/bin/python`, čo zodpovedá WSL2/Ubuntu.
    pub executable: Option<PathBuf>,
}

impl Default for PythonVenv {
    fn default() -> Self {
        Self {
            path: PathBuf::from("venv"),
            executable: None,
        }
    }
}

impl PythonVenv {
    pub fn python_path(&self) -> PathBuf {
        self.executable
            .clone()
            .unwrap_or_else(|| self.path.join("bin").join("python"))
    }

    fn validate(&self, field: &str) -> Result<(), ConfigError> {
        validate_path(field, &self.path)?;
        if let Some(executable) = &self.executable {
            validate_path(&format!("{field}.executable"), executable)?;
        }
        Ok(())
    }

    fn resolve_relative_to(&self, base: &Path) -> Self {
        Self {
            path: resolve_path(base, &self.path),
            executable: self
                .executable
                .as_ref()
                .map(|path| resolve_path(base, path)),
        }
    }
}

/// Vlastné virtualenv prostredie pre každý Python modul.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PythonEnvironments {
    pub whisper_sk: PythonVenv,
    pub translation: PythonVenv,
    pub coqui_xtts: PythonVenv,
    pub latentsync: PythonVenv,
}

impl Default for PythonEnvironments {
    fn default() -> Self {
        Self {
            whisper_sk: PythonVenv {
                path: PathBuf::from("venvs/whisper-sk"),
                ..PythonVenv::default()
            },
            translation: PythonVenv {
                path: PathBuf::from("venvs/translation"),
                ..PythonVenv::default()
            },
            coqui_xtts: PythonVenv {
                path: PathBuf::from("venvs/coqui-xtts"),
                ..PythonVenv::default()
            },
            latentsync: PythonVenv {
                path: PathBuf::from("venvs/latentsync"),
                ..PythonVenv::default()
            },
        }
    }
}

impl PythonEnvironments {
    fn validate(&self) -> Result<(), ConfigError> {
        self.whisper_sk.validate("python.whisper_sk")?;
        self.translation.validate("python.translation")?;
        self.coqui_xtts.validate("python.coqui_xtts")?;
        self.latentsync.validate("python.latentsync")?;
        Ok(())
    }

    fn resolve_relative_to(&self, base: &Path) -> Self {
        Self {
            whisper_sk: self.whisper_sk.resolve_relative_to(base),
            translation: self.translation.resolve_relative_to(base),
            coqui_xtts: self.coqui_xtts.resolve_relative_to(base),
            latentsync: self.latentsync.resolve_relative_to(base),
        }
    }
}

/// Modelové identifikátory alebo lokálne cesty.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelsConfig {
    pub whisper_sk: String,
    pub nllb_200: String,
    pub xtts_v2: String,
    pub latentsync: PathBuf,
}

impl Default for ModelsConfig {
    fn default() -> Self {
        Self {
            whisper_sk: "NaiveNeuron/whisper-large-v3-sk".to_owned(),
            nllb_200: "facebook/nllb-200-distilled-600M".to_owned(),
            xtts_v2: "tts_models/multilingual/multi-dataset/xtts_v2".to_owned(),
            latentsync: PathBuf::from("models/latentsync"),
        }
    }
}

impl ModelsConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        validate_string("models.whisper_sk", &self.whisper_sk)?;
        validate_string("models.nllb_200", &self.nllb_200)?;
        validate_string("models.xtts_v2", &self.xtts_v2)?;
        validate_path("models.latentsync", &self.latentsync)?;
        Ok(())
    }

    fn resolve_relative_to(&self, base: &Path) -> Self {
        Self {
            latentsync: resolve_path(base, &self.latentsync),
            ..self.clone()
        }
    }
}

/// Cesty k vstupom, výsledkom, cache a externým binárkam.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PathsConfig {
    pub working_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub output_dir: PathBuf,
    pub ffmpeg_binary: PathBuf,
    pub ffprobe_binary: PathBuf,
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            working_dir: PathBuf::from("work"),
            temp_dir: PathBuf::from("work/tmp"),
            output_dir: PathBuf::from("output"),
            ffmpeg_binary: PathBuf::from("ffmpeg"),
            ffprobe_binary: PathBuf::from("ffprobe"),
        }
    }
}

impl PathsConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        validate_path("paths.working_dir", &self.working_dir)?;
        validate_path("paths.temp_dir", &self.temp_dir)?;
        validate_path("paths.output_dir", &self.output_dir)?;
        validate_path("paths.ffmpeg_binary", &self.ffmpeg_binary)?;
        validate_path("paths.ffprobe_binary", &self.ffprobe_binary)?;
        Ok(())
    }

    fn resolve_relative_to(&self, base: &Path) -> Self {
        Self {
            working_dir: resolve_path(base, &self.working_dir),
            temp_dir: resolve_path(base, &self.temp_dir),
            output_dir: resolve_path(base, &self.output_dir),
            ffmpeg_binary: resolve_command(base, &self.ffmpeg_binary),
            ffprobe_binary: resolve_command(base, &self.ffprobe_binary),
        }
    }
}

/// Python entrypointy modulov. Každý entrypoint dostane vlastný interpreter
/// z `PythonEnvironments`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScriptsConfig {
    pub whisper: PathBuf,
    pub translation: PathBuf,
    pub tts: PathBuf,
    pub latentsync: PathBuf,
}

impl Default for ScriptsConfig {
    fn default() -> Self {
        Self {
            whisper: PathBuf::from("scripts/whisper_sk.py"),
            translation: PathBuf::from("scripts/translate_nllb.py"),
            tts: PathBuf::from("scripts/synthesize_xtts.py"),
            latentsync: PathBuf::from("scripts/latentsync.py"),
        }
    }
}

impl ScriptsConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        validate_path("scripts.whisper", &self.whisper)?;
        validate_path("scripts.translation", &self.translation)?;
        validate_path("scripts.tts", &self.tts)?;
        validate_path("scripts.latentsync", &self.latentsync)?;
        Ok(())
    }

    fn resolve_relative_to(&self, base: &Path) -> Self {
        Self {
            whisper: resolve_path(base, &self.whisper),
            translation: resolve_path(base, &self.translation),
            tts: resolve_path(base, &self.tts),
            latentsync: resolve_path(base, &self.latentsync),
        }
    }
}

/// Nastavenia AMD ROCm, ktoré engine prenesie do environmentu podprocesov.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RocmConfig {
    pub enabled: bool,
    pub rocm_home: Option<PathBuf>,
    pub hip_visible_devices: Option<String>,
    pub hsa_override_gfx_version: Option<String>,
    pub extra_environment: BTreeMap<String, String>,
}

impl Default for RocmConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            rocm_home: None,
            hip_visible_devices: None,
            hsa_override_gfx_version: None,
            extra_environment: BTreeMap::new(),
        }
    }
}

impl RocmConfig {
    /// Vráti environment premenné pre Python/FFmpeg podproces.
    pub fn environment(&self) -> BTreeMap<String, String> {
        let mut environment = self.extra_environment.clone();
        if !self.enabled {
            return environment;
        }

        if let Some(rocm_home) = &self.rocm_home {
            let value = rocm_home.to_string_lossy().into_owned();
            environment.insert("ROCM_HOME".to_owned(), value.clone());
            environment.entry("ROCM_PATH".to_owned()).or_insert(value);
        }
        if let Some(devices) = &self.hip_visible_devices {
            environment.insert("HIP_VISIBLE_DEVICES".to_owned(), devices.clone());
        }
        if let Some(gfx_version) = &self.hsa_override_gfx_version {
            environment.insert("HSA_OVERRIDE_GFX_VERSION".to_owned(), gfx_version.clone());
        }

        environment
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if let Some(rocm_home) = &self.rocm_home {
            validate_path("rocm.rocm_home", rocm_home)?;
        }
        if let Some(devices) = &self.hip_visible_devices {
            validate_string("rocm.hip_visible_devices", devices)?;
        }
        if let Some(gfx_version) = &self.hsa_override_gfx_version {
            validate_string("rocm.hsa_override_gfx_version", gfx_version)?;
        }
        for (key, value) in &self.extra_environment {
            validate_string("rocm.extra_environment key", key)?;
            validate_string(&format!("rocm.extra_environment.{key}"), value)?;
        }
        Ok(())
    }

    fn resolve_relative_to(&self, base: &Path) -> Self {
        Self {
            rocm_home: self.rocm_home.as_ref().map(|path| resolve_path(base, path)),
            ..self.clone()
        }
    }
}

/// Jazykové a pipeline parametre nezávislé od konkrétneho GUI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PipelineConfig {
    pub source_language: String,
    pub target_language: String,
    pub nllb_source_language: String,
    pub nllb_target_language: String,
    pub max_tts_chars: usize,
    pub cleanup_on_success: bool,
    pub cleanup_on_failure: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            source_language: "sk".to_owned(),
            target_language: "zh".to_owned(),
            nllb_source_language: "slk_Latn".to_owned(),
            nllb_target_language: "zho_Hans".to_owned(),
            max_tts_chars: 240,
            cleanup_on_success: true,
            cleanup_on_failure: false,
        }
    }
}

impl PipelineConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        validate_string("pipeline.source_language", &self.source_language)?;
        validate_string("pipeline.target_language", &self.target_language)?;
        validate_string("pipeline.nllb_source_language", &self.nllb_source_language)?;
        validate_string("pipeline.nllb_target_language", &self.nllb_target_language)?;
        if self.max_tts_chars == 0 {
            return Err(ConfigError::invalid(
                "pipeline.max_tts_chars",
                "musí byť väčšie ako nula",
            ));
        }
        Ok(())
    }
}

/// Koreň konfigurácie aplikácie.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub python: PythonEnvironments,
    pub models: ModelsConfig,
    pub paths: PathsConfig,
    pub scripts: ScriptsConfig,
    pub rocm: RocmConfig,
    pub pipeline: PipelineConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            python: PythonEnvironments::default(),
            models: ModelsConfig::default(),
            paths: PathsConfig::default(),
            scripts: ScriptsConfig::default(),
            rocm: RocmConfig::default(),
            pipeline: PipelineConfig::default(),
        }
    }
}

impl AppConfig {
    /// Načíta TOML a hneď overí doménové invariánty.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let content = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let config = toml::from_str::<Self>(&content).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        config.validate()?;
        Ok(config)
    }

    /// Načíta existujúci TOML alebo vytvorí predvolené nastavenia.
    pub fn load_or_create(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        if path.exists() {
            Self::load(path)
        } else {
            let config = Self::default();
            config.save(path)?;
            Ok(config)
        }
    }

    /// Serializuje konfiguráciu do čitateľného TOML.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ConfigError> {
        self.validate()?;
        let path = path.as_ref();

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|source| ConfigError::CreateParent {
                    path: path.to_path_buf(),
                    source,
                })?;
            }
        }

        let content =
            toml::to_string_pretty(self).map_err(|source| ConfigError::Serialize { source })?;
        fs::write(path, content).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Validuje hodnoty bez kontroly, či už existujú modely/venv/binárky.
    /// Inštalácia závislostí je samostatná zodpovednosť deployment fázy.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.python.validate()?;
        self.models.validate()?;
        self.paths.validate()?;
        self.scripts.validate()?;
        self.rocm.validate()?;
        self.pipeline.validate()?;
        Ok(())
    }

    /// Rozbalí relatívne cesty voči adresáru, v ktorom leží config TOML.
    ///
    /// `load` ponecháva cesty relatívne kvôli stabilnému zápisu konfigurácie.
    /// Orchestrátor by mal pred spustením jobu použiť túto metódu.
    pub fn resolve_relative_to(&self, base: impl AsRef<Path>) -> Self {
        let base = base.as_ref();
        Self {
            python: self.python.resolve_relative_to(base),
            models: self.models.resolve_relative_to(base),
            paths: self.paths.resolve_relative_to(base),
            scripts: self.scripts.resolve_relative_to(base),
            rocm: self.rocm.resolve_relative_to(base),
            ..self.clone()
        }
    }
}

fn resolve_path(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

/// Binárka môže byť buď lokálna cesta, alebo názov vyhľadávaný v `PATH`.
fn resolve_command(base: &Path, command: &Path) -> PathBuf {
    if command.is_absolute() || command.components().count() <= 1 {
        command.to_path_buf()
    } else {
        base.join(command)
    }
}

fn validate_path(field: &str, path: &Path) -> Result<(), ConfigError> {
    if path.as_os_str().is_empty() {
        return Err(ConfigError::invalid(field, "cesta nesmie byť prázdna"));
    }
    Ok(())
}

fn validate_string(field: &str, value: &str) -> Result<(), ConfigError> {
    if value.trim().is_empty() {
        return Err(ConfigError::invalid(field, "hodnota nesmie byť prázdna"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_configuration_is_valid() {
        let config = AppConfig::default();
        assert!(config.validate().is_ok());
        assert_eq!(
            config.python.whisper_sk.python_path(),
            PathBuf::from("venvs/whisper-sk/bin/python")
        );
    }

    #[test]
    fn rocm_environment_contains_only_configured_values() {
        let mut config = RocmConfig::default();
        config.rocm_home = Some(PathBuf::from("/opt/rocm"));
        config.hip_visible_devices = Some("0".to_owned());

        let environment = config.environment();
        assert_eq!(environment.get("ROCM_HOME"), Some(&"/opt/rocm".to_owned()));
        assert_eq!(environment.get("ROCM_PATH"), Some(&"/opt/rocm".to_owned()));
        assert_eq!(
            environment.get("HIP_VISIBLE_DEVICES"),
            Some(&"0".to_owned())
        );
    }
}
