#!/usr/bin/env python3
# main.py — stdin/stdout JSON protocol handler for Mengxi AI service
#
# Phase B: Signal handling, stdin timeout, payload limits, path validation
# Phase C: Batch parallelization via ThreadPoolExecutor
# Phase D: Protocol version, latency metrics, config integration

import json
import logging
import select
import signal
import sys
import threading
import time
import uuid
from concurrent.futures import ThreadPoolExecutor, as_completed

logger = logging.getLogger(__name__)

# Configure logging: INFO to stderr only (stdout reserved for JSON protocol)
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
    stream=sys.stderr,
)

# --- Phase B.1: Signal handlers ---

_shutdown_requested = False
_shutdown_lock = threading.Lock()


def request_shutdown(signum=None, frame=None) -> None:
    """Signal handler for graceful shutdown."""
    global _shutdown_requested
    with _shutdown_lock:
        if _shutdown_requested:
            return
        _shutdown_requested = True
    logger.info("Shutdown requested (signal %d), cleaning up...", signum)


def should_shutdown() -> bool:
    """Check if shutdown has been requested."""
    global _shutdown_requested
    with _shutdown_lock:
        return _shutdown_requested


signal.signal(signal.SIGTERM, request_shutdown)
signal.signal(signal.SIGINT, request_shutdown)

# --- Constants ---

PROTOCOL_VERSION = "1"
MAX_PAYLOAD_BYTES = 50 * 1024 * 1024  # 50MB
IDLE_TIMEOUT_SECONDS = 300  # 5 minutes, matches Rust side


def handle_generate_embedding(params: dict) -> dict:
    """Handle generate_embedding method request."""
    from .embedding import generate_embedding
    from .path_utils import validate_image_path

    image_path = params.get("image_path")
    if not image_path:
        return {"code": "INVALID_PARAMS", "message": "'image_path' is required"}

    # Phase B.4: Path validation
    try:
        validate_image_path(image_path)
    except ValueError as e:
        return {"code": "INVALID_PATH", "message": str(e)}

    model_name = params.get("model_name")  # Optional
    models_dir = params.get("models_dir")  # Optional

    try:
        embedding = generate_embedding(
            image_path=image_path,
            model_name=model_name,
            models_dir=models_dir,
        )
        return {"embedding": embedding}
    except FileNotFoundError as e:
        return {"code": "FILE_NOT_FOUND", "message": str(e)}
    except RuntimeError as e:
        return {"code": "INFERENCE_ERROR", "message": str(e)}
    except Exception as e:
        logger.exception("Unexpected error during inference")
        return {"code": "INFERENCE_ERROR", "message": str(e)}


def handle_generate_embeddings_batch(params: dict) -> dict:
    """Handle generate_embeddings_batch method request (parallel, Phase C.4).

    Thread Safety: ModelRegistry sessions are protected by per-session locks.
    Image preprocessing runs in parallel; session.run() is serialized per model.
    """
    from .embedding import generate_embedding

    images = params.get("images")
    if not images:
        return {"code": "INVALID_PARAMS", "message": "'images' is required"}

    if not isinstance(images, list):
        return {"code": "INVALID_PARAMS", "message": "'images' must be a list"}

    if not all(isinstance(img, str) for img in images):
        return {"code": "INVALID_PARAMS", "message": "'images' elements must be strings"}

    model_name = params.get("model_name")  # Optional
    models_dir = params.get("models_dir")  # Optional

    try:
        embeddings = []
        # Phase C.4: ThreadPoolExecutor for parallel inference
        max_workers = min(4, len(images))
        with ThreadPoolExecutor(max_workers=max_workers) as executor:
            futures = {
                executor.submit(
                    generate_embedding,
                    image_path=image_path,
                    model_name=model_name,
                    models_dir=models_dir,
                ): image_path
                for image_path in images
            }

            for future in as_completed(futures):
                try:
                    result = future.result()
                    embeddings.append(result)
                except Exception as e:
                    logger.exception("Error in batch item")
                    raise

        return {"embeddings": embeddings}
    except FileNotFoundError as e:
        return {"code": "FILE_NOT_FOUND", "message": str(e)}
    except RuntimeError as e:
        return {"code": "INFERENCE_ERROR", "message": str(e)}
    except Exception as e:
        logger.exception("Unexpected error during batch inference")
        return {"code": "INFERENCE_ERROR", "message": str(e)}


def handle_generate_tags(params: dict) -> dict:
    """Handle generate_tags method request."""
    from .tagging import generate_tags
    from .path_utils import validate_image_path

    image_path = params.get("image_path")
    if not image_path:
        return {"code": "INVALID_PARAMS", "message": "'image_path' is required"}

    # Phase B.4: Path validation
    try:
        validate_image_path(image_path)
    except ValueError as e:
        return {"code": "INVALID_PATH", "message": str(e)}

    model_name = params.get("model_name", "")
    top_n = params.get("top_n", 5)
    candidate_tags = params.get("candidate_tags")  # Optional: list of custom tags

    if not isinstance(top_n, int) or top_n < 1:
        return {"code": "INVALID_PARAMS", "message": "'top_n' must be a positive integer"}

    if candidate_tags is not None and not isinstance(candidate_tags, list):
        return {"code": "INVALID_PARAMS", "message": "'candidate_tags' must be a list"}

    if candidate_tags is not None and candidate_tags and not all(isinstance(t, str) for t in candidate_tags):
        return {"code": "INVALID_PARAMS", "message": "'candidate_tags' elements must be strings"}

    try:
        tags = generate_tags(
            image_path=image_path,
            model_name=model_name,
            top_n=top_n,
            candidate_tags=candidate_tags,
        )
        return {"tags": tags, "count": len(tags)}
    except FileNotFoundError as e:
        return {"code": "AI_MODEL_NOT_FOUND", "message": str(e)}
    except RuntimeError as e:
        return {"code": "AI_INFERENCE_ERROR", "message": str(e)}
    except Exception as e:
        logger.exception("Unexpected error during tag generation")
        return {"code": "AI_INFERENCE_ERROR", "message": str(e)}


def handle_ping(params: dict) -> dict:
    """Health check handler."""
    return {"status": "ok"}


METHOD_HANDLERS = {
    "generate_embedding": handle_generate_embedding,
    "generate_embeddings_batch": handle_generate_embeddings_batch,
    "generate_tags": handle_generate_tags,
    "ping": handle_ping,
}


def main():
    """Main loop: read JSON requests from stdin, write JSON responses to stdout.

    Phase B.2: select.select timeout for idle detection
    Phase B.3: Payload size limit before JSON parse
    Phase D.2: Protocol version field in responses
    Phase D.4: Latency metrics logging
    """
    logger.info("Mengxi AI service started (stdin/stdout JSON protocol)")

    # Startup health check (Phase A.5)
    from .health import check_startup_health, log_startup_report

    health_checks = check_startup_health()
    log_startup_report(health_checks)

    while True:
        # Phase B.1: Check shutdown flag
        if should_shutdown():
            logger.info("Shutdown flag set, exiting main loop")
            break

        # Phase B.2: Wait for input with timeout
        try:
            readable, _, _ = select.select(
                [sys.stdin], [], [], IDLE_TIMEOUT_SECONDS
            )
        except (ValueError, OSError):
            # Windows fallback: select.select may not support stdin
            readable = [sys.stdin]

        if not readable:
            logger.info("stdin idle timeout (%ds), exiting", IDLE_TIMEOUT_SECONDS)
            sys.exit(0)

        line_raw = sys.stdin.readline()
        if not line_raw:  # EOF
            logger.info("stdin closed, exiting")
            break

        # Phase B.3: Check payload size BEFORE strip/parse
        if len(line_raw.encode("utf-8")) > MAX_PAYLOAD_BYTES:
            response = {
                "request_id": "",
                "version": PROTOCOL_VERSION,
                "status": "error",
                "error": {
                    "code": "PAYLOAD_TOO_LARGE",
                    "message": (
                        f"Request exceeds {MAX_PAYLOAD_BYTES} bytes"
                    ),
                },
            }
            print(json.dumps(response), flush=True)
            continue

        line = line_raw.strip()
        if not line:
            continue

        # Phase D.4: Start timing
        start_time = time.time()

        try:
            request = json.loads(line)
        except json.JSONDecodeError as e:
            response = {
                "request_id": "",
                "version": PROTOCOL_VERSION,
                "status": "error",
                "error": {"code": "PROTOCOL_ERROR", "message": f"Invalid JSON: {e}"},
            }
            print(json.dumps(response), flush=True)
            continue

        request_id = request.get("request_id", str(uuid.uuid4()))
        method = request.get("method", "")
        params = request.get("params", {})

        if method not in METHOD_HANDLERS:
            response = {
                "request_id": request_id,
                "version": PROTOCOL_VERSION,
                "status": "error",
                "error": {
                    "code": "UNKNOWN_METHOD",
                    "message": f"Unknown method: {method}",
                },
            }
            print(json.dumps(response), flush=True)
            continue

        logger.info("Request: %s (request_id=%s)", method, request_id)

        try:
            result = METHOD_HANDLERS[method](params)
            duration_ms = (time.time() - start_time) * 1000

            response = {
                "request_id": request_id,
                "version": PROTOCOL_VERSION,
                "status": "ok",
                "result": result,
            }
            logger.info(
                "request_complete method=%s duration_ms=%.2f status=ok",
                method,
                duration_ms,
            )
        except Exception as e:
            duration_ms = (time.time() - start_time) * 1000
            logger.exception("Error handling method %s", method)
            logger.info(
                "request_complete method=%s duration_ms=%.2f status=error",
                method,
                duration_ms,
            )
            response = {
                "request_id": request_id,
                "version": PROTOCOL_VERSION,
                "status": "error",
                "error": {
                    "code": "INTERNAL_ERROR",
                    "message": str(e),
                },
            }

        print(json.dumps(response), flush=True)


if __name__ == "__main__":
    main()
