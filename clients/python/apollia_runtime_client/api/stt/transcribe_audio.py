from http import HTTPStatus
from typing import Any, cast

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.transcribe_audio_body import TranscribeAudioBody
from ...types import Response


def _get_kwargs(
    *,
    body: TranscribeAudioBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/stt/transcribe",
    }

    _kwargs["files"] = body.to_multipart()

    headers["Content-Type"] = "multipart/form-data; boundary=+++"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Any | ApiErrorBody | None:
    if response.status_code == 200:
        response_200 = cast(Any, None)
        return response_200

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[Any | ApiErrorBody]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: TranscribeAudioBody,
) -> Response[Any | ApiErrorBody]:
    """`POST /api/v1/stt/transcribe`, transcribe an uploaded audio file.

     Accepts `multipart/form-data` with:
    - `audio` (required): WAV audio file
    - `language` (optional): language hint (ISO 639-1 code)

    Returns `200 OK` with the persisted transcript row (`TranscriptRow`).
    Returns `400 Bad Request` on missing or invalid audio.
    Returns `503 Service Unavailable` when the engine is absent.

    Args:
        body (TranscribeAudioBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    body: TranscribeAudioBody,
) -> Any | ApiErrorBody | None:
    """`POST /api/v1/stt/transcribe`, transcribe an uploaded audio file.

     Accepts `multipart/form-data` with:
    - `audio` (required): WAV audio file
    - `language` (optional): language hint (ISO 639-1 code)

    Returns `200 OK` with the persisted transcript row (`TranscriptRow`).
    Returns `400 Bad Request` on missing or invalid audio.
    Returns `503 Service Unavailable` when the engine is absent.

    Args:
        body (TranscribeAudioBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: TranscribeAudioBody,
) -> Response[Any | ApiErrorBody]:
    """`POST /api/v1/stt/transcribe`, transcribe an uploaded audio file.

     Accepts `multipart/form-data` with:
    - `audio` (required): WAV audio file
    - `language` (optional): language hint (ISO 639-1 code)

    Returns `200 OK` with the persisted transcript row (`TranscriptRow`).
    Returns `400 Bad Request` on missing or invalid audio.
    Returns `503 Service Unavailable` when the engine is absent.

    Args:
        body (TranscribeAudioBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: TranscribeAudioBody,
) -> Any | ApiErrorBody | None:
    """`POST /api/v1/stt/transcribe`, transcribe an uploaded audio file.

     Accepts `multipart/form-data` with:
    - `audio` (required): WAV audio file
    - `language` (optional): language hint (ISO 639-1 code)

    Returns `200 OK` with the persisted transcript row (`TranscriptRow`).
    Returns `400 Bad Request` on missing or invalid audio.
    Returns `503 Service Unavailable` when the engine is absent.

    Args:
        body (TranscribeAudioBody):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
