#!/usr/bin/env python3
"""LatentSync subprocess wrapper.

The wrapper prepares normalized video/audio inputs with FFmpeg, launches the
LatentSync repository's inference.py in its own virtualenv, forwards output,
and removes its preparation workspace on both success and failure.
"""
from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def progress(value: int) -> None:
    print(f"progress: {max(0, min(100, value))}%", flush=True)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="LatentSync video lip-sync")
    parser.add_argument("--video", required=True, type=Path)
    parser.add_argument("--audio", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--latentsync-root", required=True, type=Path)
    parser.add_argument("--ffmpeg", default="ffmpeg")
    parser.add_argument("--device", default="auto", choices=("auto", "cpu", "cuda"))
    parser.add_argument("--inference-steps", type=int, default=20)
    parser.add_argument("--guidance-scale", type=float, default=1.5)
    return parser.parse_args()


def run_command(command: list[str], cwd: Path | None = None) -> None:
    print("running: " + " ".join(command), flush=True)
    environment = os.environ.copy()
    environment["PYTHONUNBUFFERED"] = "1"
    process = subprocess.Popen(
        command,
        cwd=str(cwd) if cwd else None,
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )
    assert process.stdout is not None
    for line in process.stdout:
        print(line.rstrip(), flush=True)
    return_code = process.wait()
    if return_code != 0:
        raise RuntimeError(f"command failed with exit code {return_code}")


def find_inference_script(root: Path) -> Path:
    candidates = (root / "inference.py", root / "scripts" / "inference.py")
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    raise FileNotFoundError(f"LatentSync inference.py not found below {root}")


def find_checkpoint(root: Path) -> Path | None:
    for candidate in (
        root / "checkpoints" / "latentsync_unet.pt",
        root / "checkpoints" / "latentsync.pt",
        root / "checkpoints" / "latentsync.ckpt",
    ):
        if candidate.is_file():
            return candidate
    return None


def main() -> int:
    args = parse_args()
    for input_path in (args.video, args.audio):
        if not input_path.is_file():
            raise FileNotFoundError(f"LatentSync input does not exist: {input_path}")
    if not args.latentsync_root.is_dir():
        raise NotADirectoryError(f"LatentSync root does not exist: {args.latentsync_root}")

    inference = find_inference_script(args.latentsync_root)
    checkpoint = find_checkpoint(args.latentsync_root)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    workspace = Path(tempfile.mkdtemp(prefix="latentsync-"))
    prepared_video = workspace / "video.mp4"
    prepared_audio = workspace / "audio.wav"

    try:
        progress(5)
        run_command(
            [
                args.ffmpeg,
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-i",
                str(args.video),
                "-an",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                str(prepared_video),
            ]
        )
        progress(20)
        run_command(
            [
                args.ffmpeg,
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-i",
                str(args.audio),
                "-ar",
                "16000",
                "-ac",
                "1",
                "-c:a",
                "pcm_s16le",
                str(prepared_audio),
            ]
        )
        progress(35)

        inference_command = [
            sys.executable,
            str(inference),
            "--video_path",
            str(prepared_video),
            "--audio_path",
            str(prepared_audio),
            "--video_out_path",
            str(args.output),
            "--inference_steps",
            str(args.inference_steps),
            "--guidance_scale",
            str(args.guidance_scale),
        ]
        if checkpoint is not None:
            inference_command.extend(["--inference_ckpt_path", str(checkpoint)])
        run_command(inference_command, cwd=args.latentsync_root)
        progress(100)

        if not args.output.is_file():
            raise FileNotFoundError(
                f"LatentSync inference finished without output: {args.output}"
            )
        print(f"wrote lip-synced video to {args.output}", flush=True)
        return 0
    finally:
        shutil.rmtree(workspace, ignore_errors=True)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:  # noqa: BLE001 - subprocess boundary needs a readable error
        print(f"ERROR: {error}", file=sys.stderr, flush=True)
        raise
