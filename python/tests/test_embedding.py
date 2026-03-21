# tests/test_embedding.py — Unit tests for embedding module

import os
import sys
import tempfile
import unittest

import numpy as np
from PIL import Image

# Add parent directory to path for imports
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from mengxi_ai.embedding import _preprocess_image, DEFAULT_IMAGE_SIZE


class TestPreprocessImage(unittest.TestCase):
    """Tests for _preprocess_image."""

    def setUp(self):
        self.temp_dir = tempfile.mkdtemp()

    def _create_test_image(self, size=(64, 64), color=(128, 64, 32)):
        path = os.path.join(self.temp_dir, "test.png")
        img = Image.new("RGB", size, color)
        img.save(path)
        return path

    def test_output_shape_batch_channel_height_width(self):
        path = self._create_test_image()
        result = _preprocess_image(path, DEFAULT_IMAGE_SIZE)
        self.assertEqual(result.shape, (1, 3, 224, 224))

    def test_output_dtype_float32(self):
        path = self._create_test_image()
        result = _preprocess_image(path, DEFAULT_IMAGE_SIZE)
        self.assertEqual(result.dtype, np.float32)

    def test_output_range_zero_to_one(self):
        path = self._create_test_image()
        result = _preprocess_image(path, DEFAULT_IMAGE_SIZE)
        self.assertGreaterEqual(result.min(), 0.0)
        self.assertLessEqual(result.max(), 1.0)

    def test_custom_target_size(self):
        path = self._create_test_image()
        result = _preprocess_image(path, (128, 128))
        self.assertEqual(result.shape, (1, 3, 128, 128))

    def test_grayscale_image_converted_to_rgb(self):
        path = os.path.join(self.temp_dir, "gray.png")
        img = Image.new("L", (64, 64), 128)
        img.save(path)
        result = _preprocess_image(path, DEFAULT_IMAGE_SIZE)
        self.assertEqual(result.shape[1], 3)  # Still 3 channels

    def test_uniform_color_produces_expected_values(self):
        # White image: all pixels = (255, 255, 255) -> normalized = 1.0
        path = self._create_test_image(color=(255, 255, 255))
        result = _preprocess_image(path, (1, 1))
        # With a 1x1 white image, resized to 1x1 should still be white
        self.assertAlmostEqual(result[0, 0, 0, 0], 1.0, places=5)
        self.assertAlmostEqual(result[0, 1, 0, 0], 1.0, places=5)
        self.assertAlmostEqual(result[0, 2, 0, 0], 1.0, places=5)

    def test_black_image_produces_zeros(self):
        path = self._create_test_image(color=(0, 0, 0))
        result = _preprocess_image(path, (1, 1))
        self.assertAlmostEqual(result.max(), 0.0, places=5)

    def test_file_not_found_raises(self):
        with self.assertRaises(FileNotFoundError):
            _preprocess_image("/nonexistent/image.png", DEFAULT_IMAGE_SIZE)


if __name__ == "__main__":
    unittest.main()
