from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.memory_entry_view import MemoryEntryView
from ...types import Response


def _get_kwargs(
    namespace: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/memory/namespaces/{namespace}/entries".format(
            namespace=quote(str(namespace), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | list[MemoryEntryView] | None:
    if response.status_code == 200:
        response_200 = []
        _response_200 = response.json()
        for response_200_item_data in _response_200:
            response_200_item = MemoryEntryView.from_dict(response_200_item_data)

            response_200.append(response_200_item)

        return response_200

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | list[MemoryEntryView]]:
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
) -> Response[ApiErrorBody | list[MemoryEntryView]]:
    """`GET /api/v1/memory/namespaces/{namespace}/entries`: every entry of one
    namespace, newest first.

    Args:
        namespace (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | list[MemoryEntryView]]
    """

    kwargs = _get_kwargs(
        namespace=namespace,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    namespace: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | list[MemoryEntryView] | None:
    """`GET /api/v1/memory/namespaces/{namespace}/entries`: every entry of one
    namespace, newest first.

    Args:
        namespace (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | list[MemoryEntryView]
    """

    return sync_detailed(
        namespace=namespace,
        client=client,
    ).parsed


async def asyncio_detailed(
    namespace: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | list[MemoryEntryView]]:
    """`GET /api/v1/memory/namespaces/{namespace}/entries`: every entry of one
    namespace, newest first.

    Args:
        namespace (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | list[MemoryEntryView]]
    """

    kwargs = _get_kwargs(
        namespace=namespace,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    namespace: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | list[MemoryEntryView] | None:
    """`GET /api/v1/memory/namespaces/{namespace}/entries`: every entry of one
    namespace, newest first.

    Args:
        namespace (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | list[MemoryEntryView]
    """

    return (
        await asyncio_detailed(
            namespace=namespace,
            client=client,
        )
    ).parsed
