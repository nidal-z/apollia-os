from http import HTTPStatus
from typing import Any, cast

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    limit: int | Unset = UNSET,
    n_ctx: int | Unset = UNSET,
    hf_token: str | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["limit"] = limit

    params["n_ctx"] = n_ctx

    params["hf_token"] = hf_token

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/llm/recommend",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Any | ApiErrorBody | None:
    if response.status_code == 200:
        response_200 = cast(Any, None)
        return response_200

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
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
    n_ctx: int | Unset = UNSET,
    hf_token: str | Unset = UNSET,
) -> Response[Any | ApiErrorBody]:
    r"""`GET /api/v1/llm/recommend?limit=...&n_ctx=...`

     Ranks the models this machine should run: plans against the embedded
    generation table, resolves the survivors on HuggingFace, reads each
    finalist's GGUF header over a range request, and orders what is left.

    Answers `503` when the Hub cannot be reached, which is deliberately not the
    same as an empty `200`. The catalogue is fetched live, so no network means
    no catalogue; reporting that as \"no models fit\" would be a different and
    false claim about the operator's machine.

    Args:
        limit (int | Unset):
        n_ctx (int | Unset):
        hf_token (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        limit=limit,
        n_ctx=n_ctx,
        hf_token=hf_token,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
    n_ctx: int | Unset = UNSET,
    hf_token: str | Unset = UNSET,
) -> Any | ApiErrorBody | None:
    r"""`GET /api/v1/llm/recommend?limit=...&n_ctx=...`

     Ranks the models this machine should run: plans against the embedded
    generation table, resolves the survivors on HuggingFace, reads each
    finalist's GGUF header over a range request, and orders what is left.

    Answers `503` when the Hub cannot be reached, which is deliberately not the
    same as an empty `200`. The catalogue is fetched live, so no network means
    no catalogue; reporting that as \"no models fit\" would be a different and
    false claim about the operator's machine.

    Args:
        limit (int | Unset):
        n_ctx (int | Unset):
        hf_token (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return sync_detailed(
        client=client,
        limit=limit,
        n_ctx=n_ctx,
        hf_token=hf_token,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
    n_ctx: int | Unset = UNSET,
    hf_token: str | Unset = UNSET,
) -> Response[Any | ApiErrorBody]:
    r"""`GET /api/v1/llm/recommend?limit=...&n_ctx=...`

     Ranks the models this machine should run: plans against the embedded
    generation table, resolves the survivors on HuggingFace, reads each
    finalist's GGUF header over a range request, and orders what is left.

    Answers `503` when the Hub cannot be reached, which is deliberately not the
    same as an empty `200`. The catalogue is fetched live, so no network means
    no catalogue; reporting that as \"no models fit\" would be a different and
    false claim about the operator's machine.

    Args:
        limit (int | Unset):
        n_ctx (int | Unset):
        hf_token (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiErrorBody]
    """

    kwargs = _get_kwargs(
        limit=limit,
        n_ctx=n_ctx,
        hf_token=hf_token,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
    n_ctx: int | Unset = UNSET,
    hf_token: str | Unset = UNSET,
) -> Any | ApiErrorBody | None:
    r"""`GET /api/v1/llm/recommend?limit=...&n_ctx=...`

     Ranks the models this machine should run: plans against the embedded
    generation table, resolves the survivors on HuggingFace, reads each
    finalist's GGUF header over a range request, and orders what is left.

    Answers `503` when the Hub cannot be reached, which is deliberately not the
    same as an empty `200`. The catalogue is fetched live, so no network means
    no catalogue; reporting that as \"no models fit\" would be a different and
    false claim about the operator's machine.

    Args:
        limit (int | Unset):
        n_ctx (int | Unset):
        hf_token (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiErrorBody
    """

    return (
        await asyncio_detailed(
            client=client,
            limit=limit,
            n_ctx=n_ctx,
            hf_token=hf_token,
        )
    ).parsed
