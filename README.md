# Dabovanie videí v2

Rust desktopová aplikácia pre automatizovaný dabing videa zo slovenčiny do čínštiny s lip-sync. Cieľové prostredie je WSL2 / Ubuntu 24.04 s AMD ROCm.

## Stav projektu

Implementované sú **Fázy 1 až 3**:

- doménové typy v `src/types.rs`,
- striktne definovaný `PipelineState` a povolené prechody,
- TOML konfigurácia v `src/config.rs`,
- samostatné Python venv konfigurácie pre Whisper-SK, NLLB, Coqui XTTS-v2 a LatentSync,
- nastavenia modelov, skriptov, temp/output adresárov a ROCm environmentu,
- `thiserror` chyby pre konfiguráciu a aplikačná chybová hranica,
- Tokio orchestration engine v `src/engine/`,
- `tokio::process::Command` runner so súčasným čítaním stdout/stderr,
- regex progress parser a streamovanie logov/progressu cez `tokio::sync::mpsc`,
- `CancellationToken` pre zrušenie aktívneho subprocessu,
- konkrétne Rust adaptéry v `src/modules/` pre Whisper-SK, NLLB-200, Coqui XTTS-v2, LatentSync a FFmpeg,
- `ConfiguredPlanner`, ktorý vytvára izolovaný workspace a plán piatich pipeline stageov,
- bezpečný cleanup workspace s ochranou pred odstránením mimo temp rootu,
- samostatné Python entrypointy v `scripts/` s JSON/media kontraktmi a progress výstupom.

Python entrypointy sa spúšťajú iba cez dedikované interpretery nastavené v `config.toml`; PyO3 nie je a nebude použitý. Fáza 4 doplní eframe/egui GUI a Fáza 5 deployment skripty pre WSL2 + ROCm.

## Lokálne spustenie fázy 1

Po nainštalovaní Rust toolchainu:

```bash
cargo fmt --all
cargo check
cargo run -- config.toml
```

Ak `config.toml` neexistuje, aplikácia vytvorí predvolený súbor. Relatívne cesty sa pri načítaní zachovávajú; runtime ich má pred spustením jobu rozlíšiť cez `AppConfig::resolve_relative_to(...)`.

## Bezpečnostná poznámka

Prístupové tokeny do GitHubu nepatria do repozitára, `config.toml`, shell skriptov ani commitov. Pre push používajte credential helper alebo krátkodobý token uložený mimo pracovného stromu.
