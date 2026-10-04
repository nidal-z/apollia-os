from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.source_kind_dto import SourceKindDto
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    developer_mode: bool | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["developerMode"] = developer_mode

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/sources/folder-kinds",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> list[SourceKindDto] | None:
    if response.status_code == 200:
        response_200 = []
        _response_200 = response.json()
        for response_200_item_data in _response_200:
            response_200_item = SourceKindDto(response_200_item_data)

            response_200.append(response_200_item)

        return response_200

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[list[SourceKindDto]]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    developer_mode: bool | Unset = UNSET,
) -> Response[list[SourceKindDto]]:
    """`GET /api/v1/sources/folder-kinds`: the kinds offered for a source made
    on a folder connection.

     The code folder is offered only when the developer mode is on. This is
    what the user is offered, not a check: `POST /api/v1/sources` accepts a
    code folder whatever the mode.

    Args:
        developer_mode (bool | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[list[SourceKindDto]]
    """

    kwargs = _get_kwargs(
        developer_mode=developer_mode,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    developer_mode: bool | Unset = UNSET,
) -> list[SourceKindDto] | None:
    """`GET /api/v1/sources/folder-kinds`: the kinds offered for a source made
    on a folder connection.

     The code folder is offered only when the developer mode is on. This is
    what the user is offered, not a check: `POST /api/v1/sources` accepts a
    code folder whatever the mode.

    Args:
        developer_mode (bool | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        list[SourceKindDto]
    """

    return sync_detailed(
        client=client,
        developer_mode=developer_mode,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    developer_mode: bool | Unset = UNSET,
) -> Response[list[SourceKindDto]]:
    """`GET /api/v1/sources/folder-kinds`: the kinds offered for a source made
    on a folder connection.

     The code folder is offered only when the developer mode is on. This is
    what the user is offered, not a check: `POST /api/v1/sources` accepts a
    code folder whatever the mode.

    Args:
        developer_mode (bool | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[list[SourceKindDto]]
    """

    kwargs = _get_kwargs(
        developer_mode=developer_mode,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    developer_mode: bool | Unset = UNSET,
) -> list[SourceKindDto] | None:
    """`GET /api/v1/sources/folder-kinds`: the kinds offered for a source made
    on a folder connection.

     The code folder is offered only when the developer mode is on. This is
    what the user is offered, not a check: `POST /api/v1/sources` accepts a
    code folder whatever the mode.

    Args:
        developer_mode (bool | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        list[SourceKindDto]
    """

    return (
        await asyncio_detailed(
            client=client,
            developer_mode=developer_mode,
        )
    ).parsed
