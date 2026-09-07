use anyhow::Result;
use dabovanie_videi_v2::config::AppConfig;
use std::{env, path::PathBuf};

fn main() -> Result<()> {
    let config_path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config.toml"));

    if config_path.exists() {
        let config = AppConfig::load(&config_path)?;
        println!(
            "Konfigurácia načítaná z {} (stav: {}).",
            config_path.display(),
            if config.rocm.enabled {
                "ROCm povolený"
            } else {
                "ROCm vypnutý"
            }
        );
    } else {
        AppConfig::default().save(&config_path)?;
        println!(
            "Vytvorená predvolená konfigurácia v {}.",
            config_path.display()
        );
    }

    Ok(())
}
