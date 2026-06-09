"""Adapter registry: lookup ModelLoader implementations by URI scheme."""
from .model_loader import ModelLoader
from .adapters.huggingface import HuggingFaceLoader
from .adapters.local import LocalLoader

_REGISTRY: dict[str, type[ModelLoader]] = {
    "hf://": HuggingFaceLoader,
    "local://": LocalLoader,
}


def resolve(uri: str) -> ModelLoader:
    """Pick the right adapter based on the URI scheme."""
    for scheme, cls in _REGISTRY.items():
        if uri.startswith(scheme):
            return cls()
    raise ValueError(f"No adapter registered for URI scheme: {uri!r}")


def register(scheme: str, cls: type[ModelLoader]) -> None:
    """Register a new adapter (used by RemoteLoader, etc.)."""
    _REGISTRY[scheme] = cls
