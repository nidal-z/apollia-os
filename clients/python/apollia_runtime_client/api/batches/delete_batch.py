from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.batch_deleted_view import BatchDeletedView
from ...models.batch_error_response import BatchErrorResponse
from ...types import Response


def _get_kwargs(
    id: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "delete",
        "url": "/api/v1/batches/{id}".format(
            id=quote(str(id), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> BatchDeletedView | BatchErrorResponse | None:
    if response.status_code == 200:
        response_200 = BatchDeletedView.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = BatchErrorResponse.from_dict(response.json())

        return response_404

    if response.status_code == 409:
        response_409 = BatchErrorResponse.from_dict(response.json())

        return response_409

    if response.status_code == 503:
        response_503 = BatchErrorResponse.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[BatchDeletedView | BatchErrorResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[BatchDeletedView | BatchErrorResponse]:
    """`DELETE /api/v1/batches/{id}`: removes a batch and its items.

     Refused while the batch is active: `running` or `paused`, or stopped or
    interrupted with an item still running. It is stopped first. The tasks that
    ran its items stay in the history of the agent. Nothing outside the
    runtime's store is touched: a file the batch was exported to, or the
    source its answers were written back into, stays as it is.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[BatchDeletedView | BatchErrorResponse]
    """

    kwargs = _get_kwargs(
        id=id,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    id: str,
    *,
    client: AuthenticatedClient | Client,
) -> BatchDeletedView | BatchErrorResponse | None:
    """`DELETE /api/v1/batches/{id}`: removes a batch and its items.

     Refused while the batch is active: `running` or `paused`, or stopped or
    interrupted with an item still running. It is stopped first. The tasks that
    ran its items stay in the history of the agent. Nothing outside the
    runtime's store is touched: a file the batch was exported to, or the
    source its answers were written back into, stays as it is.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        BatchDeletedView | BatchErrorResponse
    """

    return sync_detailed(
        id=id,
        client=client,
    ).parsed


async def asyncio_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[BatchDeletedView | BatchErrorResponse]:
    """`DELETE /api/v1/batches/{id}`: removes a batch and its items.

     Refused while the batch is active: `running` or `paused`, or stopped or
    interrupted with an item still running. It is stopped first. The tasks that
    ran its items stay in the history of the agent. Nothing outside the
    runtime's store is touched: a file the batch was exported to, or the
    source its answers were written back into, stays as it is.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[BatchDeletedView | BatchErrorResponse]
    """

    kwargs = _get_kwargs(
        id=id,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    id: str,
    *,
    client: AuthenticatedClient | Client,
) -> BatchDeletedView | BatchErrorResponse | None:
    """`DELETE /api/v1/batches/{id}`: removes a batch and its items.

     Refused while the batch is active: `running` or `paused`, or stopped or
    interrupted with an item still running. It is stopped first. The tasks that
    ran its items stay in the history of the agent. Nothing outside the
    runtime's store is touched: a file the batch was exported to, or the
    source its answers were written back into, stays as it is.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        BatchDeletedView | BatchErrorResponse
    """

    return (
        await asyncio_detailed(
            id=id,
            client=client,
        )
    ).parsed
