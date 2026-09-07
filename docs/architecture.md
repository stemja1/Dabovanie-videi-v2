# Architektúrny plán

## Fáza 1 — doména a konfigurácia

`src/types.rs`, `src/config.rs` a `src/error.rs` nepoznajú GUI ani konkrétny Python runtime. Zdieľajú iba serializovateľné dátové typy a validačné pravidlá.

## Plánované vrstvy

- `src/engine/` — Tokio runtime, `tokio::process::Command`, streamovanie stdout/stderr, regex progress parser a `CancellationToken`.
- `src/modules/` — Whisper-SK, NLLB, Coqui XTTS-v2, LatentSync a FFmpeg adaptéry; každý modul dostane svoju venv konfiguráciu.
- `src/gui/` — eframe/egui stav a event loop; s engine komunikuje iba cez `tokio::sync::mpsc`.
- `scripts/` — Python entrypointy, nikdy nie importované cez PyO3.
