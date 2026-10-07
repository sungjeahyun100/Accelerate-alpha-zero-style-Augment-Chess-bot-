"""One-process typed batching RSS probe; run each profile/size/rep separately."""
from __future__ import annotations

import argparse
from hashlib import sha256
import json
from pathlib import Path
import random
import time

import numpy as np

from accelerate_chess.encoding import canonical_json
from accelerate_chess.ir import ObservationIR, TypedEncoder, TypedEncoderSpec, batch_typed_positions
from .common import report


SIZES = (4, 16, 32, 64)
PROFILES = ("normal", "stress")


def _source():
    catalog_root = Path(__file__).resolve().parents[4] / "augment-chess" / "contracts" / "catalog"
    catalog = json.loads((catalog_root / "site-20260928.json").read_text(encoding="utf-8"))
    policy = json.loads((catalog_root / "observation-20260928.json").read_text(encoding="utf-8"))
    return catalog, policy


def _observation(spec: TypedEncoderSpec, profile: str):
    board = [[None for _ in range(8)] for _ in range(8)]
    count = 1 if profile == "normal" else 32
    for index in range(count):
        row, col = divmod(index, 8)
        board[row][col] = {"type": "pawn", "color": "white", "status": {"witchTrial": True},
                           "anchorRow": row, "anchorCol": col}
    observation = {"protocolVersion": "accelerate-observation-v2", "viewer": "white",
                   "turn": "white", "board": board, "ownCards": [], "opponentHandCount": 2,
                   "publicState": {"rulesVersion": spec.rules_version,
                                   "projectionVersion": "source-visible-20260928-v2",
                                   "observationPolicyHash": spec.observation_policy_hash,
                                   "deathmatchStatus": {"active": False, "warning": False},
                                   "actionsRemaining": 1, "moveCount": 1, "fullMove": 1,
                                   "collapsedCells": [], "boardMarks": [], "relationships": [],
                                   "overlays": [], "legalHints": {"moves": [], "cardTargets": []}},
                   "history": []}
    observation["informationStateKey"] = sha256(canonical_json(observation).encode()).hexdigest()
    return observation


def _actions(profile: str, rng: random.Random, index: int):
    count = (8 if profile == "normal" else 48) - index % (3 if profile == "normal" else 8)
    actions = []
    for index in range(count):
        row, col = divmod(index, 8)
        actions.append({"type": "move", "color": "white", "from": {"row": row, "col": col},
                        "destination": {"row": rng.randrange(8), "col": rng.randrange(8)}})
    return actions


def _rss_bytes():
    values = {}
    with open("/proc/self/status", encoding="ascii") as source:
        for line in source:
            if line.startswith(("VmRSS:", "VmHWM:")):
                key, amount, unit = line.split()
                if unit != "kB":
                    raise RuntimeError("unexpected Linux RSS unit")
                values[key[:-1]] = int(amount) * 1024
    return values["VmRSS"], values["VmHWM"]


def _batch(spec, profile, size, seed, non_retaining):
    rng = random.Random(seed)
    observation = _observation(spec, profile)
    ir = ObservationIR.from_public(observation, spec)
    encoder = TypedEncoder(spec)
    positions = [encoder.encode(ir, _actions(profile, rng, index)) for index in range(size)]
    input_bytes = sum(array.nbytes for position in positions for array in position.inputs.values())
    started = time.perf_counter()
    if non_retaining:
        batch = batch_typed_positions(positions, retain_positions=False)
    else:
        batch = batch_typed_positions(positions)
    batch_seconds = time.perf_counter() - started
    return batch, input_bytes, batch_seconds


def run(args):
    if args.profile not in PROFILES or args.batch_size not in SIZES or args.seed < 0 or args.repeat < 1:
        raise ValueError("invalid finite typed memory profile, size, seed or repeat")
    catalog, policy = _source()
    spec = TypedEncoderSpec.from_catalog(catalog, observation_policy=policy)
    batch, input_bytes, batch_seconds = _batch(spec, args.profile, args.batch_size, args.seed,
                                               args.non_retaining)
    batch_bytes = sum(array.nbytes for array in batch.inputs.values())
    # Explicit copies approximate an evaluator that owns separate host inputs.
    # Keep these alive through the RSS read; no model or backend is invoked.
    evaluator_inputs = tuple(np.array(array, copy=True) for array in batch.as_family_inputs("entity-transformer"))
    checksum = sum(int(array.size) for array in evaluator_inputs)
    rss, peak = _rss_bytes()
    result = {"status": "ok", "profile": args.profile, "batch_size": args.batch_size,
              "repeat": args.repeat, "input_position_bytes": input_bytes,
              "batch_numpy_bytes": batch_bytes, "peak_rss_bytes": peak,
              "steady_rss_bytes": rss, "batching_ms": batch_seconds * 1000,
              "batching_batches_per_second": 1 / batch_seconds, "evaluator_copy_elements": checksum,
              "record_count": int(batch.inputs["record_mask"][0].sum()),
              "relation_count": int(batch.inputs["relation_mask"][0].sum()),
              "candidate_count": int(batch.candidate_mask[0].sum()),
              "spec_digest": spec.digest, "synthetic": True}
    return report("typed-memory", {"profile": args.profile, "batch_size": args.batch_size,
                                   "seed": args.seed, "repeat": args.repeat,
                                   "non_retaining": args.non_retaining,
                                   "evaluator": "numpy-copy-proxy"}, result,
                  output_root=args.artifact_root, run_id=args.run_id)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=PROFILES, required=True)
    parser.add_argument("--batch-size", type=int, choices=SIZES, required=True)
    parser.add_argument("--seed", type=int, default=37)
    parser.add_argument("--repeat", type=int, default=1)
    parser.add_argument("--non-retaining", action="store_true")
    parser.add_argument("--artifact-root")
    parser.add_argument("--run-id")
    run(parser.parse_args())


if __name__ == "__main__":
    main()
