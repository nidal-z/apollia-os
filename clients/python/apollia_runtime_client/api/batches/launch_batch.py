from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.batch_error_response import BatchErrorResponse
from ...models.batch_launch import BatchLaunch
from ...models.batch_view import BatchView
from ...types import Response


def _get_kwargs(
    *,
    body: BatchLaunch,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/batches",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> BatchErrorResponse | BatchView | None:
    if response.status_code == 202:
        response_202 = BatchView.from_dict(response.json())

        return response_202

    if response.status_code == 400:
        response_400 = BatchErrorResponse.from_dict(response.json())

        return response_400

    if response.status_code == 422:
        response_422 = BatchErrorResponse.from_dict(response.json())

        return response_422

    if response.status_code == 503:
        response_503 = BatchErrorResponse.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[BatchErrorResponse | BatchView]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: BatchLaunch,
) -> Response[BatchErrorResponse | BatchView]:
    """`POST /api/v1/batches`: launch an agent on every item of a source.

     The items are read and recorded before the answer; they run in the
    background, at most `concurrency` at once (1 by default, 16 at most).
    Once more than `maxFailures` items have failed (10 by default), the
    batch is interrupted: no further item starts.

    Args:
        body (BatchLaunch): What a batch is launched with.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[BatchErrorResponse | BatchView]
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
    body: BatchLaunch,
) -> BatchErrorResponse | BatchView | None:
    """`POST /api/v1/batches`: launch an agent on every item of a source.

     The items are read and recorded before the answer; they run in the
    background, at most `concurrency` at once (1 by default, 16 at most).
    Once more than `maxFailures` items have failed (10 by default), the
    batch is interrupted: no further item starts.

    Args:
        body (BatchLaunch): What a batch is launched with.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        BatchErrorResponse | BatchView
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: BatchLaunch,
) -> Response[BatchErrorResponse | BatchView]:
    """`POST /api/v1/batches`: launch an agent on every item of a source.

     The items are read and recorded before the answer; they run in the
    background, at most `concurrency` at once (1 by default, 16 at most).
    Once more than `maxFailures` items have failed (10 by default), the
    batch is interrupted: no further item starts.

    Args:
        body (BatchLaunch): What a batch is launched with.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[BatchErrorResponse | BatchView]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: BatchLaunch,
) -> BatchErrorResponse | BatchView | None:
    """`POST /api/v1/batches`: launch an agent on every item of a source.

     The items are read and recorded before the answer; they run in the
    background, at most `concurrency` at once (1 by default, 16 at most).
    Once more than `maxFailures` items have failed (10 by default), the
    batch is interrupted: no further item starts.

    Args:
        body (BatchLaunch): What a batch is launched with.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        BatchErrorResponse | BatchView
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
