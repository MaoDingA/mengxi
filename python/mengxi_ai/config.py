# config.py — Configuration management for Mengxi AI service

import logging
import os
import tomllib  # Python 3.11+
from dataclasses import dataclass, field
from typing import Optional

logger = logging.getLogger(__name__)


@dataclass
class ClipConfig:
    """CLIP model preprocessing parameters."""

    mean: tuple[float, float, float] = (
        0.48145466,
        0.4578275,
        0.40821073,
    )
    std: tuple[float, float, float] = (
        0.26862954,
        0.26130258,
        0.27577711,
    )
    image_size: tuple[int, int] = (224, 224)


@dataclass
class Config:
    """Main configuration for Mengxi AI service."""

    models_dir: str = ""
    vocab_path: str = "vocab.json"
    merges_path: str = "merges.txt"
    max_payload_bytes: int = 50 * 1024 * 1024
    idle_timeout_seconds: int = 300
    model_cache_ttl_seconds: int = 3600
    batch_max_workers: int = 4
    json_logging: bool = False


def load_config(config_path: Optional[str] = None) -> Config:
    """Load configuration from TOML file.

    Args:
        config_path: Path to config file. If None, uses MENGXI_AI_CONFIG env var
            or defaults to ~/.mengxi/ai_config.toml.

    Returns:
        Config object with sensible defaults if file not found.
    """
    if config_path is None:
        config_path = os.environ.get("MENGXI_AI_CONFIG")

    if config_path and os.path.isfile(config_path):
        logger.info("Loading config from %s", config_path)
        with open(config_path, "rb") as f:
            data = tomllib.load(f)
        return _parse_config(data, config_path)
    else:
        logger.info("No config file found, using defaults")
        return _default_config()


def _parse_config(data: dict, source_path: str) -> Config:
    """Parse TOML config data into Config object."""
    base_dir = os.path.dirname(source_path)

    models_dir = data.get("models_dir", "models")
    if not os.path.isabs(models_dir):
        models_dir = os.path.join(base_dir, models_dir)

    clip_data = data.get("clip", {})

    return Config(
        models_dir=models_dir,
        vocab_path=data.get("vocab_path", "vocab.json"),
        merges_path=data.get("merges_path", "merges.txt"),
        max_payload_bytes=data.get("max_payload_bytes", 50 * 1024 * 1024),
        idle_timeout_seconds=data.get("idle_timeout_seconds", 300),
        model_cache_ttl_seconds=data.get("model_cache_ttl_seconds", 3600),
        batch_max_workers=data.get("batch_max_workers", 4),
        json_logging=data.get("json_logging", False),
    )


def _default_config() -> Config:
    """Return default configuration."""
    project_root = os.path.dirname(os.path.dirname(__file__))
    return Config(
        models_dir=os.path.join(project_root, "models"),
    )
