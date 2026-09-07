#!/usr/bin/env python3
"""Coqui XTTS-v2 synthesis subprocess entrypoint.

Text is sanitized and split before entering the XTTS engine. Each segment is
placed on a simple timeline using the ASR timestamps so that the resulting WAV
can be consumed by LatentSync and the final FFmpeg render.
"""
from __future__ import annotations

import argparse
import json
import shutil
import sys
import tempfile
import unicodedata
import wave
from pathlib import Path
from typing import Any


def progress(value: int) -> None:
    print(f"progress: {max(0, min(100, value))}%", flush=True)


def sanitize_text(value: str) -> str:
    output: list[str] = []
    previous_space = False
    for character in value:
        if character.isspace() or unicodedata.category(character).startswith("C"):
            if not previous_space:
                output.append(" ")
                previous_space = True
        else:
            output.append(character)
            previous_space = False
    return "".join(output).strip()


def split_text(value: str, max_chars: int) -> list[str]:
    clean = sanitize_text(value)
    if not clean or max_chars <= 0:
        return []
    characters = list(clean)
    chunks: list[str] = []
    start = 0
    boundaries = set(".,!?;:。，！？；：、")
    while start < len(characters):
        end = min(start + max_chars, len(characters))
        if end < len(characters):
            for index in range(end - 1, start + max_chars // 2 - 1, -1):
                if characters[index] in boundaries:
                    end = index + 1
                    break
        chunk = "".join(characters[start:end]).strip()
        if chunk:
            chunks.append(chunk)
        start = end
    return chunks


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Coqui XTTS-v2 synthesis")
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--model", required=True)
    parser.add_argument("--language", default="zh")
    parser.add_argument("--max-chars", type=int, default=240)
    parser.add_argument("--speaker-wav", type=Path)
    parser.add_argument("--device", default="auto", choices=("auto", "cpu", "cuda"))
    return parser.parse_args()


def read_wav(path: Path) -> tuple[wave._wave_params, bytes]:
    with wave.open(str(path), "rb") as source:
        params = source.getparams()
        frames = source.readframes(source.getnframes())
    if params.sampwidth != 2 or params.nchannels != 1:
        raise ValueError(
            f"XTTS output must be mono 16-bit PCM, got {params.nchannels} channels / "
            f"{params.sampwidth * 8}-bit"
        )
    return params, frames


def silence(frame_count: int, frame_width: int) -> bytes:
    return b"\x00" * max(0, frame_count * frame_width)


def normalize_language(language: str) -> str:
    lowered = language.lower().replace("_", "-")
    if lowered in {"zh", "zh-cn", "zho-hans"}:
        return "zh-cn"
    return lowered


def main() -> int:
    args = parse_args()
    if not args.input.is_file():
        raise FileNotFoundError(f"TTS input does not exist: {args.input}")
    if args.max_chars <= 0:
        raise ValueError("--max-chars must be greater than zero")
    if args.speaker_wav is not None and not args.speaker_wav.is_file():
        raise FileNotFoundError(f"speaker reference does not exist: {args.speaker_wav}")

    try:
        from TTS.api import TTS
    except ImportError as error:
        raise RuntimeError("XTTS environment is missing the TTS package") from error

    with args.input.open("r", encoding="utf-8") as handle:
        payload = json.load(handle)
    utterances = payload.get("utterances", [])
    if not utterances:
        raise ValueError("translated metadata contains no utterances")

    use_gpu = args.device == "cuda"
    if args.device == "auto":
        try:
            import torch

            use_gpu = bool(torch.cuda.is_available())
        except ImportError:
            use_gpu = False

    print(f"loading XTTS model: {args.model}", flush=True)
    progress(1)
    tts = TTS(model_name=args.model, progress_bar=False, gpu=use_gpu)
    progress(10)

    output_chunks: list[tuple[float, Path]] = []
    temporary = Path(tempfile.mkdtemp(prefix="xtts-"))
    try:
        total = max(1, len(utterances))
        for index, utterance in enumerate(utterances):
            text = str(utterance.get("translated_text") or utterance.get("text") or "")
            chunks = split_text(text, args.max_chars)
            if not chunks:
                continue

            segment_dir = temporary / f"segment-{index:05d}"
            segment_dir.mkdir(parents=True, exist_ok=True)
            generated: list[Path] = []
            for chunk_index, chunk in enumerate(chunks):
                target = segment_dir / f"chunk-{chunk_index:03d}.wav"
                kwargs: dict[str, Any] = {
                    "text": chunk,
                    "file_path": str(target),
                    "language": normalize_language(args.language),
                }
                if args.speaker_wav is not None:
                    kwargs["speaker_wav"] = str(args.speaker_wav)
                tts.tts_to_file(**kwargs)
                generated.append(target)

            # Merge chunks belonging to one utterance into one temporary WAV.
            params, frames = read_wav(generated[0])
            merged = bytearray(frames)
            for generated_path in generated[1:]:
                next_params, next_frames = read_wav(generated_path)
                if next_params.framerate != params.framerate:
                    raise ValueError("XTTS chunks have inconsistent sample rates")
                merged.extend(next_frames)
            merged_path = segment_dir / "merged.wav"
            with wave.open(str(merged_path), "wb") as target:
                target.setnchannels(params.nchannels)
                target.setsampwidth(params.sampwidth)
                target.setframerate(params.framerate)
                target.writeframes(merged)

            start = max(0.0, float(utterance.get("start", 0.0)))
            output_chunks.append((start, merged_path))
            progress(10 + int(((index + 1) / total) * 85))

        if not output_chunks:
            raise ValueError("no non-empty translated text could be synthesized")

        first_params, _ = read_wav(output_chunks[0][1])
        frame_width = first_params.nchannels * first_params.sampwidth
        timeline = bytearray()
        cursor = 0
        for start, chunk_path in sorted(output_chunks, key=lambda item: item[0]):
            params, frames = read_wav(chunk_path)
            if params.framerate != first_params.framerate:
                raise ValueError("XTTS segments have inconsistent sample rates")
            target_frame = int(round(start * params.framerate))
            if target_frame > cursor:
                timeline.extend(silence(target_frame - cursor, frame_width))
                cursor = target_frame
            timeline.extend(frames)
            cursor += len(frames) // frame_width

        args.output.parent.mkdir(parents=True, exist_ok=True)
        with wave.open(str(args.output), "wb") as target:
            target.setnchannels(first_params.nchannels)
            target.setsampwidth(first_params.sampwidth)
            target.setframerate(first_params.framerate)
            target.writeframes(timeline)
    finally:
        shutil.rmtree(temporary, ignore_errors=True)

    progress(100)
    print(f"wrote synthesized audio to {args.output}", flush=True)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:  # noqa: BLE001 - subprocess boundary needs a readable error
        print(f"ERROR: {error}", file=sys.stderr, flush=True)
        raise
