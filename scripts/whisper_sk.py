#!/usr/bin/env python3
"""Whisper-SK subprocess entrypoint.

The Rust engine starts this file in the dedicated Whisper virtualenv. The
script deliberately communicates through JSON files and line-oriented progress
messages; it is never imported into the Rust process.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


def progress(value: int) -> None:
    print(f"progress: {max(0, min(100, value))}%", flush=True)


def clean_text(value: Any) -> str:
    return " ".join(str(value or "").split()).strip()


def timestamp(value: Any, fallback: float) -> float:
    if value is None:
        return fallback
    try:
        return max(0.0, float(value))
    except (TypeError, ValueError):
        return fallback


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Whisper Slovak transcription")
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--model", required=True)
    parser.add_argument("--language", default="sk")
    parser.add_argument("--device", default="auto", choices=("auto", "cpu", "cuda"))
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if not args.input.is_file():
        raise FileNotFoundError(f"input video/audio does not exist: {args.input}")

    try:
        import torch
        from transformers import pipeline
    except ImportError as error:
        raise RuntimeError(
            "Whisper environment is missing torch/transformers dependencies"
        ) from error

    use_cuda = args.device == "cuda" or (
        args.device == "auto" and torch.cuda.is_available()
    )
    device = 0 if use_cuda else -1
    dtype = torch.float16 if use_cuda else torch.float32

    print(f"loading Whisper model: {args.model}", flush=True)
    progress(1)
    transcriber = pipeline(
        "automatic-speech-recognition",
        model=args.model,
        device=device,
        torch_dtype=dtype,
    )
    progress(10)

    result = transcriber(
        str(args.input),
        return_timestamps=True,
        chunk_length_s=30,
        stride_length_s=(5, 2),
        generate_kwargs={"language": args.language, "task": "transcribe"},
    )

    utterances: list[dict[str, Any]] = []
    cursor = 0.0
    chunks = result.get("chunks", []) if isinstance(result, dict) else []
    if chunks:
        for index, chunk in enumerate(chunks):
            text = clean_text(chunk.get("text"))
            if not text:
                continue
            raw_timestamp = chunk.get("timestamp") or (None, None)
            start = timestamp(raw_timestamp[0], cursor)
            end = timestamp(raw_timestamp[1], start)
            if end < start:
                end = start
            utterances.append(
                {"id": index, "start": start, "end": end, "text": text}
            )
            cursor = max(cursor, end)
    else:
        text = clean_text(result.get("text") if isinstance(result, dict) else result)
        if text:
            utterances.append({"id": 0, "start": 0.0, "end": 0.0, "text": text})

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", encoding="utf-8") as handle:
        json.dump({"utterances": utterances}, handle, ensure_ascii=False, indent=2)
        handle.write("\n")

    progress(100)
    print(f"wrote {len(utterances)} utterances to {args.output}", flush=True)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:  # noqa: BLE001 - subprocess boundary needs a readable error
        print(f"ERROR: {error}", file=sys.stderr, flush=True)
        raise
