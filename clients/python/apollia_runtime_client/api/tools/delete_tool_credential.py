from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.deleted_credential import DeletedCredential
from ...types import Response


def _get_kwargs(
    tool_name: str,
    key_name: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "delete",
        "url": "/api/v1/tool-credentials/{tool_name}/{key_name}".format(
            tool_name=quote(str(tool_name), safe=""),
            key_name=quote(str(key_name), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | DeletedCredential | None:
    if response.status_code == 200:
        response_200 = DeletedCredential.from_dict(response.json())

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
) -> Response[ApiErrorBody | DeletedCredential]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    tool_name: str,
    key_name: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | DeletedCredential]:
    """`DELETE /api/v1/tool-credentials/{tool_name}/{key_name}`: removes one
    credential. Removing one that is not there is not an error.

    Args:
        tool_name (str):
        key_name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | DeletedCredential]
    """

    kwargs = _get_kwargs(
        tool_name=tool_name,
        key_name=key_name,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    tool_name: str,
    key_name: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | DeletedCredential | None:
    """`DELETE /api/v1/tool-credentials/{tool_name}/{key_name}`: removes one
    credential. Removing one that is not there is not an error.

    Args:
        tool_name (str):
        key_name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | DeletedCredential
    """

    return sync_detailed(
        tool_name=tool_name,
        key_name=key_name,
        client=client,
    ).parsed


async def asyncio_detailed(
    tool_name: str,
    key_name: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | DeletedCredential]:
    """`DELETE /api/v1/tool-credentials/{tool_name}/{key_name}`: removes one
    credential. Removing one that is not there is not an error.

    Args:
        tool_name (str):
        key_name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | DeletedCredential]
    """

    kwargs = _get_kwargs(
        tool_name=tool_name,
        key_name=key_name,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    tool_name: str,
    key_name: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | DeletedCredential | None:
    """`DELETE /api/v1/tool-credentials/{tool_name}/{key_name}`: removes one
    credential. Removing one that is not there is not an error.

    Args:
        tool_name (str):
        key_name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | DeletedCredential
    """

    return (
        await asyncio_detailed(
            tool_name=tool_name,
            key_name=key_name,
            client=client,
        )
    ).parsed
