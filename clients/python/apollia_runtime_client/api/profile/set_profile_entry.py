from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.profile_entry_view import ProfileEntryView
from ...models.set_profile_entry_body import SetProfileEntryBody
from ...types import Response


def _get_kwargs(
    key: str,
    *,
    body: SetProfileEntryBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "put",
        "url": "/api/v1/profile/entries/{key}".format(
            key=quote(str(key), safe=""),
        ),
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | ProfileEntryView | None:
    if response.status_code == 200:
        response_200 = ProfileEntryView.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | ProfileEntryView]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    key: str,
    *,
    client: AuthenticatedClient | Client,
    body: SetProfileEntryBody,
) -> Response[ApiErrorBody | ProfileEntryView]:
    """`PUT /api/v1/profile/entries/{key}`: creates or replaces one entry, as
    written by the user.

    Args:
        key (str):
        body (SetProfileEntryBody): Request body for `PUT /api/v1/profile/entries/{key}`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ProfileEntryView]
    """

    kwargs = _get_kwargs(
        key=key,
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    key: str,
    *,
    client: AuthenticatedClient | Client,
    body: SetProfileEntryBody,
) -> ApiErrorBody | ProfileEntryView | None:
    """`PUT /api/v1/profile/entries/{key}`: creates or replaces one entry, as
    written by the user.

    Args:
        key (str):
        body (SetProfileEntryBody): Request body for `PUT /api/v1/profile/entries/{key}`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ProfileEntryView
    """

    return sync_detailed(
        key=key,
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    key: str,
    *,
    client: AuthenticatedClient | Client,
    body: SetProfileEntryBody,
) -> Response[ApiErrorBody | ProfileEntryView]:
    """`PUT /api/v1/profile/entries/{key}`: creates or replaces one entry, as
    written by the user.

    Args:
        key (str):
        body (SetProfileEntryBody): Request body for `PUT /api/v1/profile/entries/{key}`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ProfileEntryView]
    """

    kwargs = _get_kwargs(
        key=key,
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    key: str,
    *,
    client: AuthenticatedClient | Client,
    body: SetProfileEntryBody,
) -> ApiErrorBody | ProfileEntryView | None:
    """`PUT /api/v1/profile/entries/{key}`: creates or replaces one entry, as
    written by the user.

    Args:
        key (str):
        body (SetProfileEntryBody): Request body for `PUT /api/v1/profile/entries/{key}`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ProfileEntryView
    """

    return (
        await asyncio_detailed(
            key=key,
            client=client,
            body=body,
        )
    ).parsed
