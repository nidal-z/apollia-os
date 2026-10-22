from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.connection_removal import ConnectionRemoval
from ...types import UNSET, Response, Unset


def _get_kwargs(
    id: str,
    *,
    dry_run: str | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["dry_run"] = dry_run

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "delete",
        "url": "/api/v1/connections/{id}".format(
            id=quote(str(id), safe=""),
        ),
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | ConnectionRemoval | None:
    if response.status_code == 200:
        response_200 = ConnectionRemoval.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

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
) -> Response[ApiErrorBody | ConnectionRemoval]:
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
    dry_run: str | Unset = UNSET,
) -> Response[ApiErrorBody | ConnectionRemoval]:
    """`DELETE /api/v1/connections/{id}`: remove a connection of any kind, and
    answer what was erased; with `?dry_run=1`, erase nothing and answer what
    would be.

    Args:
        id (str):
        dry_run (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ConnectionRemoval]
    """

    kwargs = _get_kwargs(
        id=id,
        dry_run=dry_run,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    dry_run: str | Unset = UNSET,
) -> ApiErrorBody | ConnectionRemoval | None:
    """`DELETE /api/v1/connections/{id}`: remove a connection of any kind, and
    answer what was erased; with `?dry_run=1`, erase nothing and answer what
    would be.

    Args:
        id (str):
        dry_run (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ConnectionRemoval
    """

    return sync_detailed(
        id=id,
        client=client,
        dry_run=dry_run,
    ).parsed


async def asyncio_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    dry_run: str | Unset = UNSET,
) -> Response[ApiErrorBody | ConnectionRemoval]:
    """`DELETE /api/v1/connections/{id}`: remove a connection of any kind, and
    answer what was erased; with `?dry_run=1`, erase nothing and answer what
    would be.

    Args:
        id (str):
        dry_run (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ConnectionRemoval]
    """

    kwargs = _get_kwargs(
        id=id,
        dry_run=dry_run,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    dry_run: str | Unset = UNSET,
) -> ApiErrorBody | ConnectionRemoval | None:
    """`DELETE /api/v1/connections/{id}`: remove a connection of any kind, and
    answer what was erased; with `?dry_run=1`, erase nothing and answer what
    would be.

    Args:
        id (str):
        dry_run (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ConnectionRemoval
    """

    return (
        await asyncio_detailed(
            id=id,
            client=client,
            dry_run=dry_run,
        )
    ).parsed
