"""Tests for the multi-modal vision helpers."""

from __future__ import annotations

import base64
from typing import TYPE_CHECKING

import pytest
from apollia.types import (
    ImageContent,
    TextContent,
    image_from_bytes,
    image_from_path,
    image_from_url,
    text,
)

if TYPE_CHECKING:
    from pathlib import Path

# Minimal PNG header (8-byte signature) - enough for ``mimetypes`` and base64.
_PNG_MAGIC = b"\x89PNG\r\n\x1a\n"


# Tests for the text() helper


def test_text_returns_text_block() -> None:
    # GIVEN a plain string
    # WHEN it is wrapped by the text() helper
    block: TextContent = text("hello")

    # THEN the block carries the "text" discriminator and the string
    assert block == {"type": "text", "text": "hello"}


def test_text_empty_string() -> None:
    # GIVEN an empty string
    # WHEN it is wrapped by the text() helper
    # THEN a well-formed block is still produced, not None
    assert text("") == {"type": "text", "text": ""}


# Tests for the image_from_url() helper


def test_image_from_url_structure() -> None:
    # GIVEN an image URL
    # WHEN it is wrapped by the image_from_url() helper
    block: ImageContent = image_from_url("https://example.com/cat.png")

    # THEN the block is an image whose source is the URL, undownloaded
    assert block["type"] == "image"
    assert block["source"]["type"] == "url"
    # mypy narrowing: source is an ImageSourceUrl here.
    src = block["source"]
    assert src["type"] == "url"
    if src["type"] == "url":
        assert src["url"] == "https://example.com/cat.png"


# Tests for the image_from_bytes() helper


def test_image_from_bytes_base64_encodes() -> None:
    # GIVEN raw PNG bytes and their MIME type
    raw = _PNG_MAGIC

    # WHEN they are wrapped by the image_from_bytes() helper
    block: ImageContent = image_from_bytes(raw, "image/png")

    # THEN the payload is base64 of the exact input bytes
    assert block["type"] == "image"
    src = block["source"]
    assert src["type"] == "base64"
    if src["type"] == "base64":
        assert src["media_type"] == "image/png"
        decoded = base64.b64decode(src["data"].encode("ascii"))
        assert decoded == raw


def test_image_from_bytes_rejects_non_image_mime() -> None:
    # GIVEN bytes declared with a MIME type outside the image family
    # WHEN they are wrapped by the image_from_bytes() helper
    # THEN the helper refuses rather than building a block no model can read
    with pytest.raises(ValueError, match="Invalid image MIME type"):
        image_from_bytes(b"hello", "text/plain")


def test_image_from_bytes_accepts_various_image_mimes() -> None:
    # GIVEN each image MIME type the helper is meant to accept
    for mime in ("image/jpeg", "image/png", "image/webp", "image/gif"):
        # WHEN bytes are wrapped with that MIME type
        block = image_from_bytes(b"x", mime)

        # THEN it is accepted and carried through unchanged
        src = block["source"]
        if src["type"] == "base64":
            assert src["media_type"] == mime


# Tests for the image_from_path() helper


def test_image_from_path_encodes_file(tmp_path: Path) -> None:
    # GIVEN a PNG file on disk
    file_path = tmp_path / "tiny.png"
    file_path.write_bytes(_PNG_MAGIC)

    # WHEN it is wrapped by the image_from_path() helper
    block: ImageContent = image_from_path(str(file_path))

    # THEN the MIME type is inferred from the extension and the bytes are carried
    assert block["type"] == "image"
    src = block["source"]
    assert src["type"] == "base64"
    if src["type"] == "base64":
        assert src["media_type"] == "image/png"
        assert base64.b64decode(src["data"].encode("ascii")) == _PNG_MAGIC


def test_image_from_path_rejects_unknown_extension(tmp_path: Path) -> None:
    # GIVEN a file whose extension maps to no known MIME type
    file_path = tmp_path / "data.unknownext"
    file_path.write_bytes(b"whatever")

    # WHEN it is wrapped by the image_from_path() helper
    # THEN the helper refuses rather than guessing a MIME type
    with pytest.raises(ValueError, match="Cannot determine image MIME type"):
        image_from_path(str(file_path))


def test_image_from_path_rejects_non_image(tmp_path: Path) -> None:
    # GIVEN a text file on disk
    file_path = tmp_path / "notes.txt"
    file_path.write_text("hello")

    # WHEN it is wrapped by the image_from_path() helper
    # THEN the helper refuses, because text/plain is not an image
    with pytest.raises(ValueError, match="Cannot determine image MIME type"):
        image_from_path(str(file_path))


# ──────────────────────────────────────────────────────────────────────
# vision content passes through MockLlmProxy end-to-end
# ──────────────────────────────────────────────────────────────────────


@pytest.mark.asyncio
async def test_mock_llm_proxy_accepts_multimodal_messages() -> None:
    """An agent that sends an ImageContent block through ``ctx.llm.complete``
    must reach the proxy unchanged - the mock records the full message list,
    including the typed image block, so the agent author can assert the wiring.
    """
    from apollia.testing.mocks import MockLlmProxy

    # GIVEN a mock proxy with one queued answer and a mixed text plus image message
    llm = MockLlmProxy(responses=[{"content": "I see a sunset"}])
    msg = {
        "role": "user",
        "content": [
            text("describe the image"),
            image_from_url("https://example.com/sunset.png"),
        ],
    }

    # WHEN the agent completes on that message
    response = await llm.complete([msg])

    # THEN the image block reached the proxy intact and the answer came back
    # The mock records the prompt verbatim - the image dict survives.
    assert llm.call_count == 1
    recorded = llm.prompts[0]
    assert isinstance(recorded, list)
    assert recorded[0]["role"] == "user"
    content = recorded[0]["content"]
    assert isinstance(content, list)
    assert content[0] == {"type": "text", "text": "describe the image"}
    assert content[1]["type"] == "image"
    assert content[1]["source"]["type"] == "url"
    # And the queued response made it back.
    assert response.content == "I see a sunset"
