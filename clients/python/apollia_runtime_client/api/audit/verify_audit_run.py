from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.verify_chain_report import VerifyChainReport
from ...types import Response


def _get_kwargs(
    run_id: str,
) -> dict[str, Any]:

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/audit/verify/{run_id}".format(
            run_id=quote(str(run_id), safe=""),
        ),
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | VerifyChainReport | None:
    if response.status_code == 200:
        response_200 = VerifyChainReport.from_dict(response.json())

        return response_200

    if response.status_code == 404:
        response_404 = ApiErrorBody.from_dict(response.json())

        return response_404

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
) -> Response[ApiErrorBody | VerifyChainReport]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    run_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | VerifyChainReport]:
    """`GET /api/v1/audit/verify/:run_id`, verify a run's hash chain and signatures.

     Returns 200 with the [`VerifyChainReport`] (whether or not the chain is
    intact), 404 when the run has no entries, 503 when the journal is not
    configured, and 500 on an internal error.

    Args:
        run_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | VerifyChainReport]
    """

    kwargs = _get_kwargs(
        run_id=run_id,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    run_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | VerifyChainReport | None:
    """`GET /api/v1/audit/verify/:run_id`, verify a run's hash chain and signatures.

     Returns 200 with the [`VerifyChainReport`] (whether or not the chain is
    intact), 404 when the run has no entries, 503 when the journal is not
    configured, and 500 on an internal error.

    Args:
        run_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | VerifyChainReport
    """

    return sync_detailed(
        run_id=run_id,
        client=client,
    ).parsed


async def asyncio_detailed(
    run_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> Response[ApiErrorBody | VerifyChainReport]:
    """`GET /api/v1/audit/verify/:run_id`, verify a run's hash chain and signatures.

     Returns 200 with the [`VerifyChainReport`] (whether or not the chain is
    intact), 404 when the run has no entries, 503 when the journal is not
    configured, and 500 on an internal error.

    Args:
        run_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | VerifyChainReport]
    """

    kwargs = _get_kwargs(
        run_id=run_id,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    run_id: str,
    *,
    client: AuthenticatedClient | Client,
) -> ApiErrorBody | VerifyChainReport | None:
    """`GET /api/v1/audit/verify/:run_id`, verify a run's hash chain and signatures.

     Returns 200 with the [`VerifyChainReport`] (whether or not the chain is
    intact), 404 when the run has no entries, 503 when the journal is not
    configured, and 500 on an internal error.

    Args:
        run_id (str):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | VerifyChainReport
    """

    return (
        await asyncio_detailed(
            run_id=run_id,
            client=client,
        )
    ).parsed
