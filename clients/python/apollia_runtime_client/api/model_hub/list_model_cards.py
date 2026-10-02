from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.model_cards_response import ModelCardsResponse
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    lang: str | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["lang"] = lang

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/llm/registry/cards",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | ModelCardsResponse | None:
    if response.status_code == 200:
        response_200 = ModelCardsResponse.from_dict(response.json())

        return response_200

    if response.status_code == 502:
        response_502 = ApiErrorBody.from_dict(response.json())

        return response_502

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | ModelCardsResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    lang: str | Unset = UNSET,
) -> Response[ApiErrorBody | ModelCardsResponse]:
    """`GET /api/v1/llm/registry/cards?lang=...`

    Args:
        lang (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ModelCardsResponse]
    """

    kwargs = _get_kwargs(
        lang=lang,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    lang: str | Unset = UNSET,
) -> ApiErrorBody | ModelCardsResponse | None:
    """`GET /api/v1/llm/registry/cards?lang=...`

    Args:
        lang (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ModelCardsResponse
    """

    return sync_detailed(
        client=client,
        lang=lang,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    lang: str | Unset = UNSET,
) -> Response[ApiErrorBody | ModelCardsResponse]:
    """`GET /api/v1/llm/registry/cards?lang=...`

    Args:
        lang (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | ModelCardsResponse]
    """

    kwargs = _get_kwargs(
        lang=lang,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    lang: str | Unset = UNSET,
) -> ApiErrorBody | ModelCardsResponse | None:
    """`GET /api/v1/llm/registry/cards?lang=...`

    Args:
        lang (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | ModelCardsResponse
    """

    return (
        await asyncio_detailed(
            client=client,
            lang=lang,
        )
    ).parsed
