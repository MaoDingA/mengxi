# tests/test_models.py — Unit tests for model registry

import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from mengxi_ai.models import ModelInfo, ModelRegistry


class TestModelInfo(unittest.TestCase):
    """Tests for ModelInfo data class."""

    def test_model_info_creation(self):
        info = ModelInfo(name="test.onnx", input_shape=(1, 3, 224, 224), output_dim=512)
        self.assertEqual(info.name, "test.onnx")
        self.assertEqual(info.input_shape, (1, 3, 224, 224))
        self.assertEqual(info.output_dim, 512)


class TestModelRegistry(unittest.TestCase):
    """Tests for ModelRegistry."""

    def test_discover_models_empty_dir(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            registry = ModelRegistry(models_dir=tmpdir)
            models = registry.discover_models()
            self.assertEqual(models, [])

    def test_discover_models_finds_onnx_files(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            # Create fake .onnx files
            open(os.path.join(tmpdir, "model_a.onnx"), "w").close()
            open(os.path.join(tmpdir, "model_b.onnx"), "w").close()
            # Create non-onnx file
            open(os.path.join(tmpdir, "readme.txt"), "w").close()

            registry = ModelRegistry(models_dir=tmpdir)
            models = registry.discover_models()
            self.assertEqual(models, ["model_a.onnx", "model_b.onnx"])

    def test_discover_models_nonexistent_dir(self):
        registry = ModelRegistry(models_dir="/nonexistent/path")
        models = registry.discover_models()
        self.assertEqual(models, [])

    def test_load_model_no_models_raises(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            registry = ModelRegistry(models_dir=tmpdir)
            with self.assertRaises(FileNotFoundError):
                registry.load_model(None)

    def test_load_model_specific_not_found_raises(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            registry = ModelRegistry(models_dir=tmpdir)
            with self.assertRaises(FileNotFoundError):
                registry.load_model("nonexistent.onnx")

    def test_session_raises_without_load(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            registry = ModelRegistry(models_dir=tmpdir)
            with self.assertRaises(RuntimeError):
                _ = registry.session

    def test_model_info_raises_without_load(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            registry = ModelRegistry(models_dir=tmpdir)
            with self.assertRaises(RuntimeError):
                _ = registry.model_info


if __name__ == "__main__":
    unittest.main()
