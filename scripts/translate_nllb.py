#!/usr/bin/env python3
"""NLLB-200 translation subprocess entrypoint."""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


def progress(value: int) -> None:
    print(f"progress: {max(0, min(100, value))}%", flush=True)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="NLLB Slovak to Chinese translation")
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--model", required=True)
    parser.add_argument("--source-lang", required=True)
    parser.add_argument("--target-lang", required=True)
    parser.add_argument("--device", default="auto", choices=("auto", "cpu", "cuda"))
    parser.add_argument("--max-new-tokens", type=int, default=256)
    return parser.parse_args()


def load_metadata(path: Path) -> list[dict[str, Any]]:
    with path.open("r", encoding="utf-8") as handle:
        payload = json.load(handle)
    utterances = payload.get("utterances", [])
    if not isinstance(utterances, list):
        raise ValueError("metadata.utterances must be an array")
    return utterances


def main() -> int:
    args = parse_args()
    if not args.input.is_file():
        raise FileNotFoundError(f"translation input does not exist: {args.input}")

    utterances = load_metadata(args.input)
    try:
        import torch
        from transformers import AutoModelForSeq2SeqLM, AutoTokenizer
    except ImportError as error:
        raise RuntimeError(
            "translation environment is missing torch/transformers dependencies"
        ) from error

    use_cuda = args.device == "cuda" or (
        args.device == "auto" and torch.cuda.is_available()
    )
    device = torch.device("cuda" if use_cuda else "cpu")

    print(f"loading NLLB model: {args.model}", flush=True)
    progress(1)
    tokenizer = AutoTokenizer.from_pretrained(args.model, src_lang=args.source_lang)
    model = AutoModelForSeq2SeqLM.from_pretrained(args.model).to(device)
    model.eval()
    target_id = tokenizer.convert_tokens_to_ids(args.target_lang)
    if target_id is None or target_id == tokenizer.unk_token_id:
        raise ValueError(f"NLLB target language is not available: {args.target_lang}")
    progress(10)

    translated: list[dict[str, Any]] = []
    total = max(1, len(utterances))
    with torch.inference_mode():
        for index, source in enumerate(utterances):
            text = " ".join(str(source.get("text", "")).split()).strip()
            if text:
                encoded = tokenizer(
                    text,
                    return_tensors="pt",
                    truncation=True,
                    max_length=512,
                ).to(device)
                generated = model.generate(
                    **encoded,
                    forced_bos_token_id=target_id,
                    max_new_tokens=args.max_new_tokens,
                )
                target_text = tokenizer.batch_decode(
                    generated, skip_special_tokens=True
                )[0].strip()
            else:
                target_text = ""

            item = {
                "id": int(source.get("id", index)),
                "start": float(source.get("start", 0.0)),
                "end": float(source.get("end", 0.0)),
                "text": text,
                "translated_text": target_text,
            }
            translated.append(item)
            progress(10 + int(((index + 1) / total) * 85))

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", encoding="utf-8") as handle:
        json.dump({"utterances": translated}, handle, ensure_ascii=False, indent=2)
        handle.write("\n")

    progress(100)
    print(f"wrote {len(translated)} translated utterances to {args.output}", flush=True)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:  # noqa: BLE001 - subprocess boundary needs a readable error
        print(f"ERROR: {error}", file=sys.stderr, flush=True)
        raise
