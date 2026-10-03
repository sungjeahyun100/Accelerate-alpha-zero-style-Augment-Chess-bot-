"""Source-backed native adapter hot-path timings on immutable initial positions."""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
from threading import Barrier
import time

from .common import positive, report, workers

OPERATIONS = ("fork", "legal", "bind", "apply", "observe", "encode", "transition")


def _position(seed):
    from accelerate_chess.adapter_client import GameAdapterClient
    return GameAdapterClient.new_game({"gameStyle": "normal", "draftDelete": True}, seed)


def _operation(operation, seed):
    from accelerate_chess.architecture import Fixed8x8SpatialEncoder
    from accelerate_chess.ir import ObservationIR, TypedEncoder

    position = _position(seed)
    actor = position.decision_actor
    actions = position.legal_intents()
    if not actions:
        raise RuntimeError("source position has no legal public intents")
    first = actions[0]
    bound = position.bind_public_intent(first)
    encoder = Fixed8x8SpatialEncoder(TypedEncoder(position.spec))
    observation = position.observe(actor)

    def one():
        if operation == "fork":
            return position.fork()
        if operation == "legal":
            return position.legal_intents()
        if operation == "bind":
            return position.bind_public_intent(first)
        if operation == "apply":
            return position.apply(bound)
        if operation == "observe":
            return position.observe(actor)
        if operation == "encode":
            return encoder.encode(ObservationIR.from_public(observation, position.spec), actions)
        if operation == "transition":
            branch = position.fork()
            intents = branch.legal_intents()
            before = branch.observe(branch.decision_actor)
            encoder.encode(ObservationIR.from_public(before, branch.spec), intents)
            next_position = branch.apply(branch.bind_public_intent(intents[0])).position
            return next_position.observe(next_position.decision_actor)
        raise ValueError("unknown engine operation")

    return one


def _time_worker(one, iterations, barrier):
    barrier.wait()
    start = time.perf_counter_ns()
    for _ in range(iterations):
        one()
    return time.perf_counter_ns() - start


def run(args):
    from accelerate_chess._native import BUILD_PROFILE

    if BUILD_PROFILE != "release":
        raise RuntimeError("engine performance requires a release native wheel")
    if args.workers > (__import__("os").cpu_count() or 1):
        raise ValueError("workers exceed logical CPU count")
    results = {}
    for operation in OPERATIONS:
        prepared = [_operation(operation, args.seed + index) for index in range(args.workers)]
        barrier = Barrier(args.workers + 1)
        with ThreadPoolExecutor(max_workers=args.workers) as pool:
            futures = [pool.submit(_time_worker, one, args.iterations, barrier) for one in prepared]
            barrier.wait()
            started = time.perf_counter_ns()
            durations = [future.result() for future in futures]
            elapsed = (time.perf_counter_ns() - started) / 1e9
        count = args.iterations * args.workers
        results[operation] = {"status": "ok", "iterations": count,
                              "elapsed_seconds": elapsed, "worker_elapsed_seconds": sum(durations) / 1e9,
                              "ns_per_op": elapsed * 1e9 / count, "ops_per_second": count / elapsed}
    results["chance_transition"] = {"status": "unsupported", "reason": "no standalone public chance transition operation in current adapter"}
    return report("engine", vars(args), results, output_root=args.artifact_root, run_id=args.run_id)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--iterations", type=positive, default=10)
    parser.add_argument("--workers", type=workers, default=1)
    parser.add_argument("--seed", type=int, default=37)
    parser.add_argument("--artifact-root")
    parser.add_argument("--run-id")
    run(parser.parse_args())


if __name__ == "__main__":
    main()
