# tests/test_main.py — Unit tests for the main JSON protocol handler

import json
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from mengxi_ai.main import METHOD_HANDLERS, handle_ping


class TestMethodHandlers(unittest.TestCase):
    """Tests for method handler dispatch."""

    def test_ping_handler_exists(self):
        self.assertIn("ping", METHOD_HANDLERS)
        self.assertIn("generate_embedding", METHOD_HANDLERS)

    def test_ping_returns_ok_status(self):
        result = handle_ping({})
        self.assertEqual(result, {"status": "ok"})

    def test_ping_accepts_empty_params(self):
        result = handle_ping({})
        self.assertEqual(result, {"status": "ok"})

    def test_ping_ignores_extra_params(self):
        result = handle_ping({"foo": "bar"})
        self.assertEqual(result, {"status": "ok"})

    def test_generate_embedding_missing_image_path(self):
        from mengxi_ai.main import handle_generate_embedding
        result = handle_generate_embedding({})
        self.assertEqual(result["code"], "INVALID_PARAMS")
        self.assertIn("image_path", result["message"])

    def test_generate_tags_rejects_non_list_candidate_tags(self):
        from mengxi_ai.main import handle_generate_tags
        result = handle_generate_tags({
            "image_path": "/tmp/test.png",
            "candidate_tags": "not-a-list",
        })
        self.assertEqual(result["code"], "INVALID_PARAMS")
        self.assertIn("must be a list", result["message"])

    def test_generate_tags_rejects_non_string_candidate_tag_elements(self):
        from mengxi_ai.main import handle_generate_tags
        result = handle_generate_tags({
            "image_path": "/tmp/test.png",
            "candidate_tags": ["valid", 42, None],
        })
        self.assertEqual(result["code"], "INVALID_PARAMS")
        self.assertIn("must be strings", result["message"])

    def test_generate_tags_accepts_empty_list_candidate_tags(self):
        from mengxi_ai.main import handle_generate_tags
        # Empty list should pass validation (error expected from missing image, not validation)
        result = handle_generate_tags({
            "image_path": "/tmp/test.png",
            "candidate_tags": [],
        })
        # Should not be INVALID_PARAMS for candidate_tags
        self.assertNotEqual(result.get("code"), "INVALID_PARAMS")


class TestJsonProtocol(unittest.TestCase):
    """Tests for JSON protocol message format."""

    def test_request_format(self):
        request = {
            "request_id": "test-123",
            "method": "ping",
            "params": {},
        }
        serialized = json.dumps(request)
        parsed = json.loads(serialized)
        self.assertEqual(parsed["request_id"], "test-123")
        self.assertEqual(parsed["method"], "ping")

    def test_response_format_ok(self):
        response = {
            "request_id": "test-123",
            "status": "ok",
            "result": {"status": "ok"},
        }
        serialized = json.dumps(response)
        parsed = json.loads(serialized)
        self.assertEqual(parsed["status"], "ok")

    def test_response_format_error(self):
        response = {
            "request_id": "test-123",
            "status": "error",
            "error": {"code": "TIMEOUT", "message": "30s exceeded"},
        }
        serialized = json.dumps(response)
        parsed = json.loads(serialized)
        self.assertEqual(parsed["error"]["code"], "TIMEOUT")


if __name__ == "__main__":
    unittest.main()
