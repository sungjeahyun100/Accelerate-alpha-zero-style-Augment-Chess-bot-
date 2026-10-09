"""Opt-in legal action profile from a verified synthetic position provenance.

The report contains counts and durations only. No source action, card identity,
hidden frame, or RNG value is written to the report.
"""
from __future__ import annotations

import argparse
from pathlib import Path
from time import perf_counter

from ._native import RULES_VERSION_V7, site_catalog, site_observation_policy
from .ir import TypedEncoderSpec
from .replay import atomic_json, read_json
from .staged import replay_position


def _run(provenance: dict, spec: TypedEncoderSpec, expected: str,
         mode: str, page_size: int) -> dict:
    replay_started = perf_counter()
    position, _ = replay_position(provenance, spec)
    replay_ms = (perf_counter() - replay_started) * 1000.0
    if position.snapshot_revision != expected:
        raise ValueError("replayed Source Position ID differs from --expected-position-id")
    actor = position.decision_actor
    public = position.observe(actor)
    public_piece_count = sum(piece is not None for row in public["board"] for piece in row)
    state_stats = position._session.begin_legal_profile()
    wall_started = perf_counter()
    examined = 0
    stream_open_ms = None
    try:
        if mode == "eager":
            legal_count = len(position.legal_intents())
        else:
            opened = perf_counter()
            stream = position.action_stream()
            stream_open_ms = (perf_counter() - opened) * 1000.0
            legal_count = 0
            exhausted = False
            while not exhausted:
                page = stream.next_page(page_size)
                legal_count += len(page["actions"])
                examined += page["examined"]
                exhausted = page["exhausted"]
                if examined > 100_000 or legal_count > 4096:
                    raise ValueError("diagnostic exceeded 100000 candidates or 4096 legal intents")
    finally:
        wall_ms = (perf_counter() - wall_started) * 1000.0
        profile = position._session.finish_legal_profile()
    if mode == "eager":
        examined = profile["counts"].get("candidates_examined", 0)
    timing = profile["timing_ms"]
    return {
        "position_id": expected,
        "actor": actor,
        "mode": mode,
        "page_size": page_size if mode == "stream" else None,
        "replay_ms": replay_ms,
        "stream_open_ms": stream_open_ms,
        "wall_legal_ms": wall_ms,
        "examined": examined,
        "legal_count": legal_count,
        "exhausted": True,
        "public_board_piece_count": public_piece_count,
        "state_statistics": state_stats,
        "timing_ms": {name: item["total_ms"] for name, item in timing.items()},
        "timing_detail": timing,
        "unclassified_ms": profile["unclassified_ms"],
        "counts": profile["counts"],
        "slowest_candidates": profile["slowest_candidates"],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Profile v7 legal actions from verified provenance")
    parser.add_argument("--provenance", type=Path, required=True)
    parser.add_argument("--expected-position-id", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--mode", choices=("both", "eager", "stream"), default="both")
    parser.add_argument("--page-size", type=int, default=64)
    args = parser.parse_args()
    if (len(args.expected_position_id) != 64
            or any(char not in "0123456789abcdef" for char in args.expected_position_id)):
        parser.error("--expected-position-id must be a lowercase SHA-256 digest")
    if not 1 <= args.page_size <= 4096:
        parser.error("--page-size must be within 1..4096")
    provenance = read_json(args.provenance)
    if not isinstance(provenance, dict) or provenance.get("generated_position_id") != args.expected_position_id:
        raise ValueError("provenance generated_position_id differs from --expected-position-id")
    spec = TypedEncoderSpec.from_catalog(site_catalog(RULES_VERSION_V7),
                                        observation_policy=site_observation_policy(RULES_VERSION_V7))
    modes = ("eager", "stream") if args.mode == "both" else (args.mode,)
    report = {"format": "accelerate-legal-diagnostic-v1",
              "position_id": args.expected_position_id,
              "runs": {mode: _run(provenance, spec, args.expected_position_id,
                                  mode, args.page_size) for mode in modes}}
    atomic_json(args.output, report)
    print(args.output)


if __name__ == "__main__":
    main()
