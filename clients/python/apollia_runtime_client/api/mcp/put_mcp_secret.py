from http import HTTPStatus
from typing import Any, cast
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.put_mcp_secret_body import PutMcpSecretBody
from ...types import Response


def _get_kwargs(
    name: str,
    key: str,
    *,
    body: PutMcpSecretBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "put",
        "url": "/api/v1/mcp/servers/{name}/secrets/{key}".format(
            name=quote(str(name), safe=""),
            key=quote(str(key), safe=""),
        ),
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Any | ApiErrorBody | None:
    if response.status_code == 204:
        response_204 = cast(Any, None)
        return response_204

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 500:
        response_500 = ApiErrorBody.from_dict(response.json())

        return response_500

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[Any | ApiErrorBody]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    name: str,
    key: str,
    *,
    client: AuthenticatedClient | Client,
    body: PutMcpSecretBody,
) -> Response[Any | ApiErrorBody]:
    """`PUT /api/v1/mcp/servers/:name/secrets/:key`: set the secret a
    `${APOLLIA_SECRET:KEY}` placeholder of server `name` resolves to.

     The server need not exist yet. A running server reads the new value at its
    next connection (`POST /api/v1/mcp/servers/:name/restart`).

    Args:
        name (str):
        key (str):
        body (PutMcpSecretBody): Body of `PUT /api/v1/mcp/servers/:name/secrets/:key`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        name=name,
        key=key,
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    name: str,
    key: str,
    *,
    client: AuthenticatedClient | Client,
    body: PutMcpSecretBody,
) -> Any | ApiErrorBody | None:
    """`PUT /api/v1/mcp/servers/:name/secrets/:key`: set the secret a
    `${APOLLIA_SECRET:KEY}` placeholder of server `name` resolves to.

     The server need not exist yet. A running server reads the new value at its
    next connection (`POST /api/v1/mcp/servers/:name/restart`).

    Args:
        name (str):
        key (str):
        body (PutMcpSecretBody): Body of `PUT /api/v1/mcp/servers/:name/secrets/:key`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return sync_detailed(
        name=name,
        key=key,
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    name: str,
    key: str,
    *,
    client: AuthenticatedClient | Client,
    body: PutMcpSecretBody,
) -> Response[Any | ApiErrorBody]:
    """`PUT /api/v1/mcp/servers/:name/secrets/:key`: set the secret a
    `${APOLLIA_SECRET:KEY}` placeholder of server `name` resolves to.

     The server need not exist yet. A running server reads the new value at its
    next connection (`POST /api/v1/mcp/servers/:name/restart`).

    Args:
        name (str):
        key (str):
        body (PutMcpSecretBody): Body of `PUT /api/v1/mcp/servers/:name/secrets/:key`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        name=name,
        key=key,
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    name: str,
    key: str,
    *,
    client: AuthenticatedClient | Client,
    body: PutMcpSecretBody,
) -> Any | ApiErrorBody | None:
    """`PUT /api/v1/mcp/servers/:name/secrets/:key`: set the secret a
    `${APOLLIA_SECRET:KEY}` placeholder of server `name` resolves to.

     The server need not exist yet. A running server reads the new value at its
    next connection (`POST /api/v1/mcp/servers/:name/restart`).

    Args:
        name (str):
        key (str):
        body (PutMcpSecretBody): Body of `PUT /api/v1/mcp/servers/:name/secrets/:key`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            name=name,
            key=key,
            client=client,
            body=body,
        )
    ).parsed
