# Python subprocess contracts

These scripts are entrypoints for the Rust `ProcessRunner`; they are not imported through PyO3.

Each script:

- accepts explicit input/output paths,
- writes structured files or media to the requested output,
- emits lines such as `progress: 42%` for the Rust regex parser,
- exits with a non-zero code and a readable stderr message on failure.

Install dependencies separately into the configured venvs. The Rust planner never shares a Python interpreter between Whisper, NLLB, XTTS and LatentSync.

The LatentSync wrapper assumes the repository exposes `inference.py` with the commonly used `--video_path`, `--audio_path`, `--video_out_path` and optional `--inference_ckpt_path` arguments. If a checkout uses different names, adapt only `latentsync.py`; the Rust layer remains unchanged.
