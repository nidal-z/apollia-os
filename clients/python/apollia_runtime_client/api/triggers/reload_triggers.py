from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.reload_response import ReloadResponse
from ...types import Response


def _get_kwargs() -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/triggers/reload",
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | ReloadResponse | None:
    if response.status_code == 200:
        response_200 = ReloadResponse.from_dict(response.json())

        return response_200

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
) -> Response[ApiErrorBody | ReloadResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | ReloadResponse]:
    """Axum handler for `POST /api/v1/triggers/reload`.

     Re-reads definitions from the SQLite repository (no longer TOML),
    converts them to rich types, and reloads the `TriggerEngine`.
    Emits `TriggersReloaded` on the EventBus via the engine.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ReloadResponse]
    """

    kwargs = _get_kwargs()

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | ReloadResponse | None:
    """Axum handler for `POST /api/v1/triggers/reload`.

     Re-reads definitions from the SQLite repository (no longer TOML),
    converts them to rich types, and reloads the `TriggerEngine`.
    Emits `TriggersReloaded` on the EventBus via the engine.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ReloadResponse
    """

    return sync_detailed(
        client=client,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | ReloadResponse]:
    """Axum handler for `POST /api/v1/triggers/reload`.

     Re-reads definitions from the SQLite repository (no longer TOML),
    converts them to rich types, and reloads the `TriggerEngine`.
    Emits `TriggersReloaded` on the EventBus via the engine.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ReloadResponse]
    """

    kwargs = _get_kwargs()

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | ReloadResponse | None:
    """Axum handler for `POST /api/v1/triggers/reload`.

     Re-reads definitions from the SQLite repository (no longer TOML),
    converts them to rich types, and reloads the `TriggerEngine`.
    Emits `TriggersReloaded` on the EventBus via the engine.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ReloadResponse
    """

    return (
        await asyncio_detailed(
            client=client,
        )
    ).parsed
