from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.audit_journal_page_response import AuditJournalPageResponse
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    limit: int | Unset = UNSET,
    offset: int | Unset = UNSET,
    agents: str | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["limit"] = limit

    params["offset"] = offset

    params["agents"] = agents

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/audit/journal",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | AuditJournalPageResponse | None:
    if response.status_code == 200:
        response_200 = AuditJournalPageResponse.from_dict(response.json())

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
) -> Response[ApiErrorBody | AuditJournalPageResponse]:
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
    offset: int | Unset = UNSET,
    agents: str | Unset = UNSET,
) -> Response[ApiErrorBody | AuditJournalPageResponse]:
    """`GET /api/v1/audit/journal?limit=N&offset=M`, a page of the chained journal
    across every run.

     This is the only read of the journal that does not need a run id up front:
    `GET /api/v1/audit/journal/{run_id}` answers a run the caller already knows,
    and `GET /api/v1/audit` answers the separate tool-invocation trail. 503 when
    the journal is not configured.

    Args:
        limit (int | Unset):
        offset (int | Unset):
        agents (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | AuditJournalPageResponse]
    """

    kwargs = _get_kwargs(
        limit=limit,
        offset=offset,
        agents=agents,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
    offset: int | Unset = UNSET,
    agents: str | Unset = UNSET,
) -> ApiErrorBody | AuditJournalPageResponse | None:
    """`GET /api/v1/audit/journal?limit=N&offset=M`, a page of the chained journal
    across every run.

     This is the only read of the journal that does not need a run id up front:
    `GET /api/v1/audit/journal/{run_id}` answers a run the caller already knows,
    and `GET /api/v1/audit` answers the separate tool-invocation trail. 503 when
    the journal is not configured.

    Args:
        limit (int | Unset):
        offset (int | Unset):
        agents (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | AuditJournalPageResponse
    """

    return sync_detailed(
        client=client,
        limit=limit,
        offset=offset,
        agents=agents,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
    offset: int | Unset = UNSET,
    agents: str | Unset = UNSET,
) -> Response[ApiErrorBody | AuditJournalPageResponse]:
    """`GET /api/v1/audit/journal?limit=N&offset=M`, a page of the chained journal
    across every run.

     This is the only read of the journal that does not need a run id up front:
    `GET /api/v1/audit/journal/{run_id}` answers a run the caller already knows,
    and `GET /api/v1/audit` answers the separate tool-invocation trail. 503 when
    the journal is not configured.

    Args:
        limit (int | Unset):
        offset (int | Unset):
        agents (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | AuditJournalPageResponse]
    """

    kwargs = _get_kwargs(
        limit=limit,
        offset=offset,
        agents=agents,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
    offset: int | Unset = UNSET,
    agents: str | Unset = UNSET,
) -> ApiErrorBody | AuditJournalPageResponse | None:
    """`GET /api/v1/audit/journal?limit=N&offset=M`, a page of the chained journal
    across every run.

     This is the only read of the journal that does not need a run id up front:
    `GET /api/v1/audit/journal/{run_id}` answers a run the caller already knows,
    and `GET /api/v1/audit` answers the separate tool-invocation trail. 503 when
    the journal is not configured.

    Args:
        limit (int | Unset):
        offset (int | Unset):
        agents (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | AuditJournalPageResponse
    """

    return (
        await asyncio_detailed(
            client=client,
            limit=limit,
            offset=offset,
            agents=agents,
        )
    ).parsed
