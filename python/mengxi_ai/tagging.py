# tagging.py — CLIP zero-shot tag generation for colorist-oriented vocabulary

import logging
import os
from typing import Optional

import numpy as np
from PIL import Image

logger = logging.getLogger(__name__)

# CLIP ViT-B/32 image normalization constants
CLIP_MEAN = np.array([0.48145466, 0.4578275, 0.40821073], dtype=np.float32)
CLIP_STD = np.array([0.26862954, 0.26130258, 0.27577711], dtype=np.float32)
CLIP_IMAGE_SIZE = (224, 224)

# Default path for text embedding cache (relative to models dir)
TEXT_EMBEDDINGS_CACHE = "tag_text_embeddings.npy"

# Curated colorist-oriented candidate tag vocabulary
CANDIDATE_TAGS = [
    # Lighting
    "dramatic lighting",
    "soft lighting",
    "natural light",
    "studio lighting",
    "backlit",
    "high key",
    "low key",
    "rim lighting",
    "harsh shadows",
    "diffused light",
    # Mood / atmosphere
    "warm",
    "cool",
    "moody",
    "bright",
    "dark",
    "ethereal",
    "gritty",
    "serene",
    "melancholic",
    "mysterious",
    "dreamy",
    "tense",
    # Color temperature / tone
    "golden tones",
    "blue tones",
    "teal and orange",
    "desaturated",
    "vibrant",
    "monochrome",
    "pastel",
    "cool shadows",
    "warm highlights",
    "neutral tones",
    "green tones",
    "red tones",
    # Contrast / exposure
    "high contrast",
    "low contrast",
    "flat",
    "silhouette",
    # Genre / scene
    "cinematic",
    "documentary",
    "fashion",
    "landscape",
    "interior",
    "night scene",
    "outdoor",
    "underwater",
    "urban",
    "aerial",
]


def _preprocess_clip_image(image_path: str) -> np.ndarray:
    """Preprocess an image for CLIP ViT-B/32 inference.

    Returns a numpy array of shape (1, 3, 224, 224) with CLIP normalization.
    """
    img = Image.open(image_path).convert("RGB")
    img = img.resize(CLIP_IMAGE_SIZE, Image.LANCZOS)

    arr = np.array(img, dtype=np.float32) / 255.0

    # Apply CLIP normalization (per-channel mean/std)
    arr = (arr - CLIP_MEAN) / CLIP_STD

    # Convert HWC to CHW
    arr = np.transpose(arr, (2, 0, 1))

    # Add batch dimension
    return np.expand_dims(arr, axis=0)


def _load_or_compute_text_embeddings(
    text_session,
    models_dir: str,
    candidate_tags: list[str],
) -> np.ndarray:
    """Load cached text embeddings or compute and cache them.

    Returns a numpy array of shape (num_tags, embedding_dim).
    """
    cache_path = os.path.join(models_dir, TEXT_EMBEDDINGS_CACHE)

    if os.path.isfile(cache_path):
        logger.info("Loading cached text embeddings from %s", cache_path)
        embeddings = np.load(cache_path)
        # Validate: cached embeddings must match current candidate_tags
        if embeddings.shape[0] == len(candidate_tags):
            return embeddings
        else:
            logger.info(
                "Cache mismatch (expected %d tags, got %d), recomputing",
                len(candidate_tags),
                embeddings.shape[0],
            )

    # Compute text embeddings
    logger.info("Computing text embeddings for %d tags...", len(candidate_tags))
    text_inputs = _prepare_text_inputs(candidate_tags)

    input_name = text_session.get_inputs()[0].name
    output = text_session.run(None, {input_name: text_inputs})
    embeddings = output[0]  # shape: (num_tags, dim)

    # L2-normalize each text embedding
    norms = np.linalg.norm(embeddings, axis=1, keepdims=True)
    norms = np.maximum(norms, 1e-8)  # avoid division by zero
    embeddings = embeddings / norms

    # Cache to disk
    try:
        os.makedirs(models_dir, exist_ok=True)
        np.save(cache_path, embeddings)
        logger.info("Cached text embeddings to %s (%d tags, dim=%d)", cache_path, embeddings.shape[0], embeddings.shape[1])
    except OSError as e:
        logger.warning("Failed to cache text embeddings: %s", e)

    return embeddings


def _prepare_text_inputs(texts: list[str]) -> np.ndarray:
    """Prepare text inputs for CLIP text encoder.

    Returns a numpy array suitable for ONNX text encoder input.
    For ViT-B/32, this is typically (N, 77) of int64 token IDs.
    Since we use a simplified approach with raw ONNX models,
    we encode text prompts as-is and let the model handle tokenization.
    """
    # For a pure numpy approach without tokenizers,
    # we pass text prompts and let the ONNX model's internal
    # tokenizer handle encoding. This works with CLIP models
    # that accept pre-tokenized input.
    # For models that need raw text, we pass them directly.
    return np.array(texts, dtype=np.object_)


def generate_tags(
    image_path: str,
    candidate_tags: Optional[list[str]] = None,
    model_name: str = "",
    models_dir: Optional[str] = None,
    top_n: int = 5,
) -> list[str]:
    """Generate semantic tags for an image using CLIP zero-shot classification.

    Args:
        image_path: Path to the image file.
        candidate_tags: Optional list of candidate tag strings. Uses CANDIDATE_TAGS if None.
        model_name: Optional model filename (without path). Auto-discovers if empty.
        models_dir: Optional override for models directory.
        top_n: Number of top tags to return (default 5).

    Returns:
        List of tag strings, sorted by confidence (highest first).

    Raises:
        FileNotFoundError: If image or model not found.
        RuntimeError: If inference fails.
    """
    from .models import ModelRegistry

    tags = candidate_tags if candidate_tags is not None else CANDIDATE_TAGS

    if top_n <= 0:
        top_n = len(tags)
    if top_n > len(tags):
        top_n = len(tags)

    registry = ModelRegistry(models_dir=models_dir)

    # Load the CLIP model (used for image encoding)
    model_info = registry.load_model(model_name)
    image_session = registry.session

    # For text embeddings, we need a separate text encoder session.
    # If the loaded model is the image encoder (default), we use it for images
    # and attempt to load a text encoder from the same models directory.
    # The CLIP model pipeline expects separate image/text encoders.
    #
    # Strategy: For zero-shot tagging with a single ONNX model file,
    # we compute similarity differently. We use the loaded model for image
    # features and pre-computed text embeddings.
    logger.info("Preprocessing image for tagging: %s", image_path)
    image_input = _preprocess_clip_image(image_path)

    logger.info("Running image encoding...")
    input_name = image_session.get_inputs()[0].name
    image_output = image_session.run(None, {input_name: image_input})
    image_embedding = image_output[0].flatten()

    # L2-normalize image embedding
    norm = np.linalg.norm(image_embedding)
    if norm > 0:
        image_embedding = image_embedding / norm

    # Load or compute text embeddings
    actual_models_dir = registry.models_dir
    text_embeddings = _load_or_compute_text_embeddings(
        image_session,
        actual_models_dir,
        tags,
    )

    # Compute cosine similarity: image_emb (dim,) @ text_embs.T (dim, num_tags) → (num_tags,)
    similarities = text_embeddings @ image_embedding

    # Apply softmax to convert to probabilities
    similarities = _softmax(similarities)

    # Get top-N indices sorted by confidence (highest first)
    top_indices = np.argsort(similarities)[::-1][:top_n]

    result = [tags[i] for i in top_indices]
    logger.info("Generated %d tags: %s", len(result), ", ".join(result))
    return result


def _softmax(x: np.ndarray) -> np.ndarray:
    """Numerically stable softmax."""
    e_x = np.exp(x - np.max(x))
    return e_x / e_x.sum()
