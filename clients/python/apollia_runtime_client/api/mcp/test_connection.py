from http import HTTPStatus
from typing import Any, cast

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...types import Response


def _get_kwargs() -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/mcp/servers/test",
    }

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
) -> Response[Any | ApiErrorBody]:
    r"""`POST /api/v1/mcp/servers/test`, Test a server configuration without persisting a session.

     Spawns an ephemeral process, performs the MCP initialize handshake and `tools/list`,
    captures the result, then immediately terminates the process. No session is stored
    and the tool registry is not modified.

    Response shape:
    - `200 {\"kind\":\"success\", …}`, handshake succeeded, tools listed.
    - `200 {\"kind\":\"oauth_required\", \"www_authenticate\":\"…\"}`, server returned
      401, the desktop should drive the MCP HTTP OAuth flow before retrying.
    - `400 Bad Request`, other handshake failures (transport, JSON-RPC error,
      timeout, etc.).

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs()

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
) -> Any | ApiErrorBody | None:
    r"""`POST /api/v1/mcp/servers/test`, Test a server configuration without persisting a session.

     Spawns an ephemeral process, performs the MCP initialize handshake and `tools/list`,
    captures the result, then immediately terminates the process. No session is stored
    and the tool registry is not modified.

    Response shape:
    - `200 {\"kind\":\"success\", …}`, handshake succeeded, tools listed.
    - `200 {\"kind\":\"oauth_required\", \"www_authenticate\":\"…\"}`, server returned
      401, the desktop should drive the MCP HTTP OAuth flow before retrying.
    - `400 Bad Request`, other handshake failures (transport, JSON-RPC error,
      timeout, etc.).

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return sync_detailed(
        client=client,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[Any | ApiErrorBody]:
    r"""`POST /api/v1/mcp/servers/test`, Test a server configuration without persisting a session.

     Spawns an ephemeral process, performs the MCP initialize handshake and `tools/list`,
    captures the result, then immediately terminates the process. No session is stored
    and the tool registry is not modified.

    Response shape:
    - `200 {\"kind\":\"success\", …}`, handshake succeeded, tools listed.
    - `200 {\"kind\":\"oauth_required\", \"www_authenticate\":\"…\"}`, server returned
      401, the desktop should drive the MCP HTTP OAuth flow before retrying.
    - `400 Bad Request`, other handshake failures (transport, JSON-RPC error,
      timeout, etc.).

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs()

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
) -> Any | ApiErrorBody | None:
    r"""`POST /api/v1/mcp/servers/test`, Test a server configuration without persisting a session.

     Spawns an ephemeral process, performs the MCP initialize handshake and `tools/list`,
    captures the result, then immediately terminates the process. No session is stored
    and the tool registry is not modified.

    Response shape:
    - `200 {\"kind\":\"success\", …}`, handshake succeeded, tools listed.
    - `200 {\"kind\":\"oauth_required\", \"www_authenticate\":\"…\"}`, server returned
      401, the desktop should drive the MCP HTTP OAuth flow before retrying.
    - `400 Bad Request`, other handshake failures (transport, JSON-RPC error,
      timeout, etc.).

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            client=client,
        )
    ).parsed
