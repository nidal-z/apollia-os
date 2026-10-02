from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.resolved_agent_response import ResolvedAgentResponse
from ...types import Response


def _get_kwargs(
    name: str,
    field: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "delete",
        "url": "/api/v1/agent-definitions/{name}/overrides/{field}".format(
            name=quote(str(name), safe=""),
            field=quote(str(field), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | ResolvedAgentResponse | None:
    if response.status_code == 200:
        response_200 = ResolvedAgentResponse.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 422:
        response_422 = ApiErrorBody.from_dict(response.json())

        return response_422

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | ResolvedAgentResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    name: str,
    field: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | ResolvedAgentResponse]:
    """`DELETE /api/v1/agent-definitions/{name}/overrides/{field}`: remove the
    user's value of a field; the package default applies again.

    Args:
        name (str):
        field (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ResolvedAgentResponse]
    """

    kwargs = _get_kwargs(
        name=name,
        field=field,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    name: str,
    field: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | ResolvedAgentResponse | None:
    """`DELETE /api/v1/agent-definitions/{name}/overrides/{field}`: remove the
    user's value of a field; the package default applies again.

    Args:
        name (str):
        field (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ResolvedAgentResponse
    """

    return sync_detailed(
        name=name,
        field=field,
        client=client,
    ).parsed


async def asyncio_detailed(
    name: str,
    field: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | ResolvedAgentResponse]:
    """`DELETE /api/v1/agent-definitions/{name}/overrides/{field}`: remove the
    user's value of a field; the package default applies again.

    Args:
        name (str):
        field (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ResolvedAgentResponse]
    """

    kwargs = _get_kwargs(
        name=name,
        field=field,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    name: str,
    field: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | ResolvedAgentResponse | None:
    """`DELETE /api/v1/agent-definitions/{name}/overrides/{field}`: remove the
    user's value of a field; the package default applies again.

    Args:
        name (str):
        field (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ResolvedAgentResponse
    """

    return (
        await asyncio_detailed(
            name=name,
            field=field,
            client=client,
        )
    ).parsed
