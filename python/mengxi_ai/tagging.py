# tagging.py — CLIP zero-shot tag generation for colorist-oriented vocabulary

import logging
import os
import tempfile
from typing import Optional

import numpy as np
import onnxruntime as ort
from PIL import Image

logger = logging.getLogger(__name__)

# CLIP ViT-B/32 image normalization constants
CLIP_MEAN = np.array([0.48145466, 0.4578275, 0.40821073], dtype=np.float32)
CLIP_STD = np.array([0.26862954, 0.26130258, 0.27577711], dtype=np.float32)
CLIP_IMAGE_SIZE = (224, 224)

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


def _discover_text_encoder(models_dir: str) -> str:
    """Find a CLIP text encoder ONNX model in the models directory.

    Looks for common naming patterns like ``text_encoder.onnx``,
    ``clip_text_encoder.onnx``, or any file with ``text`` in the name.

    Raises FileNotFoundError if no text encoder is found.
    """
    if not os.path.isdir(models_dir):
        raise FileNotFoundError(f"Models directory not found: {models_dir}")

    onnx_files = sorted(
        f for f in os.listdir(models_dir) if f.endswith(".onnx")
    )

    # Prefer well-known naming conventions
    for name in ["text_encoder.onnx", "clip_text_encoder.onnx", "textual.onnx"]:
        if name in onnx_files:
            return name

    # Fall back to any file containing "text"
    for name in onnx_files:
        if "text" in name.lower():
            return name

    raise FileNotFoundError(
        f"No text encoder model found in {models_dir}. "
        f"Available models: {onnx_files}. "
        "Need a text encoder ONNX model (e.g. text_encoder.onnx)."
    )


def _load_text_encoder_session(models_dir: str, text_model_name: Optional[str]) -> ort.InferenceSession:
    """Load the CLIP text encoder as a separate ONNX Runtime session."""
    if text_model_name:
        model_path = os.path.join(models_dir, text_model_name)
    else:
        discovered = _discover_text_encoder(models_dir)
        model_path = os.path.join(models_dir, discovered)

    if not os.path.isfile(model_path):
        raise FileNotFoundError(f"Text encoder model not found: {model_path}")

    logger.info("Loading text encoder: %s", model_path)
    return ort.InferenceSession(model_path)


def _get_cache_key(model_name: str, models_dir: str) -> str:
    """Generate cache key based on model name and mtime."""
    model_path = os.path.join(models_dir, model_name)
    mtime = 0
    if os.path.isfile(model_path):
        mtime = int(os.path.getmtime(model_path))
    safe_name = model_name.replace(".onnx", "").replace("/", "_").replace("\\", "_")
    return f"tag_text_embeddings_{safe_name}_mtime{mtime}.npy"


def _load_or_compute_text_embeddings(
    text_session: ort.InferenceSession,
    models_dir: str,
    model_name: str,
    candidate_tags: list[str],
    tokenizer,  # ClipTokenizer
) -> np.ndarray:
    """Load cached text embeddings or compute and cache them (atomic write + mtime invalidation).

    Returns a numpy array of shape (num_tags, embedding_dim).
    """
    cache_path = os.path.join(models_dir, _get_cache_key(model_name, models_dir))

    if os.path.isfile(cache_path):
        logger.info("Loading cached text embeddings from %s", cache_path)
        embeddings = np.load(cache_path)
        if embeddings.shape[0] == len(candidate_tags):
            return embeddings
        else:
            logger.info(
                "Cache mismatch (expected %d tags, got %d), recomputing",
                len(candidate_tags),
                embeddings.shape[0],
            )

    # Tokenize candidate tags using CLIP BPE tokenizer
    logger.info("Computing text embeddings for %d tags...", len(candidate_tags))
    token_ids = tokenizer.encode_batch(candidate_tags)
    input_array = np.array(token_ids, dtype=np.int64)  # (num_tags, 77)

    input_name = text_session.get_inputs()[0].name
    output = text_session.run(None, {input_name: input_array})

    text_features = output[0]  # (num_tags, dim) or (num_tags, 77, dim)

    # Handle 3D output (full hidden states) — use EOS token position
    if text_features.ndim == 3:
        text_features = text_features[:, -1, :]  # (num_tags, dim)

    # L2-normalize each text embedding
    norms = np.linalg.norm(text_features, axis=1, keepdims=True)
    norms = np.maximum(norms, 1e-8)
    text_features = text_features / norms

    # Atomic write: write to temp file, then rename
    try:
        os.makedirs(models_dir, exist_ok=True)

        temp_fd, temp_path = tempfile.mkstemp(dir=models_dir, suffix=".npy.tmp")
        try:
            with os.fdopen(temp_fd, "wb") as f:
                np.save(f, text_features)
            # Atomic rename (overwrites target if exists)
            os.replace(temp_path, cache_path)
            logger.info(
                "Cached text embeddings to %s (%d tags, dim=%d)",
                cache_path,
                text_features.shape[0],
                text_features.shape[1],
            )
        except (OSError, IOError) as e:
            try:
                os.unlink(temp_path)
            except OSError:
                pass
            raise
    except OSError as e:
        logger.warning("Failed to cache text embeddings: %s", e)

    return text_features


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
        model_name: Optional image encoder model filename (without path). Auto-discovers if empty.
        models_dir: Optional override for models directory.
        top_n: Number of top tags to return (default 5).

    Returns:
        List of tag strings, sorted by confidence (highest first).

    Raises:
        FileNotFoundError: If image, model, tokenizer files, or text encoder not found.
        RuntimeError: If inference fails.
    """
    from .models import ModelRegistry
    from .tokenizer import ClipTokenizer

    # Merge custom tags with defaults: custom first for priority in ranking
    if candidate_tags is not None:
        seen = set(candidate_tags)
        merged = list(candidate_tags)
        for t in CANDIDATE_TAGS:
            if t not in seen:
                merged.append(t)
                seen.add(t)
        tags = merged
    else:
        tags = CANDIDATE_TAGS

    if top_n <= 0:
        top_n = len(tags)
    if top_n > len(tags):
        top_n = len(tags)

    registry = ModelRegistry(models_dir=models_dir)

    # Load the CLIP image encoder
    model_info = registry.load_model(model_name)
    image_session = registry.session

    # Discover text encoder model name for cache keying
    text_model_name = None
    try:
        text_model_name = _discover_text_encoder(registry.models_dir)
    except FileNotFoundError:
        pass  # will fail in _load_text_encoder_session with a clear error

    # Load the CLIP text encoder as a separate ONNX session
    text_session = _load_text_encoder_session(registry.models_dir, text_model_name)

    # Load CLIP BPE tokenizer
    tokenizer = ClipTokenizer(registry.models_dir)

    # Encode image
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

    # Load or compute text embeddings (separate encoder + tokenizer)
    text_embeddings = _load_or_compute_text_embeddings(
        text_session,
        registry.models_dir,
        text_model_name or "unknown",
        tags,
        tokenizer,
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
