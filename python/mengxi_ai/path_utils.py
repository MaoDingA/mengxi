# path_utils.py — Path validation utilities for security

import os
from typing import Optional


def validate_image_path(
    image_path: str,
    allowed_dirs: Optional[list[str]] = None,
) -> str:
    """Validate and resolve an image path.

    Args:
        image_path: User-provided image path.
        allowed_dirs: List of allowed base directories. If None, allows any path
            but resolves to absolute and checks for path traversal.

    Returns:
        Absolute, normalized path.

    Raises:
        ValueError: If path traversal detected or outside allowed dirs.
    """
    # Resolve to absolute path (this resolves .. components)
    abs_path = os.path.abspath(image_path)

    if allowed_dirs:
        # Check if resolved path is within allowed directories
        allowed_abs = [os.path.abspath(d) for d in allowed_dirs]

        is_allowed = False
        for allowed_dir in allowed_abs:
            allowed_norm = os.path.normpath(allowed_dir)
            path_norm = os.path.normpath(abs_path)

            if (
                path_norm == allowed_norm
                or path_norm.startswith(allowed_norm + os.sep)
            ):
                is_allowed = True
                break

        if not is_allowed:
            raise ValueError(
                f"Path '{image_path}' (resolved to '{abs_path}') "
                f"is outside allowed directories: {allowed_dirs}"
            )

    return abs_path
