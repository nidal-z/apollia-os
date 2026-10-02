from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.embedding_models_response import EmbeddingModelsResponse
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    language_model_bytes: int | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["language_model_bytes"] = language_model_bytes

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/llm/embeddings",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | EmbeddingModelsResponse | None:
    if response.status_code == 200:
        response_200 = EmbeddingModelsResponse.from_dict(response.json())

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
) -> Response[ApiErrorBody | EmbeddingModelsResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    language_model_bytes: int | Unset = UNSET,
) -> Response[ApiErrorBody | EmbeddingModelsResponse]:
    """`GET /api/v1/llm/embeddings?language_model_bytes=...`

    Args:
        language_model_bytes (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | EmbeddingModelsResponse]
    """

    kwargs = _get_kwargs(
        language_model_bytes=language_model_bytes,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    language_model_bytes: int | Unset = UNSET,
) -> ApiErrorBody | EmbeddingModelsResponse | None:
    """`GET /api/v1/llm/embeddings?language_model_bytes=...`

    Args:
        language_model_bytes (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | EmbeddingModelsResponse
    """

    return sync_detailed(
        client=client,
        language_model_bytes=language_model_bytes,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    language_model_bytes: int | Unset = UNSET,
) -> Response[ApiErrorBody | EmbeddingModelsResponse]:
    """`GET /api/v1/llm/embeddings?language_model_bytes=...`

    Args:
        language_model_bytes (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | EmbeddingModelsResponse]
    """

    kwargs = _get_kwargs(
        language_model_bytes=language_model_bytes,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    language_model_bytes: int | Unset = UNSET,
) -> ApiErrorBody | EmbeddingModelsResponse | None:
    """`GET /api/v1/llm/embeddings?language_model_bytes=...`

    Args:
        language_model_bytes (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | EmbeddingModelsResponse
    """

    return (
        await asyncio_detailed(
            client=client,
            language_model_bytes=language_model_bytes,
        )
    ).parsed
