from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.mcp_bridge_error_body import McpBridgeErrorBody
from ...models.mcp_bridge_tool_list import McpBridgeToolList
from ...types import Response


def _get_kwargs() -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/mcp/bridge/tools",
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> McpBridgeErrorBody | McpBridgeToolList | None:
    if response.status_code == 200:
        response_200 = McpBridgeToolList.from_dict(response.json())

        return response_200

    if response.status_code == 403:
        response_403 = McpBridgeErrorBody.from_dict(response.json())

        return response_403

    if response.status_code == 500:
        response_500 = McpBridgeErrorBody.from_dict(response.json())

        return response_500

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[McpBridgeErrorBody | McpBridgeToolList]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[McpBridgeErrorBody | McpBridgeToolList]:
    """`GET /api/v1/mcp/bridge/tools`: the exposed items, as MCP tools.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[McpBridgeErrorBody | McpBridgeToolList]
    """

    kwargs = _get_kwargs()

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
) -> McpBridgeErrorBody | McpBridgeToolList | None:
    """`GET /api/v1/mcp/bridge/tools`: the exposed items, as MCP tools.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        McpBridgeErrorBody | McpBridgeToolList
    """

    return sync_detailed(
        client=client,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[McpBridgeErrorBody | McpBridgeToolList]:
    """`GET /api/v1/mcp/bridge/tools`: the exposed items, as MCP tools.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[McpBridgeErrorBody | McpBridgeToolList]
    """

    kwargs = _get_kwargs()

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
) -> McpBridgeErrorBody | McpBridgeToolList | None:
    """`GET /api/v1/mcp/bridge/tools`: the exposed items, as MCP tools.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        McpBridgeErrorBody | McpBridgeToolList
    """

    return (
        await asyncio_detailed(
            client=client,
        )
    ).parsed
