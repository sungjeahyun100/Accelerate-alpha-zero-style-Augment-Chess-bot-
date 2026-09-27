"""Accelerate's single Python package.

Native rules objects are imported on demand so encoding and model tooling can
also run before a platform wheel has been built. Missing native code is an
explicit import failure when rules or inference are requested.
"""

__version__ = "0.1.0"
__all__ = ["Position", "Action", "ActionStream", "StepResult", "NativeError", "StaleActionError", "UnsupportedFeatureError", "ConditioningMismatchError", "site_catalog", "InferenceSession", "ProductionEvaluator"]


def __getattr__(name: str):
    if name == "ProductionEvaluator":
        from .inference import ProductionEvaluator

        globals()[name] = ProductionEvaluator
        return ProductionEvaluator
    if name in __all__:
        from . import _native

        value = getattr(_native, name)
        globals()[name] = value
        return value
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
