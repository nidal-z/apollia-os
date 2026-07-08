from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.sse_task_event import SseTaskEvent
from ...types import Response


def _get_kwargs(
    id: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/sessions/{id}/stream".format(
            id=quote(str(id), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | SseTaskEvent | None:
    if response.status_code == 200:
        response_200 = SseTaskEvent.from_dict(response.text)

        return response_200

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | SseTaskEvent]:
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
) -> Response[ApiErrorBody | SseTaskEvent]:
    """Handler for `GET /api/v1/sessions/:id/stream`, SSE streaming.

     Opens a persistent SSE stream filtering `ChatXxx` [`RuntimeEvent`]s by `session_id`.
    The stream closes when `ChatSessionClosed` is emitted.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | SseTaskEvent]
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
) -> ApiErrorBody | SseTaskEvent | None:
    """Handler for `GET /api/v1/sessions/:id/stream`, SSE streaming.

     Opens a persistent SSE stream filtering `ChatXxx` [`RuntimeEvent`]s by `session_id`.
    The stream closes when `ChatSessionClosed` is emitted.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | SseTaskEvent
    """

    return sync_detailed(
        id=id,
        client=client,
    ).parsed


async def asyncio_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | SseTaskEvent]:
    """Handler for `GET /api/v1/sessions/:id/stream`, SSE streaming.

     Opens a persistent SSE stream filtering `ChatXxx` [`RuntimeEvent`]s by `session_id`.
    The stream closes when `ChatSessionClosed` is emitted.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | SseTaskEvent]
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
) -> ApiErrorBody | SseTaskEvent | None:
    """Handler for `GET /api/v1/sessions/:id/stream`, SSE streaming.

     Opens a persistent SSE stream filtering `ChatXxx` [`RuntimeEvent`]s by `session_id`.
    The stream closes when `ChatSessionClosed` is emitted.

    Args:
        id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | SseTaskEvent
    """

    return (
        await asyncio_detailed(
            id=id,
            client=client,
        )
    ).parsed
