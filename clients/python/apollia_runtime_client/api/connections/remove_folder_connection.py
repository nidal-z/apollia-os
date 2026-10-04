from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.connection import Connection
from ...models.folder_error_body import FolderErrorBody
from ...types import UNSET, Response, Unset


def _get_kwargs(
    id: str,
    *,
    confirm: bool | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["confirm"] = confirm

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "delete",
        "url": "/api/v1/connections/folders/{id}".format(
            id=quote(str(id), safe=""),
        ),
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Connection | FolderErrorBody | None:
    if response.status_code == 200:
        response_200 = Connection.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = FolderErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 409:
        response_409 = FolderErrorBody.from_dict(response.json())

        return response_409

    if response.status_code == 500:
        response_500 = FolderErrorBody.from_dict(response.json())

        return response_500

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[Connection | FolderErrorBody]:
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
    confirm: bool | Unset = UNSET,
) -> Response[Connection | FolderErrorBody]:
    """`DELETE /api/v1/connections/folders/{id}`: remove a folder connection.

    Args:
        id (str):
        confirm (bool | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Connection | FolderErrorBody]
    """

    kwargs = _get_kwargs(
        id=id,
        confirm=confirm,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    confirm: bool | Unset = UNSET,
) -> Connection | FolderErrorBody | None:
    """`DELETE /api/v1/connections/folders/{id}`: remove a folder connection.

    Args:
        id (str):
        confirm (bool | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Connection | FolderErrorBody
    """

    return sync_detailed(
        id=id,
        client=client,
        confirm=confirm,
    ).parsed


async def asyncio_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    confirm: bool | Unset = UNSET,
) -> Response[Connection | FolderErrorBody]:
    """`DELETE /api/v1/connections/folders/{id}`: remove a folder connection.

    Args:
        id (str):
        confirm (bool | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Connection | FolderErrorBody]
    """

    kwargs = _get_kwargs(
        id=id,
        confirm=confirm,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    confirm: bool | Unset = UNSET,
) -> Connection | FolderErrorBody | None:
    """`DELETE /api/v1/connections/folders/{id}`: remove a folder connection.

    Args:
        id (str):
        confirm (bool | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Connection | FolderErrorBody
    """

    return (
        await asyncio_detailed(
            id=id,
            client=client,
            confirm=confirm,
        )
    ).parsed
