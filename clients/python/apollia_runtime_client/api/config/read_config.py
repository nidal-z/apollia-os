from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.config_error_body import ConfigErrorBody
from ...models.config_view import ConfigView
from ...types import Response


def _get_kwargs() -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/config",
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ConfigErrorBody | ConfigView | None:
    if response.status_code == 200:
        response_200 = ConfigView.from_dict(response.json())

        return response_200

    if response.status_code == 500:
        response_500 = ConfigErrorBody.from_dict(response.json())

        return response_500

    if response.status_code == 503:
        response_503 = ConfigErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ConfigErrorBody | ConfigView]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[ConfigErrorBody | ConfigView]:
    """`GET /api/v1/config`: the text of `apollia.toml`, its fingerprint, its
    faults, the keys the policy fixes and the keys waiting for a restart.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ConfigErrorBody | ConfigView]
    """

    kwargs = _get_kwargs()

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
) -> ConfigErrorBody | ConfigView | None:
    """`GET /api/v1/config`: the text of `apollia.toml`, its fingerprint, its
    faults, the keys the policy fixes and the keys waiting for a restart.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ConfigErrorBody | ConfigView
    """

    return sync_detailed(
        client=client,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[ConfigErrorBody | ConfigView]:
    """`GET /api/v1/config`: the text of `apollia.toml`, its fingerprint, its
    faults, the keys the policy fixes and the keys waiting for a restart.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ConfigErrorBody | ConfigView]
    """

    kwargs = _get_kwargs()

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
) -> ConfigErrorBody | ConfigView | None:
    """`GET /api/v1/config`: the text of `apollia.toml`, its fingerprint, its
    faults, the keys the policy fixes and the keys waiting for a restart.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ConfigErrorBody | ConfigView
    """

    return (
        await asyncio_detailed(
            client=client,
        )
    ).parsed
