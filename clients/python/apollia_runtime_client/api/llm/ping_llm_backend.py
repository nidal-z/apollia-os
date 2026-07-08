from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.ping_request import PingRequest
from ...models.ping_response import PingResponse
from ...types import Response


def _get_kwargs(
    *,
    body: PingRequest,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/llm/ping",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> PingResponse | None:
    if response.status_code == 200:
        response_200 = PingResponse.from_dict(response.json())

        return response_200

    if response.status_code == 503:
        response_503 = PingResponse.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[PingResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: PingRequest,
) -> Response[PingResponse]:
    r"""Handler for `POST /api/v1/llm/ping`.

     Sends a trivial completion request (`\"ping\"`) to the specified backend (or the
    router default) and measures the round-trip latency.

    Returns `503 Service Unavailable` with `available: false` when:
    - No `LlmRouter` is configured, or
    - The backend call fails (key missing, network error, etc.).

    Args:
        body (PingRequest): Request body for `POST /api/v1/llm/ping`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[PingResponse]
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
    body: PingRequest,
) -> PingResponse | None:
    r"""Handler for `POST /api/v1/llm/ping`.

     Sends a trivial completion request (`\"ping\"`) to the specified backend (or the
    router default) and measures the round-trip latency.

    Returns `503 Service Unavailable` with `available: false` when:
    - No `LlmRouter` is configured, or
    - The backend call fails (key missing, network error, etc.).

    Args:
        body (PingRequest): Request body for `POST /api/v1/llm/ping`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        PingResponse
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: PingRequest,
) -> Response[PingResponse]:
    r"""Handler for `POST /api/v1/llm/ping`.

     Sends a trivial completion request (`\"ping\"`) to the specified backend (or the
    router default) and measures the round-trip latency.

    Returns `503 Service Unavailable` with `available: false` when:
    - No `LlmRouter` is configured, or
    - The backend call fails (key missing, network error, etc.).

    Args:
        body (PingRequest): Request body for `POST /api/v1/llm/ping`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[PingResponse]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: PingRequest,
) -> PingResponse | None:
    r"""Handler for `POST /api/v1/llm/ping`.

     Sends a trivial completion request (`\"ping\"`) to the specified backend (or the
    router default) and measures the round-trip latency.

    Returns `503 Service Unavailable` with `available: false` when:
    - No `LlmRouter` is configured, or
    - The backend call fails (key missing, network error, etc.).

    Args:
        body (PingRequest): Request body for `POST /api/v1/llm/ping`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        PingResponse
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
