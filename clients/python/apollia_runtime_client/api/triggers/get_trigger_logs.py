from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.logs_response import LogsResponse
from ...types import UNSET, Response, Unset


def _get_kwargs(
    id: str,
    *,
    last: int | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["last"] = last

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/triggers/{id}/logs".format(
            id=quote(str(id), safe=""),
        ),
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | LogsResponse | None:
    if response.status_code == 200:
        response_200 = LogsResponse.from_dict(response.json())

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
) -> Response[ApiErrorBody | LogsResponse]:
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
    last: int | Unset = UNSET,
) -> Response[ApiErrorBody | LogsResponse]:
    """`GET /api/v1/triggers/:id/logs`, firing history from SQLite.

     The `?last=N` query parameter controls the number of entries (default: 20).

    Args:
        id (str):
        last (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | LogsResponse]
    """

    kwargs = _get_kwargs(
        id=id,
        last=last,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    last: int | Unset = UNSET,
) -> ApiErrorBody | LogsResponse | None:
    """`GET /api/v1/triggers/:id/logs`, firing history from SQLite.

     The `?last=N` query parameter controls the number of entries (default: 20).

    Args:
        id (str):
        last (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | LogsResponse
    """

    return sync_detailed(
        id=id,
        client=client,
        last=last,
    ).parsed


async def asyncio_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    last: int | Unset = UNSET,
) -> Response[ApiErrorBody | LogsResponse]:
    """`GET /api/v1/triggers/:id/logs`, firing history from SQLite.

     The `?last=N` query parameter controls the number of entries (default: 20).

    Args:
        id (str):
        last (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | LogsResponse]
    """

    kwargs = _get_kwargs(
        id=id,
        last=last,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    last: int | Unset = UNSET,
) -> ApiErrorBody | LogsResponse | None:
    """`GET /api/v1/triggers/:id/logs`, firing history from SQLite.

     The `?last=N` query parameter controls the number of entries (default: 20).

    Args:
        id (str):
        last (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | LogsResponse
    """

    return (
        await asyncio_detailed(
            id=id,
            client=client,
            last=last,
        )
    ).parsed
