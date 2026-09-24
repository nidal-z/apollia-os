from http import HTTPStatus
from typing import Any, cast

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    search: str | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["search"] = search

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/mcp/registry",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Any | ApiErrorBody | None:
    if response.status_code == 200:
        response_200 = cast(Any, None)
        return response_200

    if response.status_code == 502:
        response_502 = ApiErrorBody.from_dict(response.json())

        return response_502

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
    search: str | Unset = UNSET,
) -> Response[Any | ApiErrorBody]:
    """`GET /api/v1/mcp/registry`: the servers the MCP registry lists, the latest
    version of each, in the registry's own shape.

    Args:
        search (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        search=search,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    search: str | Unset = UNSET,
) -> Any | ApiErrorBody | None:
    """`GET /api/v1/mcp/registry`: the servers the MCP registry lists, the latest
    version of each, in the registry's own shape.

    Args:
        search (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return sync_detailed(
        client=client,
        search=search,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    search: str | Unset = UNSET,
) -> Response[Any | ApiErrorBody]:
    """`GET /api/v1/mcp/registry`: the servers the MCP registry lists, the latest
    version of each, in the registry's own shape.

    Args:
        search (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        search=search,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    search: str | Unset = UNSET,
) -> Any | ApiErrorBody | None:
    """`GET /api/v1/mcp/registry`: the servers the MCP registry lists, the latest
    version of each, in the registry's own shape.

    Args:
        search (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            client=client,
            search=search,
        )
    ).parsed
