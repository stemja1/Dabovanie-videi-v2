# Architektúrny plán

## Fáza 1 — doména a konfigurácia

`src/types.rs`, `src/config.rs` a `src/error.rs` nepoznajú GUI ani konkrétny Python runtime. Zdieľajú iba serializovateľné dátové typy a validačné pravidlá.

## Fáza 2 — orchestration engine

`src/engine/` obsahuje tri oddelené časti:

- `process.rs` — generický `tokio::process::Command` runner s piped stdout/stderr, limitovaným capture bufferom, log eventmi a korektným kill pri zrušení,
- `progress.rs` — regex parser percent/fraction progress hlásení,
- `orchestrator.rs` — `PipelinePlanner`, sekvenčné spúšťanie procesných stageov, state transition validácia a mpsc event/command hranica.

GUI dostane `EngineHandle`; priamo nepozná `Child`, `Command` ani Python interpreter. `NoopPlanner` zatiaľ slúži ako testovací placeholder pre Fázu 3.

## Fáza 3 — konkrétne moduly a planner

`src/modules/` obsahuje tenké Rust adaptéry. Neobsahujú modelový runtime; iba validujú konfiguráciu a vytvárajú `ProcessSpec` s príslušným venv interpreterom:

- `whisper.rs` — `NaiveNeuron/whisper-large-v3-sk` → `utterance_metadata.json`,
- `translation.rs` — NLLB-200 `slk_Latn` → `zho_Hans`,
- `tts.rs` — XTTS-v2, sanitizácia a delenie dlhých textov,
- `latentsync.rs` — FFmpeg príprava + LatentSync wrapper,
- `ffmpeg.rs` — mapovanie lip-synced videa a nového audio tracku,
- `cleanup.rs` — canonical path kontrola a bezpečné odstránenie workspace,
- `planner.rs` — `ConfiguredPlanner` zostavujúci celý plán.

Python entrypointy v `scripts/` komunikujú iba cez súbory a stdout/stderr. Nikdy sa neimportujú cez PyO3. LatentSync wrapper predpokladá štandardné argumenty `inference.py` (`--video_path`, `--audio_path`, `--video_out_path`); prípadná odchýlka konkrétneho checkoutu sa upravuje iba v tomto wrapperi.

## Plánované vrstvy

- `src/gui/` — eframe/egui stav a event loop; s engine komunikuje iba cez `tokio::sync::mpsc`.
- deployment — izolované venv inštalácie, ROCm environment a WSL2 bash skripty.
