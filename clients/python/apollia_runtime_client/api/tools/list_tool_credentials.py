from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.credential_entry_view import CredentialEntryView
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    tool_name: str | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["tool_name"] = tool_name

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/tool-credentials",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | list[CredentialEntryView] | None:
    if response.status_code == 200:
        response_200 = []
        _response_200 = response.json()
        for response_200_item_data in _response_200:
            response_200_item = CredentialEntryView.from_dict(response_200_item_data)

            response_200.append(response_200_item)

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
) -> Response[ApiErrorBody | list[CredentialEntryView]]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    tool_name: str | Unset = UNSET,
) -> Response[ApiErrorBody | list[CredentialEntryView]]:
    """`GET /api/v1/tool-credentials`: the stored credentials, values left out.

    Args:
        tool_name (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | list[CredentialEntryView]]
    """

    kwargs = _get_kwargs(
        tool_name=tool_name,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    tool_name: str | Unset = UNSET,
) -> ApiErrorBody | list[CredentialEntryView] | None:
    """`GET /api/v1/tool-credentials`: the stored credentials, values left out.

    Args:
        tool_name (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | list[CredentialEntryView]
    """

    return sync_detailed(
        client=client,
        tool_name=tool_name,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    tool_name: str | Unset = UNSET,
) -> Response[ApiErrorBody | list[CredentialEntryView]]:
    """`GET /api/v1/tool-credentials`: the stored credentials, values left out.

    Args:
        tool_name (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | list[CredentialEntryView]]
    """

    kwargs = _get_kwargs(
        tool_name=tool_name,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    tool_name: str | Unset = UNSET,
) -> ApiErrorBody | list[CredentialEntryView] | None:
    """`GET /api/v1/tool-credentials`: the stored credentials, values left out.

    Args:
        tool_name (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | list[CredentialEntryView]
    """

    return (
        await asyncio_detailed(
            client=client,
            tool_name=tool_name,
        )
    ).parsed
