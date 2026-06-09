"""Local safetensors adapter for ModelLoader (uses llama.cpp or pure Rust)."""
from .model_loader import ModelLoader, ModelRef, InferenceRequest, InferenceResponse
from typing import AsyncIterator

class LocalLoader(ModelLoader):
    def __init__(self): self._ctx = None
    async def load(self, ref: ModelRef) -> None:
        from pathlib import Path
        path = Path(ref.uri.replace("local://", ""))
        assert path.exists(), f"Model file not found: {path}"
        self._ctx = {"path": path, "quantization": ref.quantization}
    async def unload(self) -> None: self._ctx = None
    async def infer(self, req: InferenceRequest) -> InferenceResponse:
        # Stub: in production, call llama-cpp-python or rust binding
        return InferenceResponse(text=f"[local inference: {req.prompt[:20]}...]", tokens_used=len(req.prompt)//4, finish_reason="stop")
    async def infer_stream(self, req: InferenceRequest) -> AsyncIterator[str]:
        yield (await self.infer(req)).text
    @property
    def is_loaded(self) -> bool: return self._ctx is not None
    @property
    def adapter_name(self) -> str: return "local-safetensors"
