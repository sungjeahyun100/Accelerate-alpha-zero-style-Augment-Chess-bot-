"""Production evaluator using exactly the explicitly selected Rust backend."""
from __future__ import annotations

from os import PathLike
from .encoding import EncoderSpec
from .ir import TypedEncoderSpec


class ProductionEvaluator:
    def __init__(self, manifest_path: str | PathLike[str], expected_spec: EncoderSpec | TypedEncoderSpec,
                 backend: str = "ort", **limits: int):
        from ._native import InferenceSession

        self.spec = expected_spec
        self.session = InferenceSession(manifest_path, backend, expected_spec.digest, **limits)

    @property
    def backend(self) -> str:
        return self.session.backend

    @property
    def architecture_family(self) -> str:
        return self.session.architecture_family

    def evaluate(self, board, condition, action_features):
        return self.session.evaluate(board, condition, action_features)

    def evaluate_typed(self, inputs):
        return self.session.evaluate_typed(inputs)
