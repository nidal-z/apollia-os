from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.config_error_body import ConfigErrorBody
from ...models.config_save_request import ConfigSaveRequest
from ...models.config_view import ConfigView
from ...types import Response


def _get_kwargs(
    *,
    body: ConfigSaveRequest,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "put",
        "url": "/api/v1/config",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ConfigErrorBody | ConfigView | None:
    if response.status_code == 200:
        response_200 = ConfigView.from_dict(response.json())

        return response_200

    if response.status_code == 403:
        response_403 = ConfigErrorBody.from_dict(response.json())

        return response_403

    if response.status_code == 409:
        response_409 = ConfigErrorBody.from_dict(response.json())

        return response_409

    if response.status_code == 422:
        response_422 = ConfigErrorBody.from_dict(response.json())

        return response_422

    if response.status_code == 500:
        response_500 = ConfigErrorBody.from_dict(response.json())

        return response_500

    if response.status_code == 503:
        response_503 = ConfigErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ConfigErrorBody | ConfigView]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: ConfigSaveRequest,
) -> Response[ConfigErrorBody | ConfigView]:
    """`PUT /api/v1/config`: saves a whole text through the writer.

     The change is written to the signed journal under the origin the caller
    declares. A caller that wrote the file itself, as the command line does,
    presents the text it wrote with its own fingerprint: nothing is written
    again, and the change it made is journaled under its origin instead of as
    a file changed from outside.

    Args:
        body (ConfigSaveRequest): A text to save, with the fingerprint of the text it was made
            from.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ConfigErrorBody | ConfigView]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    body: ConfigSaveRequest,
) -> ConfigErrorBody | ConfigView | None:
    """`PUT /api/v1/config`: saves a whole text through the writer.

     The change is written to the signed journal under the origin the caller
    declares. A caller that wrote the file itself, as the command line does,
    presents the text it wrote with its own fingerprint: nothing is written
    again, and the change it made is journaled under its origin instead of as
    a file changed from outside.

    Args:
        body (ConfigSaveRequest): A text to save, with the fingerprint of the text it was made
            from.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ConfigErrorBody | ConfigView
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: ConfigSaveRequest,
) -> Response[ConfigErrorBody | ConfigView]:
    """`PUT /api/v1/config`: saves a whole text through the writer.

     The change is written to the signed journal under the origin the caller
    declares. A caller that wrote the file itself, as the command line does,
    presents the text it wrote with its own fingerprint: nothing is written
    again, and the change it made is journaled under its origin instead of as
    a file changed from outside.

    Args:
        body (ConfigSaveRequest): A text to save, with the fingerprint of the text it was made
            from.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ConfigErrorBody | ConfigView]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: ConfigSaveRequest,
) -> ConfigErrorBody | ConfigView | None:
    """`PUT /api/v1/config`: saves a whole text through the writer.

     The change is written to the signed journal under the origin the caller
    declares. A caller that wrote the file itself, as the command line does,
    presents the text it wrote with its own fingerprint: nothing is written
    again, and the change it made is journaled under its origin instead of as
    a file changed from outside.

    Args:
        body (ConfigSaveRequest): A text to save, with the fingerprint of the text it was made
            from.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ConfigErrorBody | ConfigView
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
