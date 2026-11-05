from http import HTTPStatus
from typing import Any, cast
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.engine_removal_error_body import EngineRemovalErrorBody
from ...types import Response


def _get_kwargs(
    engine: str,
    tag: str,
    variant: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "delete",
        "url": "/api/v1/engines/{engine}/{tag}/{variant}".format(
            engine=quote(str(engine), safe=""),
            tag=quote(str(tag), safe=""),
            variant=quote(str(variant), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Any | EngineRemovalErrorBody | None:
    if response.status_code == 204:
        response_204 = cast(Any, None)
        return response_204

    if response.status_code == 404:
        response_404 = EngineRemovalErrorBody.from_dict(response.json())

        return response_404

    if response.status_code == 409:
        response_409 = EngineRemovalErrorBody.from_dict(response.json())

        return response_409

    if response.status_code == 500:
        response_500 = EngineRemovalErrorBody.from_dict(response.json())

        return response_500

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[Any | EngineRemovalErrorBody]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    engine: str,
    tag: str,
    variant: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[Any | EngineRemovalErrorBody]:
    """
    Args:
        engine (str):
        tag (str):
        variant (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | EngineRemovalErrorBody]
    """

    kwargs = _get_kwargs(
        engine=engine,
        tag=tag,
        variant=variant,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    engine: str,
    tag: str,
    variant: str,
    *,
    client: AuthenticatedClient | Client,
) -> Any | EngineRemovalErrorBody | None:
    """
    Args:
        engine (str):
        tag (str):
        variant (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | EngineRemovalErrorBody
    """

    return sync_detailed(
        engine=engine,
        tag=tag,
        variant=variant,
        client=client,
    ).parsed


async def asyncio_detailed(
    engine: str,
    tag: str,
    variant: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[Any | EngineRemovalErrorBody]:
    """
    Args:
        engine (str):
        tag (str):
        variant (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | EngineRemovalErrorBody]
    """

    kwargs = _get_kwargs(
        engine=engine,
        tag=tag,
        variant=variant,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    engine: str,
    tag: str,
    variant: str,
    *,
    client: AuthenticatedClient | Client,
) -> Any | EngineRemovalErrorBody | None:
    """
    Args:
        engine (str):
        tag (str):
        variant (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | EngineRemovalErrorBody
    """

    return (
        await asyncio_detailed(
            engine=engine,
            tag=tag,
            variant=variant,
            client=client,
        )
    ).parsed
