from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.mcp_bridge_call import McpBridgeCall
from ...models.mcp_bridge_call_result import McpBridgeCallResult
from ...models.mcp_bridge_error_body import McpBridgeErrorBody
from ...types import Response


def _get_kwargs(
    *,
    body: McpBridgeCall,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/mcp/bridge/call",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> McpBridgeCallResult | McpBridgeErrorBody | None:
    if response.status_code == 200:
        response_200 = McpBridgeCallResult.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = McpBridgeErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 403:
        response_403 = McpBridgeErrorBody.from_dict(response.json())

        return response_403

    if response.status_code == 404:
        response_404 = McpBridgeErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 503:
        response_503 = McpBridgeErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[McpBridgeCallResult | McpBridgeErrorBody]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: McpBridgeCall,
) -> Response[McpBridgeCallResult | McpBridgeErrorBody]:
    """`POST /api/v1/mcp/bridge/call`: run an exposed item to its end.

    Args:
        body (McpBridgeCall): Body of `POST /api/v1/mcp/bridge/call`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[McpBridgeCallResult | McpBridgeErrorBody]
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
    body: McpBridgeCall,
) -> McpBridgeCallResult | McpBridgeErrorBody | None:
    """`POST /api/v1/mcp/bridge/call`: run an exposed item to its end.

    Args:
        body (McpBridgeCall): Body of `POST /api/v1/mcp/bridge/call`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        McpBridgeCallResult | McpBridgeErrorBody
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: McpBridgeCall,
) -> Response[McpBridgeCallResult | McpBridgeErrorBody]:
    """`POST /api/v1/mcp/bridge/call`: run an exposed item to its end.

    Args:
        body (McpBridgeCall): Body of `POST /api/v1/mcp/bridge/call`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[McpBridgeCallResult | McpBridgeErrorBody]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: McpBridgeCall,
) -> McpBridgeCallResult | McpBridgeErrorBody | None:
    """`POST /api/v1/mcp/bridge/call`: run an exposed item to its end.

    Args:
        body (McpBridgeCall): Body of `POST /api/v1/mcp/bridge/call`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        McpBridgeCallResult | McpBridgeErrorBody
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
