# logging_config.py — Structured logging configuration

import json
import logging
import sys
from datetime import datetime, timezone


class JsonFormatter(logging.Formatter):
    """JSON log formatter for structured logging."""

    def format(self, record: logging.LogRecord) -> str:
        log_entry = {
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "level": record.levelname,
            "logger": record.name,
            "message": record.getMessage(),
        }
        if record.exc_info:
            log_entry["exception"] = self.formatException(record.exc_info)
        return json.dumps(log_entry)


def setup_logging(json_logging: bool = False) -> None:
    """Configure logging for the service.

    Args:
        json_logging: If True, use JSON format; otherwise plain text.
    """
    level = logging.INFO

    if json_logging:
        fmt = JsonFormatter()
        logging.basicConfig(
            level=level,
            format="%(message)s",
            stream=sys.stderr,
            force=True,
        )
        for handler in logging.root.handlers:
            handler.setFormatter(fmt)
    else:
        logging.basicConfig(
            level=level,
            format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
            datefmt="%Y-%m-%d %H:%M:%S",
            stream=sys.stderr,
            force=True,
        )
