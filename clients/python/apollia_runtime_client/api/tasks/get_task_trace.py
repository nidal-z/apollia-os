from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.trace_response import TraceResponse
from ...types import UNSET, Response, Unset


def _get_kwargs(
    id: str,
    *,
    since: str | Unset = UNSET,
    limit: int | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["since"] = since

    params["limit"] = limit

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/tasks/{id}/trace".format(
            id=quote(str(id), safe=""),
        ),
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | TraceResponse | None:
    if response.status_code == 200:
        response_200 = TraceResponse.from_dict(response.json())

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
) -> Response[ApiErrorBody | TraceResponse]:
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
    since: str | Unset = UNSET,
    limit: int | Unset = UNSET,
) -> Response[ApiErrorBody | TraceResponse]:
    """Handler for `GET /api/v1/tasks/{id}/trace`.

     200: `TraceResponse` (may be empty if the task has not yet produced any
    persisted event).
    500: error opening the database or running the query.

    Args:
        id (str):
        since (str | Unset):
        limit (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | TraceResponse]
    """

    kwargs = _get_kwargs(
        id=id,
        since=since,
        limit=limit,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    since: str | Unset = UNSET,
    limit: int | Unset = UNSET,
) -> ApiErrorBody | TraceResponse | None:
    """Handler for `GET /api/v1/tasks/{id}/trace`.

     200: `TraceResponse` (may be empty if the task has not yet produced any
    persisted event).
    500: error opening the database or running the query.

    Args:
        id (str):
        since (str | Unset):
        limit (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | TraceResponse
    """

    return sync_detailed(
        id=id,
        client=client,
        since=since,
        limit=limit,
    ).parsed


async def asyncio_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    since: str | Unset = UNSET,
    limit: int | Unset = UNSET,
) -> Response[ApiErrorBody | TraceResponse]:
    """Handler for `GET /api/v1/tasks/{id}/trace`.

     200: `TraceResponse` (may be empty if the task has not yet produced any
    persisted event).
    500: error opening the database or running the query.

    Args:
        id (str):
        since (str | Unset):
        limit (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | TraceResponse]
    """

    kwargs = _get_kwargs(
        id=id,
        since=since,
        limit=limit,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    since: str | Unset = UNSET,
    limit: int | Unset = UNSET,
) -> ApiErrorBody | TraceResponse | None:
    """Handler for `GET /api/v1/tasks/{id}/trace`.

     200: `TraceResponse` (may be empty if the task has not yet produced any
    persisted event).
    500: error opening the database or running the query.

    Args:
        id (str):
        since (str | Unset):
        limit (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | TraceResponse
    """

    return (
        await asyncio_detailed(
            id=id,
            client=client,
            since=since,
            limit=limit,
        )
    ).parsed
