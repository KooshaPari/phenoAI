"""T61: phenoAI hexagonal port — ModelLoader.

3 adapters: HuggingFace, Local (safetensors), Remote (HTTP).
"""
from __future__ import annotations
from abc import ABC, abstractmethod
from dataclasses import dataclass
from pathlib import Path
from typing import AsyncIterator, Optional

@dataclass(frozen=True)
class ModelRef:
    """Reference to a model artifact. Format depends on the adapter."""
    uri: str                   # e.g. "hf://meta-llama/Llama-3.1-8B", "local:///path/to/model.safetensors", "remote://api.example.com/v1/models/foo"
    revision: str = "main"
    quantization: str = "f16"  # f16, q4_k_m, q8_0, etc.

@dataclass(frozen=True)
class InferenceRequest:
    prompt: str
    max_tokens: int = 512
    temperature: float = 0.7
    top_p: float = 0.95
    stop: tuple[str, ...] = ()

@dataclass(frozen=True)
class InferenceResponse:
    text: str
    tokens_used: int
    finish_reason: str  # "stop" | "length" | "error"

class ModelLoader(ABC):
    """Hexagonal port: load a model + run inference."""
    @abstractmethod
    async def load(self, ref: ModelRef) -> None: ...
    @abstractmethod
    async def unload(self) -> None: ...
    @abstractmethod
    async def infer(self, req: InferenceRequest) -> InferenceResponse: ...
    @abstractmethod
    async def infer_stream(self, req: InferenceRequest) -> AsyncIterator[str]: ...
    @property
    @abstractmethod
    def is_loaded(self) -> bool: ...
    @property
    @abstractmethod
    def adapter_name(self) -> str: ...
