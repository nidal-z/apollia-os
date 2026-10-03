from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.injected_memory_entry_view import InjectedMemoryEntryView
from ...types import Response


def _get_kwargs(
    turn_id: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/memory/injections/{turn_id}".format(
            turn_id=quote(str(turn_id), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> list[InjectedMemoryEntryView] | None:
    if response.status_code == 200:
        response_200 = []
        _response_200 = response.json()
        for response_200_item_data in _response_200:
            response_200_item = InjectedMemoryEntryView.from_dict(response_200_item_data)

            response_200.append(response_200_item)

        return response_200

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[list[InjectedMemoryEntryView]]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    turn_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[list[InjectedMemoryEntryView]]:
    """`GET /api/v1/memory/injections/{turn_id}`: the entries an agent recalled
    during one turn.

    Args:
        turn_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[list[InjectedMemoryEntryView]]
    """

    kwargs = _get_kwargs(
        turn_id=turn_id,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    turn_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> list[InjectedMemoryEntryView] | None:
    """`GET /api/v1/memory/injections/{turn_id}`: the entries an agent recalled
    during one turn.

    Args:
        turn_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        list[InjectedMemoryEntryView]
    """

    return sync_detailed(
        turn_id=turn_id,
        client=client,
    ).parsed


async def asyncio_detailed(
    turn_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[list[InjectedMemoryEntryView]]:
    """`GET /api/v1/memory/injections/{turn_id}`: the entries an agent recalled
    during one turn.

    Args:
        turn_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[list[InjectedMemoryEntryView]]
    """

    kwargs = _get_kwargs(
        turn_id=turn_id,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    turn_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> list[InjectedMemoryEntryView] | None:
    """`GET /api/v1/memory/injections/{turn_id}`: the entries an agent recalled
    during one turn.

    Args:
        turn_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        list[InjectedMemoryEntryView]
    """

    return (
        await asyncio_detailed(
            turn_id=turn_id,
            client=client,
        )
    ).parsed
