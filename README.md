# Dabovanie videí v2

Rust desktopová aplikácia pre automatizovaný dabing videa zo slovenčiny do čínštiny s lip-sync. Cieľové prostredie je WSL2 / Ubuntu 24.04 s AMD ROCm.

## Stav projektu

Implementovaná je **Fáza 1**:

- doménové typy v `src/types.rs`,
- striktne definovaný `PipelineState` a povolené prechody,
- TOML konfigurácia v `src/config.rs`,
- samostatné Python venv konfigurácie pre Whisper-SK, NLLB, Coqui XTTS-v2 a LatentSync,
- nastavenia modelov, skriptov, temp/output adresárov a ROCm environmentu,
- `thiserror` chyby pre konfiguráciu a aplikačná chybová hranica,
- kostra adresárov pre ďalšie fázy.

Tokio orchestration, Python subprocess adaptéry, GUI a deployment skripty budú doplnené postupne. PyO3 nie je a nebude použitý.

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
