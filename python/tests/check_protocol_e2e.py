"""End-to-end tests for stdin/stdout JSON protocol."""

import json
import subprocess
import sys
import unittest


class TestProtocolE2E(unittest.TestCase):
    """Tests for full JSON-RPC request/response cycle."""

    def _send_request(self, request: dict) -> dict:
        """Send a single request and parse response."""
        proc = subprocess.Popen(
            [sys.executable, "-m", "mengxi_ai"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        stdout, stderr = proc.communicate(
            input=json.dumps(request) + "\n", timeout=10
        )
        return json.loads(stdout.strip())

    def test_ping_request_response(self):
        """Verify ping method returns ok status."""
        response = self._send_request({
            "request_id": "test-001",
            "method": "ping",
            "params": {},
        })
        self.assertEqual(response["status"], "ok")
        self.assertEqual(response["result"]["status"], "ok")

    def test_invalid_method_returns_error(self):
        """Verify unknown method returns error response."""
        response = self._send_request({
            "request_id": "test-002",
            "method": "unknown_method",
            "params": {},
        })
        self.assertEqual(response["status"], "error")
        self.assertEqual(response["error"]["code"], "UNKNOWN_METHOD")

    def test_malformed_json_returns_error(self):
        """Verify invalid JSON returns protocol error."""
        proc = subprocess.Popen(
            [sys.executable, "-m", "mengxi_ai"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        stdout, _ = proc.communicate(input="not json\n", timeout=10)
        response = json.loads(stdout.strip())
        self.assertEqual(response["status"], "error")
        self.assertEqual(response["error"]["code"], "PROTOCOL_ERROR")

    def test_missing_image_path_returns_invalid_params(self):
        """Verify missing image_path returns INVALID_PARAMS."""
        response = self._send_request({
            "request_id": "test-003",
            "method": "generate_embedding",
            "params": {},
        })
        # Handler returns error dict inside result envelope
        self.assertEqual(response["status"], "ok")
        self.assertEqual(response["result"]["code"], "INVALID_PARAMS")

    def test_generate_tags_missing_image_path(self):
        """Verify generate_tags without image_path returns error."""
        response = self._send_request({
            "request_id": "test-004",
            "method": "generate_tags",
            "params": {},
        })
        # Handler returns error dict inside result envelope
        self.assertEqual(response["status"], "ok")
        self.assertEqual(response["result"]["code"], "INVALID_PARAMS")


if __name__ == "__main__":
    unittest.main()
