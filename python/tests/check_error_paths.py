"""Tests for error handling paths."""

import unittest
import unittest.mock as mock
import tempfile
import os

# Import conftest helpers
from conftest import create_temp_models_dir


class TestErrorPaths(unittest.TestCase):
    """Tests for OOM, corrupt model, timeout, malformed JSON scenarios."""

    def setUp(self):
        self.models_dir = create_temp_models_dir()

    def test_out_of_memory_returns_inference_error(self):
        """Simulate OOM during inference maps to INFERENCE_ERROR."""
        from mengxi_ai.main import handle_generate_embedding

        with mock.patch(
            "mengxi_ai.embedding.generate_embedding",
            side_effect=MemoryError("CUDA out of memory"),
        ):
            result = handle_generate_embedding({
                "image_path": "/fake/image.png",
                "model_name": "test.onnx",
                "models_dir": self.models_dir,
            })
        self.assertEqual(result["code"], "INFERENCE_ERROR")
        self.assertIn("memory", result["message"].lower())

    def test_corrupt_model_raises_runtime_error(self):
        """Corrupt ONNX model raises RuntimeError from ModelRegistry."""
        from mengxi_ai.models import ModelRegistry

        with mock.patch(
            "onnxruntime.InferenceSession",
            side_effect=RuntimeError("Invalid ONNX model file"),
        ):
            registry = ModelRegistry(models_dir=self.models_dir)
            with self.assertRaises(RuntimeError) as ctx:
                registry.load_model("image_encoder.onnx")
            self.assertIn("Invalid ONNX model", str(ctx.exception))

    def test_timeout_propagates_as_inference_error(self):
        """Timeout during inference maps to INFERENCE_ERROR."""
        from mengxi_ai.main import handle_generate_embedding

        with mock.patch(
            "mengxi_ai.embedding.generate_embedding",
            side_effect=TimeoutError("Inference timed out after 30s"),
        ):
            result = handle_generate_embedding({
                "image_path": "/fake/image.png",
                "models_dir": self.models_dir,
            })
        self.assertEqual(result["code"], "INFERENCE_ERROR")
        self.assertIn("timed out", result["message"].lower())

    def test_unexpected_exception_maps_to_internal_error(self):
        """Unexpected exceptions are caught and mapped to INFERENCE_ERROR."""
        from mengxi_ai.main import handle_generate_embedding

        with mock.patch(
            "mengxi_ai.embedding.generate_embedding",
            side_effect=ValueError("unexpected internal state"),
        ):
            result = handle_generate_embedding({
                "image_path": "/fake/image.png",
                "models_dir": self.models_dir,
            })
        self.assertEqual(result["code"], "INFERENCE_ERROR")

    def test_tag_generation_model_not_found(self):
        """Missing model in tag generation returns AI_MODEL_NOT_FOUND."""
        from mengxi_ai.main import handle_generate_tags

        result = handle_generate_tags({
            "image_path": "/fake/image.png",
            "models_dir": "/nonexistent/models/dir",
        })
        self.assertIn(result["code"], ("AI_MODEL_NOT_FOUND", "FILE_NOT_FOUND"))

    def test_batch_with_non_list_images(self):
        """Non-list images parameter returns INVALID_PARAMS."""
        from mengxi_ai.main import handle_generate_embeddings_batch

        result = handle_generate_embeddings_batch({
            "images": "not-a-list",
        })
        self.assertEqual(result["code"], "INVALID_PARAMS")

    def test_batch_with_non_string_elements(self):
        """Non-string elements in images list returns INVALID_PARAMS."""
        from mengxi_ai.main import handle_generate_embeddings_batch

        result = handle_generate_embeddings_batch({
            "images": ["valid.png", 42, None],
        })
        self.assertEqual(result["code"], "INVALID_PARAMS")


if __name__ == "__main__":
    unittest.main()
