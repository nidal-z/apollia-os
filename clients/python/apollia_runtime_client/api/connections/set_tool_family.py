from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.connection import Connection
from ...models.tool_family_body import ToolFamilyBody
from ...types import Response


def _get_kwargs(
    id: str,
    tool: str,
    *,
    body: ToolFamilyBody,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "put",
        "url": "/api/v1/connections/{id}/tools/{tool}/family".format(
            id=quote(str(id), safe=""),
            tool=quote(str(tool), safe=""),
        ),
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | Connection | None:
    if response.status_code == 200:
        response_200 = Connection.from_dict(response.json())

        return response_200

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 409:
        response_409 = ApiErrorBody.from_dict(response.json())

        return response_409

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
) -> Response[ApiErrorBody | Connection]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    id: str,
    tool: str,
    *,
    client: AuthenticatedClient | Client,
    body: ToolFamilyBody,
) -> Response[ApiErrorBody | Connection]:
    """`PUT /api/v1/connections/{id}/tools/{tool}/family`: give a tool of a
    connection the selection does not classify another action family.

    Args:
        id (str):
        tool (str):
        body (ToolFamilyBody): Body of `PUT /api/v1/connections/{id}/tools/{tool}/family`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | Connection]
    """

    kwargs = _get_kwargs(
        id=id,
        tool=tool,
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    id: str,
    tool: str,
    *,
    client: AuthenticatedClient | Client,
    body: ToolFamilyBody,
) -> ApiErrorBody | Connection | None:
    """`PUT /api/v1/connections/{id}/tools/{tool}/family`: give a tool of a
    connection the selection does not classify another action family.

    Args:
        id (str):
        tool (str):
        body (ToolFamilyBody): Body of `PUT /api/v1/connections/{id}/tools/{tool}/family`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | Connection
    """

    return sync_detailed(
        id=id,
        tool=tool,
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    id: str,
    tool: str,
    *,
    client: AuthenticatedClient | Client,
    body: ToolFamilyBody,
) -> Response[ApiErrorBody | Connection]:
    """`PUT /api/v1/connections/{id}/tools/{tool}/family`: give a tool of a
    connection the selection does not classify another action family.

    Args:
        id (str):
        tool (str):
        body (ToolFamilyBody): Body of `PUT /api/v1/connections/{id}/tools/{tool}/family`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | Connection]
    """

    kwargs = _get_kwargs(
        id=id,
        tool=tool,
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    id: str,
    tool: str,
    *,
    client: AuthenticatedClient | Client,
    body: ToolFamilyBody,
) -> ApiErrorBody | Connection | None:
    """`PUT /api/v1/connections/{id}/tools/{tool}/family`: give a tool of a
    connection the selection does not classify another action family.

    Args:
        id (str):
        tool (str):
        body (ToolFamilyBody): Body of `PUT /api/v1/connections/{id}/tools/{tool}/family`.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | Connection
    """

    return (
        await asyncio_detailed(
            id=id,
            tool=tool,
            client=client,
            body=body,
        )
    ).parsed
