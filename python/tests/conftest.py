"""Shared test fixtures and mocks for mengxi_ai tests."""

import json
import os
import sys
import tempfile
from typing import Optional

import numpy as np

# Add parent directory to path for imports
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))


class MockOnnxSession:
    """Mock ONNX InferenceSession for testing."""

    def __init__(
        self,
        output_shape: tuple = (1, 512),
        output_dim: int = 512,
        input_name: str = "input",
    ):
        self._output_shape = output_shape
        self._output_dim = output_dim
        self._input_name = input_name
        self._inputs = [_MockInput(input_name, shape=(1, 3, 224, 224))]
        self._outputs = [_MockOutput("output", shape=output_shape)]

    def get_inputs(self):
        return self._inputs

    def get_outputs(self):
        return self._outputs

    def run(self, _, feed_dict):
        np.random.seed(42)
        return [np.random.randn(*self._output_shape).astype(np.float32)]


class _MockInput:
    def __init__(self, name: str, shape: tuple):
        self.name = name
        self.shape = shape


class _MockOutput:
    def __init__(self, name: str, shape: tuple):
        self.name = name
        self.shape = shape


class MockImage:
    """Mock PIL.Image for testing."""

    def __init__(self, size=(224, 224), mode="RGB"):
        self._size = size
        self._mode = mode
        self._data = np.random.randint(0, 255, (*size[::-1], len(mode)), dtype=np.uint8)

    def convert(self, mode):
        return MockImage(self._size, mode)

    def resize(self, size, _):
        return MockImage(size, self._mode)

    def toarray(self):
        return self._data

    def __array__(self, dtype=None):
        """Support np.array() conversion."""
        return self._data.astype(dtype) if dtype else self._data


def mock_image_open(_path: str) -> MockImage:
    """Mock PIL.Image.open that returns a test image."""
    return MockImage()


def mock_onnx_inference_session(_model_path: str) -> MockOnnxSession:
    """Mock ort.InferenceSession for testing."""
    return MockOnnxSession()


def create_temp_models_dir() -> str:
    """Create a temporary models directory with required stub files.

    Returns:
        Path to temporary directory with vocab.json, merges.txt, and .onnx stubs.
    """
    tmpdir = tempfile.mkdtemp()

    # Minimal CLIP vocab (real one is ~49K entries)
    vocab_path = os.path.join(tmpdir, "vocab.json")
    with open(vocab_path, "w") as f:
        json.dump({"<|startoftext|>": 49406, "<|endoftext|>": 49407, "a": 320}, f)

    # Merges file
    merges_path = os.path.join(tmpdir, "merges.txt")
    with open(merges_path, "w") as f:
        f.write("#version: 0.2\na b\n")

    # ONNX model stubs
    open(os.path.join(tmpdir, "image_encoder.onnx"), "w").close()
    open(os.path.join(tmpdir, "text_encoder.onnx"), "w").close()

    return tmpdir
