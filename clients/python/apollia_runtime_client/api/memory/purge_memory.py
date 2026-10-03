from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.memory_removed import MemoryRemoved
from ...models.purge_memory_body import PurgeMemoryBody
from ...types import Response


def _get_kwargs(
    namespace: str,
    *,
    body: PurgeMemoryBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/memory/namespaces/{namespace}/purge".format(
            namespace=quote(str(namespace), safe=""),
        ),
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | MemoryRemoved | None:
    if response.status_code == 200:
        response_200 = MemoryRemoved.from_dict(response.json())

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
) -> Response[ApiErrorBody | MemoryRemoved]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    namespace: str,
    *,
    client: AuthenticatedClient | Client,
    body: PurgeMemoryBody,
) -> Response[ApiErrorBody | MemoryRemoved]:
    """`POST /api/v1/memory/namespaces/{namespace}/purge`: removes the entries
    older than a number of days.

    Args:
        namespace (str):
        body (PurgeMemoryBody): Request body for `POST
            /api/v1/memory/namespaces/{namespace}/purge`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | MemoryRemoved]
    """

    kwargs = _get_kwargs(
        namespace=namespace,
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    namespace: str,
    *,
    client: AuthenticatedClient | Client,
    body: PurgeMemoryBody,
) -> ApiErrorBody | MemoryRemoved | None:
    """`POST /api/v1/memory/namespaces/{namespace}/purge`: removes the entries
    older than a number of days.

    Args:
        namespace (str):
        body (PurgeMemoryBody): Request body for `POST
            /api/v1/memory/namespaces/{namespace}/purge`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | MemoryRemoved
    """

    return sync_detailed(
        namespace=namespace,
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    namespace: str,
    *,
    client: AuthenticatedClient | Client,
    body: PurgeMemoryBody,
) -> Response[ApiErrorBody | MemoryRemoved]:
    """`POST /api/v1/memory/namespaces/{namespace}/purge`: removes the entries
    older than a number of days.

    Args:
        namespace (str):
        body (PurgeMemoryBody): Request body for `POST
            /api/v1/memory/namespaces/{namespace}/purge`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | MemoryRemoved]
    """

    kwargs = _get_kwargs(
        namespace=namespace,
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    namespace: str,
    *,
    client: AuthenticatedClient | Client,
    body: PurgeMemoryBody,
) -> ApiErrorBody | MemoryRemoved | None:
    """`POST /api/v1/memory/namespaces/{namespace}/purge`: removes the entries
    older than a number of days.

    Args:
        namespace (str):
        body (PurgeMemoryBody): Request body for `POST
            /api/v1/memory/namespaces/{namespace}/purge`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | MemoryRemoved
    """

    return (
        await asyncio_detailed(
            namespace=namespace,
            client=client,
            body=body,
        )
    ).parsed
