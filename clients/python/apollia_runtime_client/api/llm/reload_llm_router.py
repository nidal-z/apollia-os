from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.reload_router_response import ReloadRouterResponse
from ...types import Response


def _get_kwargs() -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/llm/reload",
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | ReloadRouterResponse | None:
    if response.status_code == 200:
        response_200 = ReloadRouterResponse.from_dict(response.json())

        return response_200

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | ReloadRouterResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | ReloadRouterResponse]:
    """Handler for `POST /api/v1/llm/reload`.

     Rebuilds the active `LlmRouter` from `system.db` and swaps it into the
    shared cell exposed by [`AppState::llm_router`], without restarting the
    daemon. The new router becomes visible to every subsequent reader
    (`ping`, `chat`, `complete`, `status`); in-flight requests that already
    hold a snapshot of the previous router finish against the old router and
    are not interrupted.

    The route also forwards the freshly-built router to the
    [`ChatSessionManager`] via its `ReloadLlm` actor command so live chat
    sessions pick up the new model on their next turn.

    Returns:
    - `200 OK` with the list of backends now active.
    - `503 Service Unavailable` when `llm_backend_repo` is `None` (the runtime
      was started without `system.db`, typically a unit test).
    - `500 Internal Server Error` when the repository is reachable but
      building the router fails (invalid config_json, model file missing for
      a local backend, etc.).

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ReloadRouterResponse]
    """

    kwargs = _get_kwargs()

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | ReloadRouterResponse | None:
    """Handler for `POST /api/v1/llm/reload`.

     Rebuilds the active `LlmRouter` from `system.db` and swaps it into the
    shared cell exposed by [`AppState::llm_router`], without restarting the
    daemon. The new router becomes visible to every subsequent reader
    (`ping`, `chat`, `complete`, `status`); in-flight requests that already
    hold a snapshot of the previous router finish against the old router and
    are not interrupted.

    The route also forwards the freshly-built router to the
    [`ChatSessionManager`] via its `ReloadLlm` actor command so live chat
    sessions pick up the new model on their next turn.

    Returns:
    - `200 OK` with the list of backends now active.
    - `503 Service Unavailable` when `llm_backend_repo` is `None` (the runtime
      was started without `system.db`, typically a unit test).
    - `500 Internal Server Error` when the repository is reachable but
      building the router fails (invalid config_json, model file missing for
      a local backend, etc.).

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ReloadRouterResponse
    """

    return sync_detailed(
        client=client,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | ReloadRouterResponse]:
    """Handler for `POST /api/v1/llm/reload`.

     Rebuilds the active `LlmRouter` from `system.db` and swaps it into the
    shared cell exposed by [`AppState::llm_router`], without restarting the
    daemon. The new router becomes visible to every subsequent reader
    (`ping`, `chat`, `complete`, `status`); in-flight requests that already
    hold a snapshot of the previous router finish against the old router and
    are not interrupted.

    The route also forwards the freshly-built router to the
    [`ChatSessionManager`] via its `ReloadLlm` actor command so live chat
    sessions pick up the new model on their next turn.

    Returns:
    - `200 OK` with the list of backends now active.
    - `503 Service Unavailable` when `llm_backend_repo` is `None` (the runtime
      was started without `system.db`, typically a unit test).
    - `500 Internal Server Error` when the repository is reachable but
      building the router fails (invalid config_json, model file missing for
      a local backend, etc.).

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ReloadRouterResponse]
    """

    kwargs = _get_kwargs()

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | ReloadRouterResponse | None:
    """Handler for `POST /api/v1/llm/reload`.

     Rebuilds the active `LlmRouter` from `system.db` and swaps it into the
    shared cell exposed by [`AppState::llm_router`], without restarting the
    daemon. The new router becomes visible to every subsequent reader
    (`ping`, `chat`, `complete`, `status`); in-flight requests that already
    hold a snapshot of the previous router finish against the old router and
    are not interrupted.

    The route also forwards the freshly-built router to the
    [`ChatSessionManager`] via its `ReloadLlm` actor command so live chat
    sessions pick up the new model on their next turn.

    Returns:
    - `200 OK` with the list of backends now active.
    - `503 Service Unavailable` when `llm_backend_repo` is `None` (the runtime
      was started without `system.db`, typically a unit test).
    - `500 Internal Server Error` when the repository is reachable but
      building the router fails (invalid config_json, model file missing for
      a local backend, etc.).

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ReloadRouterResponse
    """

    return (
        await asyncio_detailed(
            client=client,
        )
    ).parsed
