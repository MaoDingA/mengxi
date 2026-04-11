"""Health check utilities for startup validation."""

import logging
import os
from typing import Optional

logger = logging.getLogger(__name__)


def check_startup_health(models_dir: Optional[str] = None) -> dict[str, bool | int]:
    """Run startup health checks and return status dictionary.

    Returns:
        Dict with keys: models_dir_exists, vocab_exists, merges_exists,
        onnx_files_found, onnx_file_count, all_ok.
    """
    if models_dir is None:
        models_dir = os.path.join(
            os.path.dirname(os.path.dirname(__file__)), "models"
        )

    checks: dict[str, bool | int] = {
        "models_dir_exists": os.path.isdir(models_dir),
        "vocab_exists": os.path.isfile(os.path.join(models_dir, "vocab.json")),
        "merges_exists": os.path.isfile(os.path.join(models_dir, "merges.txt")),
    }

    # Count ONNX files
    onnx_files: list[str] = []
    if checks["models_dir_exists"]:
        onnx_files = [
            f for f in os.listdir(models_dir) if f.endswith(".onnx")
        ]
    checks["onnx_files_found"] = len(onnx_files) > 0
    checks["onnx_file_count"] = len(onnx_files)
    checks["all_ok"] = all(checks[k] for k in ["models_dir_exists", "onnx_files_found"])

    return checks


def log_startup_report(checks: dict[str, bool | int]) -> None:
    """Log startup health check results to stderr."""
    logger.info("=" * 50)
    logger.info("Mengxi AI Service - Startup Health Check")
    logger.info("=" * 50)

    for key, value in checks.items():
        if key == "onnx_file_count":
            logger.info("  %s: %d", key, value)
        elif isinstance(value, bool):
            status = "OK" if value else "MISSING"
            logger.info("  %s: %s", key, status)

    if checks.get("all_ok"):
        logger.info("Health check: PASSED")
    else:
        logger.warning("Health check: FAILED - Some dependencies missing")

    logger.info("=" * 50)
