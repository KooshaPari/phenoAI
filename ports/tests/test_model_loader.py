"""5 smoke tests for the ModelLoader port + adapters."""
import pytest
from ..model_loader import ModelRef, InferenceRequest, InferenceResponse
from ..adapters.local import LocalLoader
from ..registry import resolve, register


class TestModelLoaderPort:
    def test_local_loader_default_state(self):
        loader = LocalLoader()
        assert loader.adapter_name == "local-safetensors"
        assert loader.is_loaded is False

    def test_inference_request_is_frozen(self):
        req = InferenceRequest(prompt="hi", max_tokens=10)
        with pytest.raises(Exception):
            req.prompt = "bye"  # type: ignore[misc]

    def test_inference_response_finish_reason_valid(self):
        r = InferenceResponse(text="x", tokens_used=1, finish_reason="stop")
        assert r.finish_reason in ("stop", "length", "error")

    def test_model_ref_defaults(self):
        ref = ModelRef(uri="local:///tmp/x")
        assert ref.revision == "main"
        assert ref.quantization == "f16"

    @pytest.mark.asyncio
    async def test_local_loader_lifecycle(self, tmp_path):
        f = tmp_path / "model.safetensors"
        f.write_bytes(b"fake")
        loader = LocalLoader()
        await loader.load(ModelRef(uri=f"local://{f}"))
        assert loader.is_loaded is True
        await loader.unload()
        assert loader.is_loaded is False


class TestRegistry:
    def test_resolve_known_scheme(self):
        loader = resolve("local:///tmp/m.safetensors")
        assert isinstance(loader, LocalLoader)

    def test_resolve_unknown_scheme_raises(self):
        with pytest.raises(ValueError, match="No adapter"):
            resolve("unknown://x")

    def test_register_custom_adapter(self):
        class FakeAdapter(LocalLoader):
            @property
            def adapter_name(self): return "fake"
        register("fake://", FakeAdapter)
        assert isinstance(resolve("fake://x"), FakeAdapter)
