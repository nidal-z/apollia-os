from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.connection import Connection
from ...models.folder_error_body import FolderErrorBody
from ...models.repoint_folder_body import RepointFolderBody
from ...types import Response


def _get_kwargs(
    id: str,
    *,
    body: RepointFolderBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "put",
        "url": "/api/v1/connections/folders/{id}".format(
            id=quote(str(id), safe=""),
        ),
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Connection | FolderErrorBody | None:
    if response.status_code == 200:
        response_200 = Connection.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = FolderErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 403:
        response_403 = FolderErrorBody.from_dict(response.json())

        return response_403

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
    body: RepointFolderBody,
) -> Response[Connection | FolderErrorBody]:
    """`PUT /api/v1/connections/folders/{id}`: point a folder connection at
    another folder, its identifier, label and accesses kept.

    Args:
        id (str):
        body (RepointFolderBody): Body of `PUT /api/v1/connections/folders/{id}`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Connection | FolderErrorBody]
    """

    kwargs = _get_kwargs(
        id=id,
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    body: RepointFolderBody,
) -> Connection | FolderErrorBody | None:
    """`PUT /api/v1/connections/folders/{id}`: point a folder connection at
    another folder, its identifier, label and accesses kept.

    Args:
        id (str):
        body (RepointFolderBody): Body of `PUT /api/v1/connections/folders/{id}`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Connection | FolderErrorBody
    """

    return sync_detailed(
        id=id,
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    body: RepointFolderBody,
) -> Response[Connection | FolderErrorBody]:
    """`PUT /api/v1/connections/folders/{id}`: point a folder connection at
    another folder, its identifier, label and accesses kept.

    Args:
        id (str):
        body (RepointFolderBody): Body of `PUT /api/v1/connections/folders/{id}`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Connection | FolderErrorBody]
    """

    kwargs = _get_kwargs(
        id=id,
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    id: str,
    *,
    client: AuthenticatedClient | Client,
    body: RepointFolderBody,
) -> Connection | FolderErrorBody | None:
    """`PUT /api/v1/connections/folders/{id}`: point a folder connection at
    another folder, its identifier, label and accesses kept.

    Args:
        id (str):
        body (RepointFolderBody): Body of `PUT /api/v1/connections/folders/{id}`.

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
            body=body,
        )
    ).parsed
