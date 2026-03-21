#!/usr/bin/env python3
# main.py — stdin/stdout JSON protocol handler for Mengxi AI service

import json
import logging
import sys
import uuid

logger = logging.getLogger(__name__)

# Configure logging: INFO to stderr only (stdout reserved for JSON protocol)
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
    stream=sys.stderr,
)


def handle_generate_embedding(params: dict) -> dict:
    """Handle generate_embedding method request."""
    from .embedding import generate_embedding

    image_path = params.get("image_path")
    if not image_path:
        return {"code": "INVALID_PARAMS", "message": "'image_path' is required"}

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


def handle_ping(params: dict) -> dict:
    """Health check handler."""
    return {"status": "ok"}


METHOD_HANDLERS = {
    "generate_embedding": handle_generate_embedding,
    "ping": handle_ping,
}


def main():
    """Main loop: read JSON requests from stdin, write JSON responses to stdout."""
    logger.info("Mengxi AI service started (stdin/stdout JSON protocol)")

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue

        try:
            request = json.loads(line)
        except json.JSONDecodeError as e:
            response = {
                "request_id": "",
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
            response = {
                "request_id": request_id,
                "status": "ok",
                "result": result,
            }
        except Exception as e:
            logger.exception("Error handling method %s", method)
            response = {
                "request_id": request_id,
                "status": "error",
                "error": {
                    "code": "INTERNAL_ERROR",
                    "message": str(e),
                },
            }

        print(json.dumps(response), flush=True)


if __name__ == "__main__":
    main()
