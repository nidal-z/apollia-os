from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.shutdown_response import ShutdownResponse
from ...types import Response


def _get_kwargs() -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/shutdown",
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ShutdownResponse | None:
    if response.status_code == 200:
        response_200 = ShutdownResponse.from_dict(response.json())

        return response_200

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ShutdownResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[ShutdownResponse]:
    """Handler for `POST /api/v1/shutdown`.

     Emits [`RuntimeEvent::ShutdownRequested`] on the EventBus. The caller
    (typically `apollia-os start`) listens for this event to trigger
    graceful shutdown.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ShutdownResponse]
    """

    kwargs = _get_kwargs()

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
) -> ShutdownResponse | None:
    """Handler for `POST /api/v1/shutdown`.

     Emits [`RuntimeEvent::ShutdownRequested`] on the EventBus. The caller
    (typically `apollia-os start`) listens for this event to trigger
    graceful shutdown.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ShutdownResponse
    """

    return sync_detailed(
        client=client,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[ShutdownResponse]:
    """Handler for `POST /api/v1/shutdown`.

     Emits [`RuntimeEvent::ShutdownRequested`] on the EventBus. The caller
    (typically `apollia-os start`) listens for this event to trigger
    graceful shutdown.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ShutdownResponse]
    """

    kwargs = _get_kwargs()

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
) -> ShutdownResponse | None:
    """Handler for `POST /api/v1/shutdown`.

     Emits [`RuntimeEvent::ShutdownRequested`] on the EventBus. The caller
    (typically `apollia-os start`) listens for this event to trigger
    graceful shutdown.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ShutdownResponse
    """

    return (
        await asyncio_detailed(
            client=client,
        )
    ).parsed
