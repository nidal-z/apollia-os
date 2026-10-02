from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.model_source_dto import ModelSourceDto
from ...models.presets_list_response import PresetsListResponse
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    model_source: ModelSourceDto | Unset = UNSET,
    model_id: str | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    json_model_source: str | Unset = UNSET
    if not isinstance(model_source, Unset):
        json_model_source = model_source.value

    params["model_source"] = json_model_source

    params["model_id"] = model_id

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/llm/presets",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | PresetsListResponse | None:
    if response.status_code == 200:
        response_200 = PresetsListResponse.from_dict(response.json())

        return response_200

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
) -> Response[ApiErrorBody | PresetsListResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    model_source: ModelSourceDto | Unset = UNSET,
    model_id: str | Unset = UNSET,
) -> Response[ApiErrorBody | PresetsListResponse]:
    """`GET /api/v1/llm/presets`: every preset, or those of one model.

    Args:
        model_source (ModelSourceDto | Unset): Where a model runs.
        model_id (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | PresetsListResponse]
    """

    kwargs = _get_kwargs(
        model_source=model_source,
        model_id=model_id,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    model_source: ModelSourceDto | Unset = UNSET,
    model_id: str | Unset = UNSET,
) -> ApiErrorBody | PresetsListResponse | None:
    """`GET /api/v1/llm/presets`: every preset, or those of one model.

    Args:
        model_source (ModelSourceDto | Unset): Where a model runs.
        model_id (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | PresetsListResponse
    """

    return sync_detailed(
        client=client,
        model_source=model_source,
        model_id=model_id,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    model_source: ModelSourceDto | Unset = UNSET,
    model_id: str | Unset = UNSET,
) -> Response[ApiErrorBody | PresetsListResponse]:
    """`GET /api/v1/llm/presets`: every preset, or those of one model.

    Args:
        model_source (ModelSourceDto | Unset): Where a model runs.
        model_id (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | PresetsListResponse]
    """

    kwargs = _get_kwargs(
        model_source=model_source,
        model_id=model_id,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    model_source: ModelSourceDto | Unset = UNSET,
    model_id: str | Unset = UNSET,
) -> ApiErrorBody | PresetsListResponse | None:
    """`GET /api/v1/llm/presets`: every preset, or those of one model.

    Args:
        model_source (ModelSourceDto | Unset): Where a model runs.
        model_id (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | PresetsListResponse
    """

    return (
        await asyncio_detailed(
            client=client,
            model_source=model_source,
            model_id=model_id,
        )
    ).parsed
