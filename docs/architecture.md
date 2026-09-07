# Architektúrny plán

## Fáza 1 — doména a konfigurácia

`src/types.rs`, `src/config.rs` a `src/error.rs` nepoznajú GUI ani konkrétny Python runtime. Zdieľajú iba serializovateľné dátové typy a validačné pravidlá.

## Fáza 2 — orchestration engine

`src/engine/` obsahuje tri oddelené časti:

- `process.rs` — generický `tokio::process::Command` runner s piped stdout/stderr, limitovaným capture bufferom, log eventmi a korektným kill pri zrušení,
- `progress.rs` — regex parser percent/fraction progress hlásení,
- `orchestrator.rs` — `PipelinePlanner`, sekvenčné spúšťanie procesných stageov, state transition validácia a mpsc event/command hranica.

GUI dostane `EngineHandle`; priamo nepozná `Child`, `Command` ani Python interpreter. `NoopPlanner` zatiaľ slúži ako testovací placeholder pre Fázu 3.

## Plánované vrstvy

- `src/modules/` — Whisper-SK, NLLB, Coqui XTTS-v2, LatentSync a FFmpeg adaptéry; každý modul dostane svoju venv konfiguráciu.
- `src/gui/` — eframe/egui stav a event loop; s engine komunikuje iba cez `tokio::sync::mpsc`.
- `scripts/` — Python entrypointy, nikdy nie importované cez PyO3.
