# tokenizer.py — CLIP BPE tokenizer for text encoding

import json
import logging
import os
import re
from typing import Optional

logger = logging.getLogger(__name__)

# CLIP special token IDs (ViT-B/32)
CLIP_SOS_TOKEN_ID = 49406  # Start of sequence
CLIP_EOS_TOKEN_ID = 49407  # End of sequence
CLIP_PAD_TOKEN_ID = 0      # Padding
CLIP_CONTEXT_LENGTH = 77    # Maximum context length


def _bytes_to_unicode() -> dict[int, str]:
    """Returns the GPT-2/CLIP byte-to-unicode mapping.

    Maps byte values 0-255 to printable unicode characters,
    ensuring all bytes have a visible representation for BPE.
    """
    bs = (
        list(range(ord("!"), ord("~") + 1))
        + list(range(ord("\xa1"), ord("\xac") + 1))
        + list(range(ord("\xae"), ord("\xff") + 1))
    )
    cs = bs[:]
    n = 0
    for b in range(256):
        if b not in bs:
            bs.append(b)
            cs.append(256 + n)
            n += 1
    return dict(zip(bs, [chr(c) for c in cs]))


_BYTE_ENCODER = _bytes_to_unicode()
_BYTE_DECODER = {v: k for k, v in _BYTE_ENCODER.items()}

# GPT-2/CLIP pre-tokenization regex pattern
_PAT = re.compile(
    r"""'s|'t|'re|'ve|'m|'ll|'d| ?\w+| ?\d+| ?[^\s\w\d]+|\s+(?!\S)|\s+"""
)


def _get_pairs(word: tuple) -> set:
    """Get all adjacent symbol pairs in a word (tuple of symbols)."""
    pairs = set()
    prev = word[0]
    for char in word[1:]:
        pairs.add((prev, char))
        prev = char
    return pairs


class ClipTokenizer:
    """CLIP BPE tokenizer for encoding text into token ID sequences.

    Requires ``vocab.json`` and ``merges.txt`` in the models directory.
    These files ship with the OpenAI CLIP model weights and are also
    available from HuggingFace ``openai/clip-vit-base-patch32``.
    """

    def __init__(self, models_dir: Optional[str] = None):
        if models_dir is None:
            models_dir = os.path.join(
                os.path.dirname(os.path.dirname(__file__)), "models"
            )

        vocab_path = os.path.join(models_dir, "vocab.json")
        merges_path = os.path.join(models_dir, "merges.txt")

        if not os.path.isfile(vocab_path) or not os.path.isfile(merges_path):
            raise FileNotFoundError(
                f"CLIP tokenizer files not found in {models_dir}. "
                "Need vocab.json and merges.txt from the CLIP model."
            )

        with open(vocab_path, "r", encoding="utf-8") as f:
            self.encoder: dict[str, int] = json.load(f)

        with open(merges_path, "r", encoding="utf-8") as f:
            lines = f.read().split("\n")
            # Skip header line (e.g. "# version: 0.2")
            merges = [
                tuple(line.split())
                for line in lines[1:]
                if line.strip() and len(line.split()) == 2
            ]

        self.bpe_ranks: dict[tuple, int] = dict(zip(merges, range(len(merges))))
        self._cache: dict[str, str] = {}

        logger.info(
            "CLIP tokenizer loaded (vocab=%d, merges=%d)",
            len(self.encoder),
            len(self.bpe_ranks),
        )

    def _bpe(self, token: str) -> str:
        """Apply Byte Pair Encoding to a single pre-tokenized word."""
        if token in self._cache:
            return self._cache[token]

        word = tuple(token)
        pairs = _get_pairs(word)
        if not pairs:
            return token

        while True:
            bigram = min(
                pairs, key=lambda p: self.bpe_ranks.get(p, float("inf"))
            )
            if bigram not in self.bpe_ranks:
                break

            first, second = bigram
            new_word: list[str] = []
            i = 0
            while i < len(word):
                try:
                    j = word.index(first, i)
                except ValueError:
                    new_word.extend(word[i:])
                    break
                new_word.extend(word[i:j])
                i = j

                if i < len(word) - 1 and word[i] == first and word[i + 1] == second:
                    new_word.append(first + second)
                    i += 2
                else:
                    new_word.append(word[i])
                    i += 1

            word = tuple(new_word)
            if len(word) == 1:
                break
            pairs = _get_pairs(word)

        result = " ".join(word)
        self._cache[token] = result
        return result

    def encode(self, text: str) -> list[int]:
        """Encode a single text string into CLIP token IDs.

        Returns a list of exactly ``CLIP_CONTEXT_LENGTH`` (77) int64 IDs:
        SOS token, content tokens, EOS token, then zero-padding.
        """
        tokens: list[int] = [CLIP_SOS_TOKEN_ID]

        for match in _PAT.findall(text.lower()):
            # Byte-encode the matched substring using GPT-2 byte mapping
            encoded = "".join(_BYTE_ENCODER[b] for b in match.encode("utf-8"))
            for bpe_tok in self._bpe(encoded).split(" "):
                if bpe_tok in self.encoder:
                    tokens.append(self.encoder[bpe_tok])
                # Silently skip unknown BPE tokens (shouldn't happen with CLIP vocab)

        tokens.append(CLIP_EOS_TOKEN_ID)

        # Truncate to fit within context window (keep SOS + EOS)
        if len(tokens) > CLIP_CONTEXT_LENGTH:
            tokens = tokens[: CLIP_CONTEXT_LENGTH - 1] + [CLIP_EOS_TOKEN_ID]

        # Zero-pad to context length
        while len(tokens) < CLIP_CONTEXT_LENGTH:
            tokens.append(CLIP_PAD_TOKEN_ID)

        return tokens

    def encode_batch(self, texts: list[str]) -> list[list[int]]:
        """Encode a batch of text strings into CLIP token ID arrays."""
        return [self.encode(t) for t in texts]
