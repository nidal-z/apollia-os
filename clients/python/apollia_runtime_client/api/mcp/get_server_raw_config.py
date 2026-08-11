from http import HTTPStatus
from typing import Any, cast
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...types import Response


def _get_kwargs(
    name: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/mcp/servers/{name}/raw_config".format(
            name=quote(str(name), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Any | ApiErrorBody | None:
    if response.status_code == 200:
        response_200 = cast("Any", None)
        return response_200

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

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
    name: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[Any | ApiErrorBody]:
    r"""`GET /api/v1/mcp/servers/:name/raw_config`, Return the persisted launch
    configuration of a server (command, args, env, transport, …) as stored in
    `mcp.db`.

     The `env` map contains either literal values for non-secret variables or
    `${APOLLIA_SECRET:NAME}` placeholders for secret ones, actual secret
    material is never returned. Used by the desktop \"Modifier les arguments\"
    flow to fetch the current config, patch `args`, and PUT the result back
    without losing the rest of the configuration.

    Returns `404 Not Found` when no server with `name` is persisted.
    Returns `503 Service Unavailable` when the MCP repository is unavailable.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        name=name,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    name: str,
    *,
    client: AuthenticatedClient | Client,
) -> Any | ApiErrorBody | None:
    r"""`GET /api/v1/mcp/servers/:name/raw_config`, Return the persisted launch
    configuration of a server (command, args, env, transport, …) as stored in
    `mcp.db`.

     The `env` map contains either literal values for non-secret variables or
    `${APOLLIA_SECRET:NAME}` placeholders for secret ones, actual secret
    material is never returned. Used by the desktop \"Modifier les arguments\"
    flow to fetch the current config, patch `args`, and PUT the result back
    without losing the rest of the configuration.

    Returns `404 Not Found` when no server with `name` is persisted.
    Returns `503 Service Unavailable` when the MCP repository is unavailable.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return sync_detailed(
        name=name,
        client=client,
    ).parsed


async def asyncio_detailed(
    name: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[Any | ApiErrorBody]:
    r"""`GET /api/v1/mcp/servers/:name/raw_config`, Return the persisted launch
    configuration of a server (command, args, env, transport, …) as stored in
    `mcp.db`.

     The `env` map contains either literal values for non-secret variables or
    `${APOLLIA_SECRET:NAME}` placeholders for secret ones, actual secret
    material is never returned. Used by the desktop \"Modifier les arguments\"
    flow to fetch the current config, patch `args`, and PUT the result back
    without losing the rest of the configuration.

    Returns `404 Not Found` when no server with `name` is persisted.
    Returns `503 Service Unavailable` when the MCP repository is unavailable.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        name=name,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    name: str,
    *,
    client: AuthenticatedClient | Client,
) -> Any | ApiErrorBody | None:
    r"""`GET /api/v1/mcp/servers/:name/raw_config`, Return the persisted launch
    configuration of a server (command, args, env, transport, …) as stored in
    `mcp.db`.

     The `env` map contains either literal values for non-secret variables or
    `${APOLLIA_SECRET:NAME}` placeholders for secret ones, actual secret
    material is never returned. Used by the desktop \"Modifier les arguments\"
    flow to fetch the current config, patch `args`, and PUT the result back
    without losing the rest of the configuration.

    Returns `404 Not Found` when no server with `name` is persisted.
    Returns `503 Service Unavailable` when the MCP repository is unavailable.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            name=name,
            client=client,
        )
    ).parsed
