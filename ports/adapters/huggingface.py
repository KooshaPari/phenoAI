"""HuggingFace adapter for ModelLoader (uses transformers)."""
from .model_loader import ModelLoader, ModelRef, InferenceRequest, InferenceResponse
from typing import AsyncIterator

class HuggingFaceLoader(ModelLoader):
    def __init__(self): self._model = None; self._tok = None
    async def load(self, ref: ModelRef) -> None:
        from transformers import AutoModelForCausalLM, AutoTokenizer
        self._tok = AutoTokenizer.from_pretrained(ref.uri, revision=ref.revision)
        self._model = AutoModelForCausalLM.from_pretrained(ref.uri, revision=ref.revision)
    async def unload(self) -> None: self._model = None; self._tok = None
    async def infer(self, req: InferenceRequest) -> InferenceResponse:
        inputs = self._tok(req.prompt, return_tensors="pt")
        out = self._model.generate(**inputs, max_new_tokens=req.max_tokens, temperature=req.temperature, do_sample=req.temperature > 0)
        text = self._tok.decode(out[0], skip_special_tokens=True)
        return InferenceResponse(text=text, tokens_used=len(out[0]), finish_reason="stop")
    async def infer_stream(self, req: InferenceRequest) -> AsyncIterator[str]:
        # Streaming requires a streaming-aware backend; for HF use TextStreamer
        yield (await self.infer(req)).text
    @property
    def is_loaded(self) -> bool: return self._model is not None
    @property
    def adapter_name(self) -> str: return "huggingface"
