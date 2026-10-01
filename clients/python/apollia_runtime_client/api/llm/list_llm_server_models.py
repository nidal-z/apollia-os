from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.server_models_response import ServerModelsResponse
from ...models.server_probe_error_response import ServerProbeErrorResponse
from ...types import Response


def _get_kwargs(
    name: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/llm/servers/{name}/models".format(
            name=quote(str(name), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ServerModelsResponse | ServerProbeErrorResponse | None:
    if response.status_code == 200:
        response_200 = ServerModelsResponse.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = ServerProbeErrorResponse.from_dict(response.json())

        return response_404

    if response.status_code == 502:
        response_502 = ServerProbeErrorResponse.from_dict(response.json())

        return response_502

    if response.status_code == 503:
        response_503 = ServerProbeErrorResponse.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ServerModelsResponse | ServerProbeErrorResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    name: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ServerModelsResponse | ServerProbeErrorResponse]:
    """`GET /api/v1/llm/servers/{name}/models`: the models of a declared server,
    each with the name it routes by.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ServerModelsResponse | ServerProbeErrorResponse]
    """

    kwargs = _get_kwargs(
        name=name,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    name: str,
    *,
    client: AuthenticatedClient | Client,
) -> ServerModelsResponse | ServerProbeErrorResponse | None:
    """`GET /api/v1/llm/servers/{name}/models`: the models of a declared server,
    each with the name it routes by.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ServerModelsResponse | ServerProbeErrorResponse
    """

    return sync_detailed(
        name=name,
        client=client,
    ).parsed


async def asyncio_detailed(
    name: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ServerModelsResponse | ServerProbeErrorResponse]:
    """`GET /api/v1/llm/servers/{name}/models`: the models of a declared server,
    each with the name it routes by.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ServerModelsResponse | ServerProbeErrorResponse]
    """

    kwargs = _get_kwargs(
        name=name,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    name: str,
    *,
    client: AuthenticatedClient | Client,
) -> ServerModelsResponse | ServerProbeErrorResponse | None:
    """`GET /api/v1/llm/servers/{name}/models`: the models of a declared server,
    each with the name it routes by.

    Args:
        name (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ServerModelsResponse | ServerProbeErrorResponse
    """

    return (
        await asyncio_detailed(
            name=name,
            client=client,
        )
    ).parsed
