from http import HTTPStatus
from typing import Any, cast

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    last: int | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["last"] = last

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/notifications/logs",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Any | ApiErrorBody | None:
    if response.status_code == 200:
        response_200 = cast("Any", None)
        return response_200

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

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
    last: int | Unset = UNSET,
) -> Response[Any | ApiErrorBody]:
    """`GET /api/v1/notifications/logs?last=N`, notification history.

     Reads from `notifications.db` via the repository.
    Falls back to `hitl.db` if the repo is not available (backward compat).

    Args:
        last (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        last=last,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    last: int | Unset = UNSET,
) -> Any | ApiErrorBody | None:
    """`GET /api/v1/notifications/logs?last=N`, notification history.

     Reads from `notifications.db` via the repository.
    Falls back to `hitl.db` if the repo is not available (backward compat).

    Args:
        last (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return sync_detailed(
        client=client,
        last=last,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    last: int | Unset = UNSET,
) -> Response[Any | ApiErrorBody]:
    """`GET /api/v1/notifications/logs?last=N`, notification history.

     Reads from `notifications.db` via the repository.
    Falls back to `hitl.db` if the repo is not available (backward compat).

    Args:
        last (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        last=last,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    last: int | Unset = UNSET,
) -> Any | ApiErrorBody | None:
    """`GET /api/v1/notifications/logs?last=N`, notification history.

     Reads from `notifications.db` via the repository.
    Falls back to `hitl.db` if the repo is not available (backward compat).

    Args:
        last (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            client=client,
            last=last,
        )
    ).parsed
