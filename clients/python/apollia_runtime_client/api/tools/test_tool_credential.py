from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.credential_test_view import CredentialTestView
from ...types import Response


def _get_kwargs(
    tool_name: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/api/v1/tool-credential-tests/{tool_name}".format(
            tool_name=quote(str(tool_name), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | CredentialTestView | None:
    if response.status_code == 200:
        response_200 = CredentialTestView.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | CredentialTestView]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    tool_name: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | CredentialTestView]:
    """`POST /api/v1/tool-credential-tests/{tool_name}`: validates the tool's
    credential with a live call. A missing or rejected key is an answer with
    `ok: false`, not an error.

    Args:
        tool_name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | CredentialTestView]
    """

    kwargs = _get_kwargs(
        tool_name=tool_name,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    tool_name: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | CredentialTestView | None:
    """`POST /api/v1/tool-credential-tests/{tool_name}`: validates the tool's
    credential with a live call. A missing or rejected key is an answer with
    `ok: false`, not an error.

    Args:
        tool_name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | CredentialTestView
    """

    return sync_detailed(
        tool_name=tool_name,
        client=client,
    ).parsed


async def asyncio_detailed(
    tool_name: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | CredentialTestView]:
    """`POST /api/v1/tool-credential-tests/{tool_name}`: validates the tool's
    credential with a live call. A missing or rejected key is an answer with
    `ok: false`, not an error.

    Args:
        tool_name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | CredentialTestView]
    """

    kwargs = _get_kwargs(
        tool_name=tool_name,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    tool_name: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | CredentialTestView | None:
    """`POST /api/v1/tool-credential-tests/{tool_name}`: validates the tool's
    credential with a live call. A missing or rejected key is an answer with
    `ok: false`, not an error.

    Args:
        tool_name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | CredentialTestView
    """

    return (
        await asyncio_detailed(
            tool_name=tool_name,
            client=client,
        )
    ).parsed
