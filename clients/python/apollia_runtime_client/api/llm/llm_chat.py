from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.chat_request import ChatRequest
from ...models.chat_response import ChatResponse
from ...types import Response


def _get_kwargs(
    *,
    body: ChatRequest,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/llm/chat",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | ChatResponse | None:
    if response.status_code == 200:
        response_200 = ChatResponse.from_dict(response.json())

        return response_200

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | ChatResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: ChatRequest,
) -> Response[ApiErrorBody | ChatResponse]:
    """Handler for `POST /api/v1/llm/chat`.

     Builds a single-turn `CompletionRequest` from the prompt and dispatches it
    via `LlmRouter::complete_with_observability`. Observability events
    (`LlmCallCompleted`) are emitted on the `EventBus` automatically.

    Returns `503 Service Unavailable` if no router is configured or the call fails.

    Args:
        body (ChatRequest): Request body for `POST /api/v1/llm/chat`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ChatResponse]
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
    body: ChatRequest,
) -> ApiErrorBody | ChatResponse | None:
    """Handler for `POST /api/v1/llm/chat`.

     Builds a single-turn `CompletionRequest` from the prompt and dispatches it
    via `LlmRouter::complete_with_observability`. Observability events
    (`LlmCallCompleted`) are emitted on the `EventBus` automatically.

    Returns `503 Service Unavailable` if no router is configured or the call fails.

    Args:
        body (ChatRequest): Request body for `POST /api/v1/llm/chat`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ChatResponse
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: ChatRequest,
) -> Response[ApiErrorBody | ChatResponse]:
    """Handler for `POST /api/v1/llm/chat`.

     Builds a single-turn `CompletionRequest` from the prompt and dispatches it
    via `LlmRouter::complete_with_observability`. Observability events
    (`LlmCallCompleted`) are emitted on the `EventBus` automatically.

    Returns `503 Service Unavailable` if no router is configured or the call fails.

    Args:
        body (ChatRequest): Request body for `POST /api/v1/llm/chat`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ChatResponse]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: ChatRequest,
) -> ApiErrorBody | ChatResponse | None:
    """Handler for `POST /api/v1/llm/chat`.

     Builds a single-turn `CompletionRequest` from the prompt and dispatches it
    via `LlmRouter::complete_with_observability`. Observability events
    (`LlmCallCompleted`) are emitted on the `EventBus` automatically.

    Returns `503 Service Unavailable` if no router is configured or the call fails.

    Args:
        body (ChatRequest): Request body for `POST /api/v1/llm/chat`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ChatResponse
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
