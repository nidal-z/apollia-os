from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.set_credential_body import SetCredentialBody
from ...models.stored_credential import StoredCredential
from ...types import Response


def _get_kwargs(
    tool_name: str,
    key_name: str,
    *,
    body: SetCredentialBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "put",
        "url": "/api/v1/tool-credentials/{tool_name}/{key_name}".format(
            tool_name=quote(str(tool_name), safe=""),
            key_name=quote(str(key_name), safe=""),
        ),
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | StoredCredential | None:
    if response.status_code == 200:
        response_200 = StoredCredential.from_dict(response.json())

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
) -> Response[ApiErrorBody | StoredCredential]:
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
    body: SetCredentialBody,
) -> Response[ApiErrorBody | StoredCredential]:
    """`PUT /api/v1/tool-credentials/{tool_name}/{key_name}`: stores or replaces
    one credential, encrypted.

    Args:
        tool_name (str):
        key_name (str):
        body (SetCredentialBody): Request body of `PUT /api/v1/tool-
            credentials/{tool_name}/{key_name}`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | StoredCredential]
    """

    kwargs = _get_kwargs(
        tool_name=tool_name,
        key_name=key_name,
        body=body,
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
    body: SetCredentialBody,
) -> ApiErrorBody | StoredCredential | None:
    """`PUT /api/v1/tool-credentials/{tool_name}/{key_name}`: stores or replaces
    one credential, encrypted.

    Args:
        tool_name (str):
        key_name (str):
        body (SetCredentialBody): Request body of `PUT /api/v1/tool-
            credentials/{tool_name}/{key_name}`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | StoredCredential
    """

    return sync_detailed(
        tool_name=tool_name,
        key_name=key_name,
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    tool_name: str,
    key_name: str,
    *,
    client: AuthenticatedClient | Client,
    body: SetCredentialBody,
) -> Response[ApiErrorBody | StoredCredential]:
    """`PUT /api/v1/tool-credentials/{tool_name}/{key_name}`: stores or replaces
    one credential, encrypted.

    Args:
        tool_name (str):
        key_name (str):
        body (SetCredentialBody): Request body of `PUT /api/v1/tool-
            credentials/{tool_name}/{key_name}`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | StoredCredential]
    """

    kwargs = _get_kwargs(
        tool_name=tool_name,
        key_name=key_name,
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    tool_name: str,
    key_name: str,
    *,
    client: AuthenticatedClient | Client,
    body: SetCredentialBody,
) -> ApiErrorBody | StoredCredential | None:
    """`PUT /api/v1/tool-credentials/{tool_name}/{key_name}`: stores or replaces
    one credential, encrypted.

    Args:
        tool_name (str):
        key_name (str):
        body (SetCredentialBody): Request body of `PUT /api/v1/tool-
            credentials/{tool_name}/{key_name}`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | StoredCredential
    """

    return (
        await asyncio_detailed(
            tool_name=tool_name,
            key_name=key_name,
            client=client,
            body=body,
        )
    ).parsed
