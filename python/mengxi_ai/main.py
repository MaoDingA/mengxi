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


def handle_generate_embeddings_batch(params: dict) -> dict:
    """Handle generate_embeddings_batch method request."""
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
        for image_path in images:
            embedding = generate_embedding(
                image_path=image_path,
                model_name=model_name,
                models_dir=models_dir,
            )
            embeddings.append(embedding)
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

    image_path = params.get("image_path")
    if not image_path:
        return {"code": "INVALID_PARAMS", "message": "'image_path' is required"}

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
