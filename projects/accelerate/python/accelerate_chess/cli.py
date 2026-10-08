"""Finite developer commands; export/evaluation never activate a model.

The activation reference records its validation backend as provenance. Each
execution command selects its backend explicitly (ort by default); the stored
reference never silently switches that selection to tract.
"""
from __future__ import annotations

import argparse
from dataclasses import asdict
import json
import math
import os
from pathlib import Path
import signal
import sys
import time

import numpy as np
import torch

from .encoding import EncoderSpec, PublicEncoder, canonical_json
from .ir import V7_RULES_VERSION
from .inference import ProductionEvaluator
from .network.artifacts import (export_onnx, export_typed_onnx, file_sha256, load_adapter, load_base,
                                load_manifest, load_typed_adapter, load_typed_base, save_adapter, save_base,
                                save_typed_adapter, save_typed_base)
from .network.model import ModelConfig, PolicyValueNetwork
from .replay import EpisodeRecorder, ReplayEpisode, artifact_root, atomic_json, read_json, reserve_slot, slot, writer_claim
from .search import (BeliefLimits, InformationSetSearch, NativeSourceFactory, ParticleBelief,
                     PublicTracker, SearchBudgetError, SearchLimits, TypedInformationSetSearch)
from .training import DatasetCursor, ReplayDataset, TrainingLimits, create_optimizer, load_training_checkpoint, optimize, save_training_checkpoint


class _ExplicitBlocks(argparse.Action):
    def __call__(self, parser, namespace, values, option_string=None):
        namespace.blocks = values
        namespace.blocks_explicit = True


def default_spec(catalog_path: str | None = None, *, observation_policy: dict | None = None,
                 model_family: str = "legacy-resnet"):
    typed = model_family in ("mask-resnet", "entity-transformer")
    if model_family != "legacy-resnet" and not typed:
        raise ValueError("unknown explicit model family")
    if typed:
        from .ir import TypedEncoderSpec

    if catalog_path:
        catalog = read_json(catalog_path)
    else:
        from ._native import site_catalog
        catalog = site_catalog(V7_RULES_VERSION) if typed else site_catalog()
    if observation_policy is None:
        from ._native import site_observation_policy

        observation_policy = (site_observation_policy(V7_RULES_VERSION) if typed
                              else site_observation_policy())
    if model_family == "legacy-resnet":
        return EncoderSpec.from_catalog(catalog, observation_policy=observation_policy,
            history_encoding="public-history-summary-v1", action_encoding="public-decision-intent-v1")
    if catalog.get("rulesVersion") != V7_RULES_VERSION:
        raise ValueError("typed model families require the pinned v7 source catalog")
    return TypedEncoderSpec.from_catalog(catalog, observation_policy=observation_policy)


def _configuration(args):
    config = read_json(args.config) if args.config else {"gameStyle": "normal"}
    if not isinstance(config, dict):
        raise ValueError("configuration must be a public GameConfig object")
    return config


def _manifest(args, root, spec):
    if args.manifest:
        return Path(args.manifest).resolve()
    activation = read_json(slot(root, "models", "active") / "activation.json")
    fields = {"version", "manifest", "manifest_sha256", "model_sha256", "encoder_hash", "backend"}
    if args.model_family == "legacy-resnet":
        if set(activation) != fields or activation["version"] != "accelerate-activation-v1":
            raise ValueError("active model reference is incompatible")
    elif (set(activation) != fields | {"architecture_family", "model_io_version"}
          or activation["version"] != "accelerate-activation-v2"
          or activation["architecture_family"] != args.model_family
          or activation["model_io_version"] != "typed-policy-value-v1"):
        raise ValueError("active typed model reference is incompatible")
    if activation["encoder_hash"] != spec.digest or activation["backend"] not in ("ort", "tract"):
        raise ValueError("active model encoder or verification backend is incompatible")
    path = Path(activation["manifest"])
    manifest = load_manifest(path, spec)
    if (file_sha256(path) != activation["manifest_sha256"]
            or manifest["model_sha256"] != activation["model_sha256"]
            or args.model_family != "legacy-resnet" and manifest["architecture_family"] != args.model_family):
        raise ValueError("activated artifact changed since explicit activation")
    return path


def _search(args, spec, manifest, backend):
    limits = SearchLimits(iterations=args.iterations, max_depth=args.depth, elapsed_ms=args.search_ms,
                          max_nodes=args.nodes, max_edges=args.edges, max_candidates=args.candidates,
                          leaf_batch_size=args.leaf_batch_size)
    evaluator = ProductionEvaluator(manifest, spec, backend, threads=args.threads)
    if args.model_family == "legacy-resnet":
        return InformationSetSearch(PublicEncoder(spec), evaluator, limits=limits)
    from .ir import TypedEncoder

    if evaluator.architecture_family != args.model_family:
        raise ValueError("typed manifest architecture differs from the explicit CLI family")
    return TypedInformationSetSearch(TypedEncoder(spec), evaluator, limits=limits)


def _typed_model(args, spec):
    from .network import (EntityTransformer, EntityTransformerConfig, MaskResNetConfig,
                          MaskResNetPolicyValueNetwork, TypedContextConfig)

    vocabulary = len(spec.category_vocabulary)
    context = TypedContextConfig((vocabulary,) * 4, (vocabulary,) * 2,
                                 (vocabulary,) * 4, hidden_dim=args.channels)
    blocks = args.blocks if args.blocks_explicit or args.model_family == "mask-resnet" else 4
    if args.model_family == "mask-resnet":
        config = MaskResNetConfig(len(spec.feature_schema["spatial_channels"]), context,
            channels=args.channels, residual_blocks=blocks, lora_rank=args.rank,
            lora_alpha=float(args.rank), max_board_axis=spec.max_board_axis,
            max_candidates=spec.max_candidates, max_batch=spec.max_batch)
        return MaskResNetPolicyValueNetwork(config), blocks
    if args.model_family == "entity-transformer":
        config = EntityTransformerConfig(context, blocks=blocks, lora_rank=args.rank,
                                         lora_alpha=float(args.rank))
        return EntityTransformer(config), blocks
    raise ValueError("typed model initialization needs an explicit A or B family")


def _typed_loaded_family(model):
    from .network import EntityTransformer, MaskResNetPolicyValueNetwork

    if isinstance(model, MaskResNetPolicyValueNetwork):
        return "mask-resnet"
    if isinstance(model, EntityTransformer):
        return "entity-transformer"
    raise ValueError("typed base checkpoint did not reconstruct a supported model family")


def _typed_sample_inputs(replay_path, spec, family, *, sample_public=None):
    from .ir import ObservationIR, TypedEncoder, batch_typed_positions

    if bool(replay_path) == bool(sample_public):
        raise ValueError("typed export or activation needs exactly one public sample or replay")
    if replay_path:
        episode = ReplayEpisode.load(replay_path, spec)
        if episode.architecture_family != family or not episode.decisions:
            raise ValueError("typed sample replay family or public decision is missing")
        record = episode.decisions[0]
        observation = episode.trackers[record["actor"]].frame_at(record["trace_step"])
        intents = [candidate["intent"] for candidate in record["candidates"]]
        belief_summary = record["belief_summary"]
    else:
        sample = read_json(sample_public)
        if (not isinstance(sample, dict) or set(sample) != {"version", "architecture_family", "encoder_hash",
                "observation", "intents", "belief_summary"}
                or sample["version"] != "typed-inference-sample-v1"
                or sample["architecture_family"] != family or sample["encoder_hash"] != spec.digest
                or not isinstance(sample["intents"], list) or not sample["intents"]):
            raise ValueError("typed public sample source/model contract is invalid")
        observation, intents, belief_summary = (sample["observation"], sample["intents"],
                                                 sample["belief_summary"])
    ir = ObservationIR.from_public(observation, spec, belief_summary=belief_summary)
    batch = batch_typed_positions([TypedEncoder(spec).encode(ir, intents)])
    if not np.array_equal(batch.candidate_mask[0],
                          np.arange(batch.candidate_mask.shape[1]) < len(intents)):
        raise ValueError("typed sample replay candidate mask differs from public intents")
    order = spec.feature_schema["input_order"][family]
    return dict(zip(order, batch.as_family_inputs(family), strict=True))


def initialize(args, root, spec):
    if args.model_family == "entity-transformer" and args.warm_start_legacy_base:
        raise ValueError("legacy residual warm start is available only for mask-resnet")
    if args.model_family == "legacy-resnet" and args.warm_start_legacy_base:
        raise ValueError("legacy residual warm start requires the new mask-resnet family")
    if (root / "models" / args.slot).is_symlink():
        raise ValueError("model slot is a symlink; choose a new --slot")
    directory = slot(root, "models", args.slot)
    base = directory / "base.pt"
    if (base.exists() or base.is_symlink()) and not args.overwrite:
        raise FileExistsError("model slot already contains a base; explicit --overwrite is required")
    torch.manual_seed(args.seed)
    if args.model_family != "legacy-resnet":
        model, blocks = _typed_model(args, spec)
        warmed = None
        if args.warm_start_legacy_base:
            legacy, _ = load_base(args.warm_start_legacy_base)
            transferred = model.warm_start_residual_convolutions(legacy)
            warmed = {"legacy_base_sha256": file_sha256(args.warm_start_legacy_base),
                      "transferred_tensors": list(transferred)}
        model.eval()
        fingerprint = save_typed_base(model, spec, base)
        return {"base": str(base), "base_hash": fingerprint, "manifest": None,
                "encoder_hash": spec.digest, "architecture_family": args.model_family,
                "architecture": {"channels": args.channels, "blocks": blocks, "rank": args.rank},
                "warm_start": warmed, "activated": False}
    blocks = args.blocks
    model = PolicyValueNetwork(ModelConfig(spec.board_channels, spec.condition_dim, spec.action_dim,
                    channels=args.channels, residual_blocks=blocks, lora_rank=args.rank, lora_alpha=float(args.rank))).eval()
    fingerprint = save_base(model, spec, base)
    manifest = export_onnx(model, spec, directory / "deployment")
    return {"base": str(base), "base_hash": fingerprint, "manifest": str(manifest), "encoder_hash": spec.digest,
            "architecture": {"channels": args.channels, "blocks": blocks, "rank": args.rank}, "activated": False}


def choose(args, root, spec, cancelled):
    typed_spec = spec if args.model_family != "legacy-resnet" else None
    tracker = PublicTracker.from_snapshot(read_json(args.trace), typed_spec=typed_spec)
    factory = NativeSourceFactory(_configuration(args), typed_spec=typed_spec)
    belief = ParticleBelief(tracker, factory, seed=args.belief_seed,
              limits=BeliefLimits(particles=args.particles, proposals=args.proposals, elapsed_ms=args.belief_ms), cancelled=cancelled)
    search = _search(args, spec, _manifest(args, root, spec), args.backend)
    return asdict(search.run(belief, cancelled=cancelled))


def _record_selfplay_failure(root, run_id, path, recorder, error, *, save_attempted, replay_saved):
    failure = {"status": "execution-failed" if recorder is not None else "initialization-failed",
               "episode": str(path) if recorder is not None else None,
               "replay_saved": replay_saved, "error": type(error).__name__, "reason": str(error)}
    if recorder is not None and not save_attempted:
        try:
            recorder.finish(None, f"{type(error).__name__}: {error}")
            recorder.save(path)
            failure["replay_saved"] = True
        except (Exception, KeyboardInterrupt) as salvage_error:
            failure["replay_save_error"] = f"{type(salvage_error).__name__}: {salvage_error}"
    try:
        atomic_json(slot(root, "reports", run_id) / "failure.json", failure)
    except (Exception, KeyboardInterrupt) as report_error:
        error.add_note(f"selfplay failure report could not be saved: {type(report_error).__name__}: {report_error}")


def _cancelled_search_budget(error, cancelled):
    # A signal alone must not relabel an unrelated source/runtime failure.
    return (isinstance(error, SearchBudgetError)
            and str(error) in ("belief reconstruction cancelled",
                               "no public decision was evaluated before cancelled")
            and cancelled())


def selfplay(args, root, spec, cancelled):
    if not 1 <= args.games <= 64 or not 1 <= args.max_plies <= 4096 or not 1 <= args.elapsed_ms <= 86_400_000:
        raise ValueError("selfplay needs finite games, plies and elapsed time limits")
    # The run deadline includes public config and model/runtime setup.
    started = time.monotonic()
    config = _configuration(args)
    output = reserve_slot(root, "datasets", args.run_id)
    try:
        manifest_path = _manifest(args, root, spec)
        deployment = load_manifest(manifest_path, spec) if args.model_family != "legacy-resnet" else None
        if deployment is not None and deployment["architecture_family"] != args.model_family:
            raise ValueError("typed selfplay model family differs from the deployment manifest")
        search = _search(args, spec, manifest_path, args.backend)
        from .adapter_client import GameAdapterClient
    except (Exception, KeyboardInterrupt) as error:
        _record_selfplay_failure(root, args.run_id, output / "episode-0000.json", None, error,
                                 save_attempted=False, replay_saved=False)
        raise
    episodes = []
    stopped_reason = None
    for game in range(args.games):
        was_cancelled = cancelled()
        if was_cancelled or (time.monotonic() - started) * 1000 >= args.elapsed_ms:
            stopped_reason = "cancelled" if was_cancelled else "elapsed"
            break
        recorder = None
        save_attempted = replay_saved = False
        path = output / f"episode-{game:04d}.json"
        try:
            position = GameAdapterClient.new_game(config, (args.seed + game) % 2**32, spec=spec)
            recorder = EpisodeRecorder({viewer: position.observe(viewer) for viewer in ("white", "black")}, spec,
                        environment_seed=(args.seed + game) % 2**32, belief_seed=args.belief_seed,
                        evidence_kind="bounded-verification" if args.verification else "selfplay",
                        model_sha256=search.evaluator.session.model_sha256,
                        architecture_family=None if args.model_family == "legacy-resnet" else args.model_family,
                        base_hash=deployment["base_hash"] if deployment is not None else None,
                        adapter_hash=deployment["adapter_hash"] if deployment is not None else None,
                        adapter_descriptor=deployment["adapter"] if deployment is not None else None)
            limits = BeliefLimits(particles=args.particles, proposals=args.proposals, elapsed_ms=args.belief_ms)
            beliefs = {viewer: ParticleBelief(tracker, NativeSourceFactory(config,
                        typed_spec=spec if args.model_family != "legacy-resnet" else None), seed=args.belief_seed + index,
                        limits=limits, cancelled=cancelled) for index, (viewer, tracker) in enumerate(recorder.trackers.items())}
            reason = "ply-limit"
            for _ in range(args.max_plies):
                if position.result is not None:
                    reason = "source-terminal"
                    break
                was_cancelled = cancelled()
                if was_cancelled or (time.monotonic() - started) * 1000 >= args.elapsed_ms:
                    reason = "cancelled" if was_cancelled else "elapsed"
                    break
                actor = position.decision_actor
                if recorder.trackers[actor].latest != position.observe(actor):
                    raise ValueError("environment public projection diverged from the complete replay")
                result = search.run(beliefs[actor], cancelled=cancelled)
                recorder.record_decision(actor, result)
                was_cancelled = cancelled()
                if was_cancelled or result.stop_reason == "cancelled" or (time.monotonic() - started) * 1000 >= args.elapsed_ms:
                    reason = "cancelled" if was_cancelled or result.stop_reason == "cancelled" else "elapsed"
                    break
                action = position.bind_public_intent(result.intent)
                was_cancelled = cancelled()
                if was_cancelled or (time.monotonic() - started) * 1000 >= args.elapsed_ms:
                    reason = "cancelled" if was_cancelled else "elapsed"
                    break
                child = position.apply(action).position
                recorder.advance({viewer: child.observe(viewer) for viewer in ("white", "black")}, actor=actor, intent=result.intent)
                position = child
            recorder.finish(position.result, "source-terminal" if position.result is not None else reason)
            save_attempted = True
            recorder.save(path)
            replay_saved = True
            episodes.append({"path": str(path), **recorder.outcome})
            if reason in ("cancelled", "elapsed"):
                stopped_reason = reason
                break
        except (Exception, KeyboardInterrupt) as error:
            if recorder is not None and position.result is None and _cancelled_search_budget(error, cancelled):
                try:
                    recorder.finish(None, "cancelled")
                    save_attempted = True
                    recorder.save(path)
                    replay_saved = True
                except (Exception, KeyboardInterrupt) as save_error:
                    _record_selfplay_failure(root, args.run_id, path, recorder, save_error,
                                             save_attempted=save_attempted, replay_saved=replay_saved)
                    raise
                episodes.append({"path": str(path), **recorder.outcome})
                stopped_reason = "cancelled"
                break
            _record_selfplay_failure(root, args.run_id, path, recorder, error,
                                     save_attempted=save_attempted, replay_saved=replay_saved)
            raise
    was_cancelled = cancelled()
    report = {"episodes": episodes, "stop_reason": stopped_reason or ("cancelled" if was_cancelled else ("elapsed" if (time.monotonic() - started) * 1000 >= args.elapsed_ms else "games")),
              "games_requested": args.games, "evidence_kind": "bounded-verification" if args.verification else "selfplay"}
    atomic_json(slot(root, "reports", args.run_id) / "selfplay.json", report)
    return report


def stage_sample(args, root, spec, cancelled):
    """Generate a legal source position and one bounded student rollout."""
    start = time.monotonic()
    from dataclasses import replace
    from .staged import END, MIDDLE, SyntheticConfig, generate_position, replay_position, rollout_stage

    if not args.generate_only and (args.stage == "middle") != bool(args.teacher_manifest):
        raise ValueError("only middle stage requires a frozen --teacher-manifest")
    config = _configuration(args)
    style = config.get("gameStyle", "normal")
    if set(config) != {"gameStyle"} or style not in ("normal", "chaos"):
        raise ValueError("stage experiment currently accepts only plain Basic or Chaos GameConfig")
    stage = replace(MIDDLE if args.stage == "middle" else END, max_actions=args.max_stage_actions)
    search = _search(args, spec, args.manifest, args.backend)
    teacher = ProductionEvaluator(args.teacher_manifest, spec, args.backend, threads=args.threads) if args.teacher_manifest else None
    generation = SyntheticConfig(stage, args.seed, max_actions=args.max_generation_actions,
                                 capture_bias=args.capture_bias,
                                 assumed_ply_center=25 if stage.name == "middle" else 45)
    def peak_rss_bytes():
        if os.name != "posix":
            return None
        import resource
        return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * 1024
    output = reserve_slot(root, "datasets", args.run_id)
    generated_seconds = None
    try:
        if args.source_provenance:
            source_provenance = read_json(args.source_provenance)
            if (not isinstance(source_provenance, dict)
                    or source_provenance.get("seed") != args.seed
                    or source_provenance.get("format") != style):
                raise ValueError("source provenance differs from requested seed or format")
            if source_provenance.get("stage") == stage.name:
                position, trackers = replay_position(source_provenance, spec)
                provenance = source_provenance
                candidate_probes = 0
                generation_method = "replay"
            elif stage.name == "end" and source_provenance.get("stage") == "middle":
                position, trackers, provenance = generate_position(generation, spec,
                    game_style=style, search=search, prefix_provenance=source_provenance)
                candidate_probes = provenance["candidate_probe_transitions"]
                generation_method = "extend"
            else:
                raise ValueError("source provenance does not precede the requested stage")
        else:
            position, trackers, provenance = generate_position(generation, spec, game_style=style,
                                                               search=search)
            candidate_probes = provenance["candidate_probe_transitions"]
            generation_method = "sample"
        generated_seconds = time.monotonic() - start
        atomic_json(output / "position-provenance.json", provenance)
        if args.generate_only:
            report = {"status": "complete", "stage": stage.name, "format": style,
                      "generation_seconds": generated_seconds,
                      "generation_actions": len(provenance["actions"]),
                      "candidate_probe_transitions": candidate_probes,
                      "generation_method": generation_method,
                      "provenance": str(output / "position-provenance.json"),
                      "statistics": provenance["statistics"]}
            atomic_json(slot(root, "reports", args.run_id) / "stage-generation.json", report)
            return report
        result = rollout_stage(position, trackers, stage, search, teacher, config,
                               belief_seed=args.belief_seed, belief_ms=args.belief_ms,
                               particles=args.particles, proposals=args.proposals,
                               cancelled=cancelled)
        if not result["samples"]:
            raise ValueError("stage rollout produced no root policy samples")
        teacher_deployment = ({"manifest_sha256": file_sha256(args.teacher_manifest),
             "base_hash": load_manifest(args.teacher_manifest, spec)["base_hash"],
             "model_sha256": teacher.session.model_sha256}
             if result["teacher_checkpoint_sha256"] is not None else None)
        payload = {"version": "accelerate-staged-v1", "encoder_hash": spec.digest,
                   "architecture_family": args.model_family, "student_model_sha256": search.evaluator.session.model_sha256,
                   "teacher_checkpoint_sha256": result["teacher_checkpoint_sha256"],
                   "teacher_deployment": teacher_deployment,
                   "provenance": provenance, **result}
        path = output / "samples.json"
        atomic_json(path, payload)
        elapsed = time.monotonic() - start
        report = {"status": "complete", "stage": stage.name, "format": style,
                  "dataset": str(path), "generated_samples": len(result["samples"]),
                  "seed": args.seed, "rules_version": spec.rules_version,
                  "student_model_sha256": search.evaluator.session.model_sha256,
                  "backend": args.backend, "threads": args.threads,
                  "search_budget": {"iterations": args.iterations, "depth": args.depth,
                                    "search_ms": args.search_ms, "nodes": args.nodes,
                                    "edges": args.edges, "candidates": args.candidates},
                  "generation_actions": len(provenance["actions"]),
                  "candidate_probe_transitions": candidate_probes,
                  "generation_method": generation_method,
                  "stage_actions": len(result["samples"]) + len(result["boundary_actions"]),
                  "generation_seconds": generated_seconds, "wall_seconds": elapsed,
                  "samples_per_second": len(result["samples"]) / elapsed,
                  "transitions_per_second": (len(provenance["actions"]) + candidate_probes + len(result["samples"]) + len(result["boundary_actions"])) / elapsed,
                  "mcts_nodes_per_sample": sum(item["mcts_nodes"] for item in result["samples"]) / len(result["samples"]),
                  "bootstrap_ratio": float(result["outcome"].startswith("bootstrap:")),
                  "terminal_ratio": float(result["outcome"] == "terminal"),
                  "teacher_inference_count": int(result["outcome"].startswith("bootstrap:")),
                  "teacher_inference_seconds": result["teacher_inference_seconds"],
                  "belief_initialization_seconds": result["belief_initialization_seconds"],
                  "root_belief_seconds": result["root_belief_seconds"],
                  "belief_diagnostics": result["belief_diagnostics"],
                  "terminal_rollout_length": len(result["samples"]) if result["outcome"] == "terminal" else None,
                  "invalid_generated_state_rejection_ratio": 0.0,
                  "teacher_checkpoint_sha256": result["teacher_checkpoint_sha256"],
                  "teacher_deployment": teacher_deployment,
                  "peak_rss_bytes": peak_rss_bytes(),
                  "gpu_memory_bytes": None,
                  "statistics": result["statistics"]}
        if args.compare_full:
            # Same seed, model and search limits; use the staged wall budget for
            # the full arm. An unfinished full game remains explicitly unfinished.
            full_args = argparse.Namespace(**vars(args))
            full_args.run_id = f"{args.run_id}-full"
            full_args.games = 1
            full_args.max_plies = args.full_max_plies
            full_args.elapsed_ms = min(86_400_000, max(1000, math.ceil(elapsed * 1000)))
            full_args.verification = False
            full_args.particles = args.particles
            full_args.proposals = args.proposals
            full_args.belief_ms = args.belief_ms
            full_started = time.monotonic()
            full_report = selfplay(full_args, root, spec, cancelled)
            full_seconds = time.monotonic() - full_started
            episode = ReplayEpisode.load(full_report["episodes"][0]["path"], spec) if full_report["episodes"] else None
            full_decisions = len(episode.decisions) if episode is not None else 0
            full_terminal = episode is not None and episode.outcome["status"] == "terminal"
            full_samples = full_decisions if full_terminal else 0
            report["full_comparison"] = {
                "seed": args.seed, "full_wall_seconds": full_seconds,
                "full_decisions": full_decisions,
                "full_training_samples": full_samples,
                "full_samples_per_second": full_samples / full_seconds,
                "full_mcts_iterations_per_decision": (sum(item["search"]["iterations"]
                    for item in episode.decisions) / full_decisions) if full_decisions else None,
                "full_terminal": full_terminal,
                "full_stop_reason": full_report["stop_reason"],
                "full_transitions": episode.trackers["white"].steps if episode is not None else 0,
                "budget_ms": full_args.elapsed_ms,
                "same_search_config": True}
        atomic_json(slot(root, "reports", args.run_id) / "stage-sample.json", report)
        return report
    except (Exception, KeyboardInterrupt) as error:
        atomic_json(slot(root, "reports", args.run_id) / "stage-sample-failure.json",
                    {"status": "failed", "error": type(error).__name__, "reason": str(error),
                     "dataset_slot": str(output),
                     "belief_diagnostics": getattr(error, "belief_diagnostics", None),
                     "belief_initialization_seconds": getattr(error, "belief_initialization_seconds", None),
                     "root_belief_seconds": getattr(error, "root_belief_seconds", None),
                     "stage_samples_completed": getattr(error, "stage_samples_completed", None),
                     "search_diagnostics": getattr(error, "search_diagnostics", None),
                     "generation_seconds": generated_seconds,
                     "wall_seconds": time.monotonic() - start,
                     "peak_rss_bytes": peak_rss_bytes()})
        raise


def train(args, root, spec, cancelled):
    if (root / "runs" / args.run_id).is_symlink():
        raise ValueError("training run slot is a symlink; choose a new --run-id")
    directory = slot(root, "runs", args.run_id)
    with writer_claim(directory):
        checkpoint = directory / "training.pt"
        if checkpoint.exists() or checkpoint.is_symlink():
            if not args.resume:
                raise FileExistsError("training run slot already contains a checkpoint; pass --resume for this checkpoint or choose a new --run-id")
            if Path(args.resume).expanduser().resolve() != checkpoint.resolve():
                raise FileExistsError("training run slot contains a different checkpoint; choose a new --run-id to resume from another checkpoint")
        typed = args.model_family != "legacy-resnet"
        model, _ = load_typed_base(args.base, spec) if typed else load_base(args.base, spec)
        if typed and _typed_loaded_family(model) != args.model_family:
            raise ValueError("typed training base architecture differs from the explicit CLI family")
        if args.adapter:
            if args.mode != "adapter":
                raise ValueError("adapter checkpoint cannot be used in base training mode")
            if typed:
                load_typed_adapter(model, spec, args.adapter)
            else:
                load_adapter(model, spec, args.adapter)
        if args.device == "cuda" and not torch.cuda.is_available():
            raise ValueError("requested CUDA device is unavailable")
        model.to(args.device)
        optimizer = create_optimizer(model, mode=args.mode, learning_rate=args.learning_rate)
        if args.staged:
            if not typed:
                raise ValueError("staged samples require the v7 typed model contract")
            from .staged import StagedDataset
            dataset = StagedDataset(args.replay, spec, args.model_family)
        else:
            dataset = ReplayDataset(args.replay, spec, architecture_family=args.model_family if typed else None)
        cursor = DatasetCursor(dataset, args.seed)
        previous = load_training_checkpoint(model, optimizer, spec, cursor, args.resume) if args.resume else 0
        limits = TrainingLimits(steps=args.steps, batch_size=args.batch_size, elapsed_ms=args.elapsed_ms,
                      max_parameter_state_bytes=args.memory_mib * 1024 * 1024)
        if not 1 <= args.checkpoint_every <= 1_000_000:
            raise ValueError("checkpoint interval must be finite and positive")
        last_saved = previous
        def checkpoint_progress(completed):
            nonlocal last_saved
            if completed % args.checkpoint_every == 0:
                save_training_checkpoint(model, optimizer, spec, cursor, checkpoint, completed_steps=previous + completed)
                last_saved = previous + completed
        try:
            if typed:
                from .ir import TypedEncoder

                encoder = TypedEncoder(spec)
            else:
                encoder = PublicEncoder(spec)
            report = optimize(model, optimizer, encoder, cursor, limits=limits, cancelled=cancelled, on_step=checkpoint_progress)
            save_training_checkpoint(model, optimizer, spec, cursor, checkpoint, completed_steps=previous + report["steps"])
        except (Exception, KeyboardInterrupt) as error:
            # Do not retry, lower resources or relabel data. A prior valid checkpoint
            # remains intact; a non-finite failed state cannot overwrite it.
            atomic_json(slot(root, "reports", args.run_id) / "training-failure.json",
                        {"status": "failed", "error": type(error).__name__, "reason": str(error), "last_checkpointed_step": last_saved})
            raise
        if args.mode == "base":
            if typed:
                save_typed_base(model, spec, directory / "base.pt")
            else:
                save_base(model, spec, directory / "base.pt")
        else:
            if typed:
                save_typed_adapter(model, spec, directory / "adapter.pt")
            else:
                save_adapter(model, spec, directory / "adapter.pt")
        report.update({"checkpoint": str(checkpoint), "completed_steps": previous + report["steps"], "mode": args.mode})
        if typed:
            report["architecture_family"] = args.model_family
        atomic_json(slot(root, "reports", args.run_id) / "training.json", report)
        return report


def export(args, root, spec):
    typed = args.model_family != "legacy-resnet"
    sample_inputs = _typed_sample_inputs(args.sample_replay, spec, args.model_family,
                                         sample_public=args.sample_public) if typed else None
    raw_slot = root / "models" / args.slot
    if raw_slot.is_symlink():
        raise ValueError("deployment slot is a symlink; choose a new --slot")
    directory = slot(root, "models", args.slot)
    with writer_claim(directory):
        outputs = (directory / "model.onnx", directory / "manifest.json")
        if any(path.is_dir() and not path.is_symlink() for path in outputs):
            raise IsADirectoryError("deployment output path is a directory; choose a new --slot")
        if any(path.exists() or path.is_symlink() for path in outputs):
            raise FileExistsError("deployment slot already contains model.onnx or manifest.json; choose a new --slot")
        model, _ = load_typed_base(args.base, spec) if typed else load_base(args.base, spec)
        if typed and _typed_loaded_family(model) != args.model_family:
            raise ValueError("typed export base architecture differs from the explicit CLI family")
        if args.adapter:
            descriptor = load_typed_adapter(model, spec, args.adapter) if typed else load_adapter(model, spec, args.adapter)
        else:
            descriptor = None
        if typed:
            manifest = export_typed_onnx(model, spec, directory, sample_inputs,
                                         architecture_family=args.model_family, descriptor=descriptor)
        else:
            manifest = export_onnx(model, spec, directory, descriptor=descriptor)
        result = {"manifest": str(manifest), "model_sha256": load_manifest(manifest, spec)["model_sha256"],
                  "activated": False}
        if typed:
            result["architecture_family"] = args.model_family
        return result


def evaluate(args, root, spec, cancelled):
    if not 1 <= args.max_samples <= 4096:
        raise ValueError("evaluation needs a finite positive sample limit")
    manifest = _manifest(args, root, spec)
    evaluator = ProductionEvaluator(manifest, spec, args.backend, threads=args.threads)
    episode = ReplayEpisode.load(args.replay, spec)
    typed = args.model_family != "legacy-resnet"
    if typed:
        from .ir import ObservationIR, TypedEncoder, batch_typed_positions

        if evaluator.architecture_family != args.model_family or episode.architecture_family != args.model_family:
            raise ValueError("typed evaluation model and replay architecture differ")
        encoder = TypedEncoder(spec)
    else:
        encoder = PublicEncoder(spec)
    metrics = []
    interrupted = False
    for record in episode.decisions[:args.max_samples]:
        if cancelled():
            interrupted = True
            break
        observation = episode.trackers[record["actor"]].frame_at(record["trace_step"])
        candidates = record["candidates"]
        intents = [item["intent"] for item in candidates]
        if typed:
            ir = ObservationIR.from_public(observation, spec, belief_summary=record["belief_summary"])
            batch = batch_typed_positions([encoder.encode(ir, intents)])
            order = spec.feature_schema["input_order"][args.model_family]
            inputs = dict(zip(order, batch.as_family_inputs(args.model_family), strict=True))
            logits, values = evaluator.evaluate_typed(inputs)
            if not np.array_equal(batch.candidate_mask[0],
                                  np.arange(batch.candidate_mask.shape[1]) < len(intents)):
                raise ValueError("typed evaluation candidate mask differs from replay intents")
            logits = logits[0, batch.candidate_mask[0]].astype(np.float64)
        else:
            encoded = encoder.encode(observation, intents, belief_summary=record["belief_summary"])
            logits, values = evaluator.evaluate(encoded.board[None], encoded.condition[None], encoded.action_features[None])
            logits = logits[0].astype(np.float64)
        if (logits.shape != (len(intents),) or values.shape != (1, 1)
                or not np.isfinite(logits).all() or not np.isfinite(values).all()
                or np.abs(values).max() > 1.00001):
            raise ValueError("evaluation returned invalid policy/value for the replay candidates")
        log_policy = logits - (logits.max() + np.log(np.exp(logits - logits.max()).sum()))
        policy = np.array([item["probability"] for item in candidates])
        metric = {"policy_ce": float(-(policy * log_policy).sum()), "value_prediction": float(values[0, 0])}
        if episode.outcome["status"] == "terminal":
            winner = episode.outcome["winner"]
            target = 0. if winner == "draw" else (1. if winner == record["actor"] else -1.)
            metric["value_mse"] = (metric["value_prediction"] - target)**2
        metrics.append(metric)
    stop_reason = "cancelled" if interrupted else ("sample-limit" if len(episode.decisions) > args.max_samples else "complete")
    report = {"version": "accelerate-evaluation-v2" if typed else "accelerate-evaluation-v1", "samples": len(metrics),
              "decisions_available": len(episode.decisions), "sample_limit": args.max_samples,
              "stop_reason": stop_reason, "metrics": metrics, "backend": args.backend,
              "episode_status": episode.outcome["status"], "replay_hash": episode.replay_hash,
              "encoder_hash": spec.digest, "model_sha256": evaluator.session.model_sha256,
              "activated": False}
    if typed:
        report["architecture_family"] = args.model_family
    atomic_json(slot(root, "reports", args.run_id) / "evaluation.json", report)
    return report


def activate(args, root, spec):
    typed = args.model_family != "legacy-resnet"
    sample_inputs = _typed_sample_inputs(args.sample_replay, spec, args.model_family,
                                         sample_public=args.sample_public) if typed else None
    manifest_path = Path(args.manifest).resolve()
    manifest = load_manifest(manifest_path, spec)
    if len(args.expected_sha256) != 64 or manifest["model_sha256"] != args.expected_sha256:
        raise ValueError("explicit activation SHA-256 does not match the verified artifact")
    if typed and manifest["architecture_family"] != args.model_family:
        raise ValueError("typed activation architecture differs from the explicit CLI family")
    evaluator = ProductionEvaluator(manifest_path, spec, args.backend, threads=args.threads)
    if typed:
        if evaluator.architecture_family != args.model_family:
            raise ValueError("typed activation runtime architecture differs from the explicit CLI family")
        logits, value = evaluator.evaluate_typed(sample_inputs)
        expected_shape = sample_inputs["candidate_mask"].shape
    else:
        logits, value = evaluator.evaluate(np.zeros((1, spec.board_channels, 8, 8), np.float32),
            np.zeros((1, spec.condition_dim), np.float32), np.zeros((1, 1, spec.action_dim), np.float32))
        expected_shape = (1, 1)
    if (logits.shape != expected_shape or value.shape != (1, 1)
            or not np.isfinite(logits).all() or not np.isfinite(value).all()
            or np.abs(value).max() > 1.00001):
        raise ValueError("activation native inference smoke returned non-finite outputs")
    reference = {"version": "accelerate-activation-v2" if typed else "accelerate-activation-v1",
                 "manifest": str(manifest_path),
                 "manifest_sha256": file_sha256(manifest_path), "model_sha256": manifest["model_sha256"],
                 "encoder_hash": spec.digest, "backend": args.backend}
    if typed:
        reference.update({"architecture_family": args.model_family,
                          "model_io_version": manifest["model_io_version"]})
    path = slot(root, "models", "active") / "activation.json"
    atomic_json(path, reference)
    return {"activation": str(path), **reference}


def parser():
    main = argparse.ArgumentParser(prog="accelerate-chess")
    main.add_argument("--artifact-root", help="external fixed artifact root; defaults to host APPDATA/Accelerate")
    main.add_argument("--catalog", help="explicit frozen catalog JSON; installed native catalog is the default")
    main.add_argument("--observation-policy", help="explicit frozen observation policy JSON; installed native policy is the default")
    main.add_argument("--model-family", choices=("legacy-resnet", "mask-resnet", "entity-transformer"),
                      default="legacy-resnet", help="select a versioned model input contract")
    main.add_argument("--threads", type=int, default=1, help="CPU threads for Torch and Rust ort (1..64); tract requires 1")
    commands = main.add_subparsers(dest="command", required=True)
    init = commands.add_parser("init")
    init.add_argument("--slot", default="default")
    init.add_argument("--seed", type=int, default=42)
    init.add_argument("--channels", type=int, default=128)
    init.set_defaults(blocks_explicit=False)
    init.add_argument("--blocks", type=int, default=8, action=_ExplicitBlocks,
                      help="residual or Transformer block count; defaults to 8 or 4")
    init.add_argument("--rank", type=int, default=8)
    init.add_argument("--overwrite", action="store_true")
    init.add_argument("--warm-start-legacy-base", help="mask-resnet only: copy matching legacy residual convolutions; never resume optimizer state")
    for name in ("choose", "selfplay"):
        command = commands.add_parser(name)
        command.add_argument("--config", help="public GameConfig JSON; defaults to source normal mode")
        command.add_argument("--manifest")
        command.add_argument("--backend", choices=("ort", "tract"), default="ort", help="explicit execution backend; activation provenance does not change this selection")
        command.add_argument("--belief-seed", type=int, default=71)
        command.add_argument("--particles", type=int, default=8)
        command.add_argument("--proposals", type=int, default=64)
        command.add_argument("--belief-ms", type=int, default=5000)
        command.add_argument("--iterations", type=int, default=32)
        command.add_argument("--leaf-batch-size", type=int, default=4)
        command.add_argument("--depth", type=int, default=8)
        command.add_argument("--search-ms", type=int, default=1000)
        command.add_argument("--nodes", type=int, default=4096)
        command.add_argument("--edges", type=int, default=65536)
        command.add_argument("--candidates", type=int, default=256)
        if name == "choose":
            command.add_argument("--trace", required=True)
        else:
            command.add_argument("--games", type=int, default=1)
            command.add_argument("--max-plies", type=int, default=2)
            command.add_argument("--elapsed-ms", type=int, default=10000)
            command.add_argument("--seed", type=int, default=37)
            command.add_argument("--run-id", default="verification")
            command.add_argument("--verification", action="store_true")
    command = commands.add_parser("train")
    command.add_argument("--base", required=True)
    command.add_argument("--adapter")
    command.add_argument("--mode", choices=("base", "adapter"), default="base")
    command.add_argument("--replay", nargs="+", required=True)
    command.add_argument("--staged", action="store_true", help="train on validated staged public samples")
    command.add_argument("--resume", help="existing training checkpoint; must be this slot's checkpoint when the run ID already exists")
    command.add_argument("--checkpoint-every", type=int, default=100)
    command.add_argument("--run-id", default="training")
    command.add_argument("--seed", type=int, default=19)
    command.add_argument("--steps", type=int, default=1)
    command.add_argument("--batch-size", type=int, default=2)
    command.add_argument("--elapsed-ms", type=int, default=60000)
    command.add_argument("--memory-mib", type=int, default=1024)
    command.add_argument("--learning-rate", type=float, default=3e-4)
    command.add_argument("--device", choices=("cpu", "cuda"), default="cpu")
    staged = commands.add_parser("stage-sample")
    staged.add_argument("--stage", choices=("end", "middle"), required=True)
    staged.add_argument("--generate-only", action="store_true")
    staged.add_argument("--source-provenance",
                        help="replay an earlier generated position by verified source actions")
    staged.add_argument("--config", help="public GameConfig JSON; Basic by default")
    staged.add_argument("--manifest", required=True, help="frozen student deployment manifest")
    staged.add_argument("--teacher-manifest", help="frozen next-stage deployment manifest; required for middle")
    staged.add_argument("--backend", choices=("ort", "tract"), default="ort")
    staged.add_argument("--seed", type=int, default=37)
    staged.add_argument("--belief-seed", type=int, default=71)
    staged.add_argument("--belief-ms", type=int, default=30000)
    staged.add_argument("--particles", type=int, default=1)
    staged.add_argument("--proposals", type=int, default=4)
    staged.add_argument("--run-id", required=True)
    staged.add_argument("--max-generation-actions", type=int, default=256)
    staged.add_argument("--max-stage-actions", type=int, default=128)
    staged.add_argument("--capture-bias", type=float, default=2.)
    staged.add_argument("--iterations", type=int, default=8)
    staged.add_argument("--leaf-batch-size", type=int, default=2)
    staged.add_argument("--depth", type=int, default=4)
    staged.add_argument("--search-ms", type=int, default=1000)
    staged.add_argument("--nodes", type=int, default=1024)
    staged.add_argument("--edges", type=int, default=8192)
    staged.add_argument("--candidates", type=int, default=128)
    staged.add_argument("--compare-full", action="store_true",
                        help="run existing full selfplay for the staged wall-time budget")
    staged.add_argument("--full-max-plies", type=int, default=4096)
    command = commands.add_parser("export")
    command.add_argument("--base", required=True)
    command.add_argument("--adapter")
    command.add_argument("--slot", default="deployment")
    export_sample = command.add_mutually_exclusive_group()
    export_sample.add_argument("--sample-replay", help="typed model only: public replay used to derive exact ONNX input shapes")
    export_sample.add_argument("--sample-public", help="typed model only: versioned public observation and intents used to bootstrap export")
    command = commands.add_parser("evaluate")
    command.add_argument("--manifest")
    command.add_argument("--backend", choices=("ort", "tract"), default="ort", help="explicit execution backend; tract requires --threads 1")
    command.add_argument("--replay", required=True)
    command.add_argument("--max-samples", type=int, default=8)
    command.add_argument("--run-id", default="evaluation")
    command = commands.add_parser("activate")
    command.add_argument("--manifest", required=True)
    command.add_argument("--expected-sha256", required=True)
    command.add_argument("--backend", choices=("ort", "tract"), default="ort", help="backend used to verify activation; recorded as provenance, with no automatic runtime selection")
    activation_sample = command.add_mutually_exclusive_group()
    activation_sample.add_argument("--sample-replay", help="typed model only: source-bound public replay for native inference smoke")
    activation_sample.add_argument("--sample-public", help="typed model only: versioned public observation and intents for native inference smoke")
    return main


def main(arguments=None):
    args = parser().parse_args(arguments)
    stopped = False
    def cancel(signum, frame):
        nonlocal stopped
        stopped = True
    previous = signal.signal(signal.SIGINT, cancel)
    try:
        if not 1 <= args.threads <= 64:
            raise ValueError("CPU threads must be bounded to 1..64")
        if hasattr(args, "seed") and not 0 <= args.seed < 2**32:
            raise ValueError("command seed must fit uint32 without silent normalization")
        if hasattr(args, "belief_seed") and not 0 <= args.belief_seed < 2**32 - 1:
            raise ValueError("independent belief seed must fit uint32 including its second viewer")
        if args.command in ("choose", "selfplay", "stage-sample") and args.model_family == "legacy-resnet":
            raise ValueError("v6 game execution is retired; choose and selfplay require a v7 typed model family")
        torch.set_num_threads(args.threads)
        policy = read_json(args.observation_policy) if args.observation_policy else None
        spec = default_spec(args.catalog, observation_policy=policy, model_family=args.model_family)
        if args.command in ("choose", "selfplay", "stage-sample") and spec.rules_version != V7_RULES_VERSION:
            raise ValueError("v6 game execution is retired; choose and selfplay require the pinned v7 rules")
        root = artifact_root(args.artifact_root)
        cancelled = lambda: stopped
        if args.command == "init":
            result = initialize(args, root, spec)
        elif args.command == "choose":
            result = choose(args, root, spec, cancelled)
        elif args.command == "selfplay":
            result = selfplay(args, root, spec, cancelled)
        elif args.command == "stage-sample":
            result = stage_sample(args, root, spec, cancelled)
        elif args.command == "train":
            result = train(args, root, spec, cancelled)
        elif args.command == "export":
            result = export(args, root, spec)
        elif args.command == "evaluate":
            result = evaluate(args, root, spec, cancelled)
        else:
            result = activate(args, root, spec)
        print(canonical_json(result))
        if stopped:
            return 130
        return 2 if args.command in ("selfplay", "train") and result.get("stop_reason") == "elapsed" else 0
    except (Exception, KeyboardInterrupt) as error:
        print(f"{type(error).__name__}: {error}", file=sys.stderr)
        return (130 if isinstance(error, KeyboardInterrupt)
                or stopped and _cancelled_search_budget(error, lambda: stopped) else 2)
    finally:
        signal.signal(signal.SIGINT, previous)


if __name__ == "__main__":
    raise SystemExit(main())
