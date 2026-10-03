from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.memory_entry_deleted import MemoryEntryDeleted
from ...types import Response


def _get_kwargs(
    namespace: str,
    entry_id: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "delete",
        "url": "/api/v1/memory/namespaces/{namespace}/entries/{entry_id}".format(
            namespace=quote(str(namespace), safe=""),
            entry_id=quote(str(entry_id), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | MemoryEntryDeleted | None:
    if response.status_code == 200:
        response_200 = MemoryEntryDeleted.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | MemoryEntryDeleted]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    namespace: str,
    entry_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | MemoryEntryDeleted]:
    """`DELETE /api/v1/memory/namespaces/{namespace}/entries/{entry_id}`: removes
    one entry, whatever its kind.

    Args:
        namespace (str):
        entry_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | MemoryEntryDeleted]
    """

    kwargs = _get_kwargs(
        namespace=namespace,
        entry_id=entry_id,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    namespace: str,
    entry_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | MemoryEntryDeleted | None:
    """`DELETE /api/v1/memory/namespaces/{namespace}/entries/{entry_id}`: removes
    one entry, whatever its kind.

    Args:
        namespace (str):
        entry_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | MemoryEntryDeleted
    """

    return sync_detailed(
        namespace=namespace,
        entry_id=entry_id,
        client=client,
    ).parsed


async def asyncio_detailed(
    namespace: str,
    entry_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | MemoryEntryDeleted]:
    """`DELETE /api/v1/memory/namespaces/{namespace}/entries/{entry_id}`: removes
    one entry, whatever its kind.

    Args:
        namespace (str):
        entry_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | MemoryEntryDeleted]
    """

    kwargs = _get_kwargs(
        namespace=namespace,
        entry_id=entry_id,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    namespace: str,
    entry_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | MemoryEntryDeleted | None:
    """`DELETE /api/v1/memory/namespaces/{namespace}/entries/{entry_id}`: removes
    one entry, whatever its kind.

    Args:
        namespace (str):
        entry_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | MemoryEntryDeleted
    """

    return (
        await asyncio_detailed(
            namespace=namespace,
            entry_id=entry_id,
            client=client,
        )
    ).parsed
