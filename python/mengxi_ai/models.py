# models.py — Model registry & pluggable loader (NFR14)

import logging
import os
from typing import Optional

import numpy as np
import onnxruntime as ort

logger = logging.getLogger(__name__)

# Default model directory (relative to this file's parent's parent)
DEFAULT_MODELS_DIR = os.path.join(os.path.dirname(os.path.dirname(__file__)), "models")


class ModelInfo:
    """Metadata about a loaded embedding model."""

    def __init__(self, name: str, input_shape: tuple, output_dim: int):
        self.name = name
        self.input_shape = input_shape
        self.output_dim = output_dim


class ModelRegistry:
    """Registry for pluggable embedding models."""

    def __init__(self, models_dir: Optional[str] = None):
        self.models_dir = models_dir or DEFAULT_MODELS_DIR
        self._model_name: Optional[str] = None
        self._session: Optional[ort.InferenceSession] = None
        self._model_info: Optional[ModelInfo] = None

    def discover_models(self) -> list[str]:
        """List available .onnx model files in the models directory."""
        if not os.path.isdir(self.models_dir):
            return []
        return [
            f
            for f in sorted(os.listdir(self.models_dir))
            if f.endswith(".onnx")
        ]

    def load_model(self, model_name: Optional[str] = None) -> ModelInfo:
        """Load an ONNX model by name. Uses first discovered .onnx file if name is None."""
        # Determine which model to load
        if model_name:
            model_path = os.path.join(self.models_dir, model_name)
        else:
            available = self.discover_models()
            if not available:
                raise FileNotFoundError(
                    f"No ONNX models found in {self.models_dir}"
                )
            model_name = available[0]
            model_path = os.path.join(self.models_dir, model_name)

        if not os.path.isfile(model_path):
            raise FileNotFoundError(
                f"Model not found: {model_path}"
            )

        logger.info("Loading model: %s", model_path)
        self._session = ort.InferenceSession(model_path)

        # Inspect model metadata
        inputs = self._session.get_inputs()
        outputs = self._session.get_outputs()
        input_shape = inputs[0].shape if inputs else (1, 3, 224, 224)
        output_dim = outputs[0].shape[-1] if outputs else 512

        self._model_name = model_name
        self._model_info = ModelInfo(
            name=model_name,
            input_shape=tuple(input_shape),
            output_dim=int(output_dim),
        )

        logger.info(
            "Model loaded: %s (input=%s, output_dim=%d)",
            model_name,
            self._model_info.input_shape,
            self._model_info.output_dim,
        )
        return self._model_info

    @property
    def session(self) -> ort.InferenceSession:
        """Get the loaded ONNX Runtime session."""
        if self._session is None:
            raise RuntimeError("No model loaded. Call load_model() first.")
        return self._session

    @property
    def model_info(self) -> ModelInfo:
        """Get metadata about the currently loaded model."""
        if self._model_info is None:
            raise RuntimeError("No model loaded. Call load_model() first.")
        return self._model_info
