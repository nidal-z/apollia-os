"""Tests for the multi-modal vision helpers (ADR-111)."""

from __future__ import annotations

import base64
from pathlib import Path

import pytest

from apollia.types import (
    ImageContent,
    TextContent,
    image_from_bytes,
    image_from_path,
    image_from_url,
    text,
)


# Minimal PNG header (8-byte signature) — enough for ``mimetypes`` and base64.
_PNG_MAGIC = b"\x89PNG\r\n\x1a\n"


# ──────────────────────────────────────────────────────────────────────
# text()
# ──────────────────────────────────────────────────────────────────────


def test_text_returns_text_block() -> None:
    block: TextContent = text("hello")
    assert block == {"type": "text", "text": "hello"}


def test_text_empty_string() -> None:
    assert text("") == {"type": "text", "text": ""}


# ──────────────────────────────────────────────────────────────────────
# image_from_url()
# ──────────────────────────────────────────────────────────────────────


def test_image_from_url_structure() -> None:
    block: ImageContent = image_from_url("https://example.com/cat.png")
    assert block["type"] == "image"
    assert block["source"]["type"] == "url"
    # mypy narrowing: source is an ImageSourceUrl here.
    src = block["source"]
    assert src["type"] == "url"
    if src["type"] == "url":
        assert src["url"] == "https://example.com/cat.png"


# ──────────────────────────────────────────────────────────────────────
# image_from_bytes()
# ──────────────────────────────────────────────────────────────────────


def test_image_from_bytes_base64_encodes() -> None:
    raw = _PNG_MAGIC
    block: ImageContent = image_from_bytes(raw, "image/png")
    assert block["type"] == "image"
    src = block["source"]
    assert src["type"] == "base64"
    if src["type"] == "base64":
        assert src["media_type"] == "image/png"
        decoded = base64.b64decode(src["data"].encode("ascii"))
        assert decoded == raw


def test_image_from_bytes_rejects_non_image_mime() -> None:
    with pytest.raises(ValueError, match="Invalid image MIME type"):
        image_from_bytes(b"hello", "text/plain")


def test_image_from_bytes_accepts_various_image_mimes() -> None:
    for mime in ("image/jpeg", "image/png", "image/webp", "image/gif"):
        block = image_from_bytes(b"x", mime)
        src = block["source"]
        if src["type"] == "base64":
            assert src["media_type"] == mime


# ──────────────────────────────────────────────────────────────────────
# image_from_path()
# ──────────────────────────────────────────────────────────────────────


def test_image_from_path_encodes_file(tmp_path: Path) -> None:
    file_path = tmp_path / "tiny.png"
    file_path.write_bytes(_PNG_MAGIC)
    block: ImageContent = image_from_path(str(file_path))
    assert block["type"] == "image"
    src = block["source"]
    assert src["type"] == "base64"
    if src["type"] == "base64":
        assert src["media_type"] == "image/png"
        assert base64.b64decode(src["data"].encode("ascii")) == _PNG_MAGIC


def test_image_from_path_rejects_unknown_extension(tmp_path: Path) -> None:
    file_path = tmp_path / "data.unknownext"
    file_path.write_bytes(b"whatever")
    with pytest.raises(ValueError, match="Cannot determine image MIME type"):
        image_from_path(str(file_path))


def test_image_from_path_rejects_non_image(tmp_path: Path) -> None:
    file_path = tmp_path / "notes.txt"
    file_path.write_text("hello")
    with pytest.raises(ValueError, match="Cannot determine image MIME type"):
        image_from_path(str(file_path))
