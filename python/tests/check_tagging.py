# tests/test_tagging.py — Unit tests for tagging module

import os
import sys
import tempfile
import unittest

import numpy as np
import unittest.mock as mock
from PIL import Image

# Add parent directory to path for imports
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from mengxi_ai.tagging import (
    CANDIDATE_TAGS,
    CLIP_IMAGE_SIZE,
    CLIP_MEAN,
    CLIP_STD,
    _discover_text_encoder,
    _preprocess_clip_image,
    _softmax,
)
from mengxi_ai.tokenizer import (
    CLIP_CONTEXT_LENGTH,
    CLIP_EOS_TOKEN_ID,
    CLIP_PAD_TOKEN_ID,
    CLIP_SOS_TOKEN_ID,
    ClipTokenizer,
    _bytes_to_unicode,
    _get_pairs,
)


class TestCandidateTags(unittest.TestCase):
    """Tests for the candidate tag vocabulary."""

    def test_candidate_tags_not_empty(self):
        self.assertGreater(len(CANDIDATE_TAGS), 30)

    def test_candidate_tags_all_strings(self):
        for tag in CANDIDATE_TAGS:
            self.assertIsInstance(tag, str)

    def test_candidate_tags_no_empty_strings(self):
        for tag in CANDIDATE_TAGS:
            self.assertTrue(tag.strip(), f"Empty tag found: '{tag}'")

    def test_candidate_tags_no_duplicates(self):
        self.assertEqual(len(CANDIDATE_TAGS), len(set(CANDIDATE_TAGS)))

    def test_candidate_tags_all_lowercase(self):
        for tag in CANDIDATE_TAGS:
            self.assertEqual(tag, tag.lower(), f"Non-lowercase tag: '{tag}'")

    def test_custom_candidate_tags_merge_dedup(self):
        """Custom tags merged with defaults, deduplicated, custom first."""
        # Simulate the merge logic from generate_tags
        candidate_tags = ["cool blue shadows", "warm"]
        seen = set(candidate_tags)
        merged = list(candidate_tags)
        for t in CANDIDATE_TAGS:
            if t not in seen:
                merged.append(t)
                seen.add(t)
        # Custom tags should be at the front
        self.assertEqual(merged[0], "cool blue shadows")
        self.assertEqual(merged[1], "warm")
        # No duplicates
        self.assertEqual(len(merged), len(set(merged)))
        # Should have more tags than custom alone
        self.assertGreater(len(merged), len(candidate_tags))

    def test_custom_candidate_tags_empty_list(self):
        """Empty custom tag list still includes defaults."""
        candidate_tags = []
        seen = set(candidate_tags)
        merged = list(candidate_tags)
        for t in CANDIDATE_TAGS:
            if t not in seen:
                merged.append(t)
                seen.add(t)
        self.assertEqual(len(merged), len(CANDIDATE_TAGS))


class TestPreprocessClipImage(unittest.TestCase):
    """Tests for _preprocess_clip_image."""

    def setUp(self):
        self.temp_dir = tempfile.mkdtemp()

    def _create_test_image(self, size=(64, 64), color=(128, 64, 32)):
        path = os.path.join(self.temp_dir, "test.png")
        img = Image.new("RGB", size, color)
        img.save(path)
        return path

    def test_output_shape_is_batch_channel_height_width(self):
        path = self._create_test_image()
        result = _preprocess_clip_image(path)
        self.assertEqual(result.shape, (1, 3, 224, 224))

    def test_output_dtype_float32(self):
        path = self._create_test_image()
        result = _preprocess_clip_image(path)
        self.assertEqual(result.dtype, np.float32)

    def test_output_has_batch_dimension(self):
        path = self._create_test_image()
        result = _preprocess_clip_image(path)
        self.assertEqual(result.ndim, 4)

    def test_output_channel_order_chw(self):
        """Verify HWC to CHW conversion: output shape should be (1, 3, H, W)."""
        path = self._create_test_image()
        result = _preprocess_clip_image(path)
        # Channel dimension should be index 1
        self.assertEqual(result.shape[1], 3)
        # Height at index 2, width at index 3
        self.assertEqual(result.shape[2], CLIP_IMAGE_SIZE[0])
        self.assertEqual(result.shape[3], CLIP_IMAGE_SIZE[1])

    def test_grayscale_converted_to_rgb(self):
        path = os.path.join(self.temp_dir, "gray.png")
        img = Image.new("L", (64, 64), 128)
        img.save(path)
        result = _preprocess_clip_image(path)
        self.assertEqual(result.shape[1], 3)

    def test_clip_normalization_applied(self):
        """Verify that CLIP mean/std normalization is applied."""
        path = self._create_test_image(color=(255, 255, 255))
        result = _preprocess_clip_image(path)
        # White image after normalization should not be 1.0
        # It should be (1.0 - mean) / std
        channel_0_expected = (1.0 - CLIP_MEAN[0]) / CLIP_STD[0]
        self.assertAlmostEqual(result[0, 0, 0, 0], channel_0_expected, places=4)

    def test_file_not_found_raises(self):
        with self.assertRaises(FileNotFoundError):
            _preprocess_clip_image("/nonexistent/image.png")


class TestSoftmax(unittest.TestCase):
    """Tests for _softmax utility."""

    def test_output_sums_to_one(self):
        x = np.array([1.0, 2.0, 3.0])
        result = _softmax(x)
        self.assertAlmostEqual(result.sum(), 1.0, places=6)

    def test_all_equal_inputs(self):
        """All equal inputs should produce uniform distribution."""
        x = np.array([5.0, 5.0, 5.0, 5.0])
        result = _softmax(x)
        expected = 0.25
        for val in result:
            self.assertAlmostEqual(val, expected, places=6)

    def test_single_element(self):
        x = np.array([0.0])
        result = _softmax(x)
        self.assertAlmostEqual(result[0], 1.0, places=6)

    def test_negative_values(self):
        """Softmax should handle negative values correctly."""
        x = np.array([-1.0, -2.0, -3.0])
        result = _softmax(x)
        self.assertAlmostEqual(result.sum(), 1.0, places=6)
        # First element should be largest
        self.assertGreater(result[0], result[1])
        self.assertGreater(result[1], result[2])

    def test_large_values_numerical_stability(self):
        """Large values should not cause overflow."""
        x = np.array([1000.0, 999.0, 998.0])
        result = _softmax(x)
        self.assertAlmostEqual(result.sum(), 1.0, places=6)
        self.assertFalse(np.any(np.isnan(result)))

    def test_output_shape_matches_input(self):
        x = np.array([1.0, 2.0, 3.0, 4.0, 5.0])
        result = _softmax(x)
        self.assertEqual(result.shape, x.shape)


class TestClipConstants(unittest.TestCase):
    """Tests for CLIP normalization constants."""

    def test_clip_mean_shape(self):
        self.assertEqual(CLIP_MEAN.shape, (3,))

    def test_clip_std_shape(self):
        self.assertEqual(CLIP_STD.shape, (3,))

    def test_clip_std_all_positive(self):
        self.assertTrue(np.all(CLIP_STD > 0))

    def test_clip_mean_in_valid_range(self):
        """CLIP mean values should be in [0, 1] range."""
        self.assertTrue(np.all(CLIP_MEAN >= 0))
        self.assertTrue(np.all(CLIP_MEAN <= 1))

    def test_clip_image_size(self):
        self.assertEqual(CLIP_IMAGE_SIZE, (224, 224))


class TestBytesToUnicode(unittest.TestCase):
    """Tests for the GPT-2/CLIP byte-to-unicode mapping."""

    def test_returns_dict(self):
        result = _bytes_to_unicode()
        self.assertIsInstance(result, dict)

    def test_maps_all_256_bytes(self):
        result = _bytes_to_unicode()
        self.assertEqual(len(result), 256)

    def test_printable_ascii_preserved(self):
        result = _bytes_to_unicode()
        # 'a' (97) should map to 'a'
        self.assertEqual(result[97], "a")
        # '!' (33) should map to '!'
        self.assertEqual(result[33], "!")

    def test_inverse_is_bijective(self):
        result = _bytes_to_unicode()
        # Each value should be unique (bijective mapping)
        self.assertEqual(len(set(result.values())), len(result))


class TestGetPairs(unittest.TestCase):
    """Tests for _get_pairs helper."""

    def test_single_element(self):
        pairs = _get_pairs(("a",))
        self.assertEqual(pairs, set())

    def test_two_elements(self):
        pairs = _get_pairs(("a", "b"))
        self.assertEqual(pairs, {("a", "b")})

    def test_three_elements(self):
        pairs = _get_pairs(("a", "b", "c"))
        self.assertEqual(pairs, {("a", "b"), ("b", "c")})

    def test_repeated_elements(self):
        pairs = _get_pairs(("a", "a", "a"))
        self.assertEqual(pairs, {("a", "a")})


class TestDiscoverTextEncoder(unittest.TestCase):
    """Tests for _discover_text_encoder."""

    def test_no_models_dir(self):
        with self.assertRaises(FileNotFoundError):
            _discover_text_encoder("/nonexistent/path")

    def test_no_text_model_found(self):
        with tempfile.TemporaryDirectory() as tmp:
            # Create an image-only model
            open(os.path.join(tmp, "image_encoder.onnx"), "w").close()
            with self.assertRaises(FileNotFoundError) as ctx:
                _discover_text_encoder(tmp)
            self.assertIn("No text encoder model found", str(ctx.exception))

    def test_discovers_named_text_encoder(self):
        with tempfile.TemporaryDirectory() as tmp:
            open(os.path.join(tmp, "image_encoder.onnx"), "w").close()
            open(os.path.join(tmp, "text_encoder.onnx"), "w").close()
            result = _discover_text_encoder(tmp)
            self.assertEqual(result, "text_encoder.onnx")

    def test_discovers_clip_text_encoder(self):
        with tempfile.TemporaryDirectory() as tmp:
            open(os.path.join(tmp, "visual.onnx"), "w").close()
            open(os.path.join(tmp, "clip_text_encoder.onnx"), "w").close()
            result = _discover_text_encoder(tmp)
            self.assertEqual(result, "clip_text_encoder.onnx")

    def test_falls_back_to_any_text_containing_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            open(os.path.join(tmp, "my_text_model_v2.onnx"), "w").close()
            result = _discover_text_encoder(tmp)
            self.assertEqual(result, "my_text_model_v2.onnx")


class TestClipTokenizerWithoutVocab(unittest.TestCase):
    """Tests for ClipTokenizer when vocab files are missing."""

    def test_raises_file_not_found_without_vocab(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(FileNotFoundError) as ctx:
                ClipTokenizer(tmp)
            self.assertIn("tokenizer files not found", str(ctx.exception))

    def test_raises_file_not_found_with_only_vocab(self):
        with tempfile.TemporaryDirectory() as tmp:
            open(os.path.join(tmp, "vocab.json"), "w").close()
            with self.assertRaises(FileNotFoundError) as ctx:
                ClipTokenizer(tmp)
            self.assertIn("tokenizer files not found", str(ctx.exception))


class TestClipTokenizerSpecialTokens(unittest.TestCase):
    """Tests for CLIP special token constants."""

    def test_sos_token_value(self):
        self.assertEqual(CLIP_SOS_TOKEN_ID, 49406)

    def test_eos_token_value(self):
        self.assertEqual(CLIP_EOS_TOKEN_ID, 49407)

    def test_pad_token_value(self):
        self.assertEqual(CLIP_PAD_TOKEN_ID, 0)

    def test_context_length(self):
        self.assertEqual(CLIP_CONTEXT_LENGTH, 77)


class TestGenerateTagsWithMocks(unittest.TestCase):
    """Tests for generate_tags using mocks (no real ONNX models)."""

    def setUp(self):
        sys.path.insert(0, os.path.dirname(__file__))
        from conftest import create_temp_models_dir
        self.models_dir = create_temp_models_dir()

    def _make_tagging_mocks(self, num_tags: int, embed_dim: int = 512):
        """Create a side_effect that returns different mocks per call.

        Call 1 → text encoder session (output shape: num_tags × embed_dim).
        Call 2+ → image encoder session (output shape: 1 × embed_dim).
        """
        from conftest import MockOnnxSession
        call_count = [0]

        def _session_side_effect(_model_path):
            call_count[0] += 1
            if call_count[0] == 1:
                # Call 1: registry.load_model() → image encoder
                sess = MockOnnxSession(output_shape=(1, embed_dim), output_dim=embed_dim)
                sess._inputs[0].shape = (1, 3, 224, 224)
            else:
                # Call 2+: _load_text_encoder_session() → text encoder
                sess = MockOnnxSession(output_shape=(num_tags, embed_dim), output_dim=embed_dim)
                sess._inputs[0].shape = (num_tags, 77)
            return sess

        return _session_side_effect

    @mock.patch("onnxruntime.InferenceSession")
    @mock.patch("mengxi_ai.tagging.Image.open")
    def test_generate_tags_returns_list(self, mock_open, mock_session_cls):
        from conftest import mock_image_open as _mock_open
        from mengxi_ai.tagging import generate_tags

        mock_open.side_effect = _mock_open
        mock_session_cls.side_effect = self._make_tagging_mocks(len(CANDIDATE_TAGS))

        result = generate_tags(
            image_path="/fake/image.png",
            models_dir=self.models_dir,
            top_n=5,
        )
        self.assertIsInstance(result, list)
        self.assertLessEqual(len(result), 5)

    @mock.patch("onnxruntime.InferenceSession")
    @mock.patch("mengxi_ai.tagging.Image.open")
    def test_generate_tags_custom_candidate_tags(self, mock_open, mock_session_cls):
        from conftest import mock_image_open as _mock_open
        from mengxi_ai.tagging import generate_tags

        mock_open.side_effect = _mock_open
        mock_session_cls.side_effect = self._make_tagging_mocks(10)

        result = generate_tags(
            image_path="/fake/image.png",
            models_dir=self.models_dir,
            candidate_tags=["custom tag 1", "custom tag 2"],
            top_n=2,
        )
        self.assertIsInstance(result, list)
        self.assertTrue(any("custom" in tag for tag in result))

    @mock.patch("onnxruntime.InferenceSession")
    @mock.patch("mengxi_ai.tagging.Image.open")
    def test_generate_tags_top_n_clamped(self, mock_open, mock_session_cls):
        from conftest import mock_image_open as _mock_open
        from mengxi_ai.tagging import generate_tags

        mock_open.side_effect = _mock_open
        mock_session_cls.side_effect = self._make_tagging_mocks(len(CANDIDATE_TAGS))

        # Request more tags than available
        result = generate_tags(
            image_path="/fake/image.png",
            models_dir=self.models_dir,
            top_n=999,
        )
        self.assertLessEqual(len(result), len(CANDIDATE_TAGS))


if __name__ == "__main__":
    unittest.main()
