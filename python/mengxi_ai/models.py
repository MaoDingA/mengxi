# models.py — Model registry with module-level singleton cache (Phase C.1)

import logging
import os
import threading
import time
from typing import Optional

import numpy as np
import onnxruntime as ort

logger = logging.getLogger(__name__)

# Default model directory (relative to this file's parent's parent)
DEFAULT_MODELS_DIR = os.path.join(os.path.dirname(os.path.dirname(__file__)), "models")

# Cache TTL: 1 hour
_MODEL_CACHE_TTL_SECONDS = 3600


class ModelInfo:
    """Metadata about a loaded embedding model."""

    def __init__(self, name: str, input_shape: tuple, output_dim: int):
        self.name = name
        self.input_shape = input_shape
        self.output_dim = output_dim


class ModelRegistry:
    """Registry for pluggable embedding models with module-level caching.

    The registry uses a module-level singleton pattern with thread-safe
    lazy initialization. Models are cached with TTL-based invalidation.

    Thread Safety: Each cached session has its own lock because
    InferenceSession.run() is NOT thread-safe (ONNX Runtime limitation).
    """

    _cache: dict[
        str,
        tuple[ort.InferenceSession, ModelInfo, float, threading.Lock],
    ] = {}
    _cache_lock = threading.Lock()

    def __init__(self, models_dir: Optional[str] = None):
        # Instance is still created per call, but cache is shared
        self.models_dir = models_dir or DEFAULT_MODELS_DIR
        # Track which model this instance loaded (for backward compat)
        self._loaded_model_name: Optional[str] = None

    @classmethod
    def get_cached_session(
        cls,
        models_dir: str,
        model_name: Optional[str] = None,
    ) -> tuple[ort.InferenceSession, ModelInfo, threading.Lock]:
        """Get or create a cached ONNX session with metadata and lock.

        Returns:
            Tuple of (session, model_info, session_lock). The session_lock
            MUST be held when calling session.run().

        Raises:
            FileNotFoundError: If model file not found.
        """
        cache_key = f"{models_dir}:{model_name or 'auto'}"
        now = time.time()

        with cls._cache_lock:
            if cache_key in cls._cache:
                session, model_info, cached_at, session_lock = cls._cache[
                    cache_key
                ]
                if now - cached_at < _MODEL_CACHE_TTL_SECONDS:
                    logger.debug("Cache hit: %s", cache_key)
                    return session, model_info, session_lock
                else:
                    logger.info("Cache expired: %s, reloading", cache_key)
                    del cls._cache[cache_key]

        # Cache miss or expired - load model
        logger.info("Cache miss: %s, loading model", cache_key)

        if model_name:
            model_path = os.path.join(models_dir, model_name)
        else:
            onnx_files = sorted(
                f for f in os.listdir(models_dir) if f.endswith(".onnx")
            )
            if not onnx_files:
                raise FileNotFoundError(
                    f"No ONNX models found in {models_dir}"
                )
            model_name = onnx_files[0]
            model_path = os.path.join(models_dir, model_name)

        if not os.path.isfile(model_path):
            raise FileNotFoundError(f"Model not found: {model_path}")

        session = ort.InferenceSession(model_path)
        session_lock = threading.Lock()
        inputs = session.get_inputs()
        outputs = session.get_outputs()
        input_shape = inputs[0].shape if inputs else (1, 3, 224, 224)
        output_dim = outputs[0].shape[-1] if outputs else 512

        model_info = ModelInfo(
            name=model_name,
            input_shape=tuple(input_shape),
            output_dim=int(output_dim),
        )

        with cls._cache_lock:
            cls._cache[cache_key] = (session, model_info, now, session_lock)

        logger.info(
            "Model loaded and cached: %s (input=%s, output_dim=%d)",
            model_name,
            model_info.input_shape,
            model_info.output_dim,
        )

        return session, model_info, session_lock

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
        """Load an ONNX model by name (delegates to cached session).

        This method exists for backward compatibility. Stores the loaded
        model name on this instance for the session property.
        """
        # Resolve name early so session property reuses the same cache key
        if not model_name:
            available = self.discover_models()
            if not available:
                raise FileNotFoundError(
                    f"No ONNX models found in {self.models_dir}"
                )
            model_name = available[0]

        self._loaded_model_name = model_name
        _, model_info, _ = self.get_cached_session(
            self.models_dir, model_name
        )
        return model_info

    @property
    def session(self) -> ort.InferenceSession:
        """Get the loaded ONNX Runtime session (backward compat).

        Uses the model name that was passed to load_model() on this instance.
        """
        if self._loaded_model_name is None:
            raise RuntimeError("No model loaded. Call load_model() first.")
        session, _, _ = self.get_cached_session(
            self.models_dir, self._loaded_model_name
        )
        return session

    @property
    def model_info(self) -> ModelInfo:
        """Get metadata about the currently loaded model (backward compat)."""
        if self._loaded_model_name is None:
            raise RuntimeError("No model loaded. Call load_model() first.")
        _, model_info, _ = self.get_cached_session(
            self.models_dir, self._loaded_model_name
        )
        return model_info
