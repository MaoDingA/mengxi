# embedding.py — ONNX model loading & image embedding inference

import logging
from typing import Optional

import numpy as np
from PIL import Image

from .models import ModelRegistry

logger = logging.getLogger(__name__)

# Default image size for embedding models
DEFAULT_IMAGE_SIZE = (224, 224)


def _preprocess_image(image_path: str, target_size: tuple[int, int]) -> np.ndarray:
    """Load and preprocess an image for embedding inference.

    Returns a numpy array of shape (1, 3, H, W) with float32 values in [0, 1].
    """
    img = Image.open(image_path).convert("RGB")

    # Resize to target size
    img = img.resize(target_size, Image.LANCZOS)

    # Convert to numpy and normalize to [0, 1]
    arr = np.array(img, dtype=np.float32) / 255.0

    # Convert HWC to CHW format
    arr = np.transpose(arr, (2, 0, 1))

    # Add batch dimension
    return np.expand_dims(arr, axis=0)


def generate_embedding(
    image_path: str,
    model_name: Optional[str] = None,
    models_dir: Optional[str] = None,
) -> list[float]:
    """Generate an embedding vector for an image using the loaded ONNX model.

    Args:
        image_path: Path to the image file.
        model_name: Optional model filename (without path). Uses first available if None.
        models_dir: Optional override for models directory.

    Returns:
        List of float values representing the embedding vector.

    Raises:
        FileNotFoundError: If image or model file not found.
        RuntimeError: If model inference fails.
    """
    registry = ModelRegistry(models_dir=models_dir)
    model_info = registry.load_model(model_name)

    logger.info("Preprocessing image: %s", image_path)
    input_array = _preprocess_image(
        image_path,
        target_size=model_info.input_shape[2:4]
        if len(model_info.input_shape) >= 4
        else DEFAULT_IMAGE_SIZE,
    )

    logger.info("Running inference (output_dim=%d)...", model_info.output_dim)

    # CRITICAL: Acquire session lock before calling run()
    # get_cached_session returns (session, model_info, session_lock)
    _, _, session_lock = registry.get_cached_session(registry.models_dir, model_name)
    session = registry.session

    with session_lock:
        input_name = session.get_inputs()[0].name
        output = session.run(None, {input_name: input_array})

    # Extract embedding vector from output
    embedding = output[0].flatten()

    # Normalize the embedding vector to unit length for cosine similarity
    norm = np.linalg.norm(embedding)
    if norm > 0:
        embedding = embedding / norm

    logger.info("Embedding generated: dim=%d", len(embedding))
    return embedding.tolist()
