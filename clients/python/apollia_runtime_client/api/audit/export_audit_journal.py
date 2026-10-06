from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.api_error_body import ApiErrorBody
from ...models.journal_export_format import JournalExportFormat
from ...types import UNSET, Response, Unset


def _get_kwargs(
    *,
    format_: JournalExportFormat,
    detail: bool | Unset = UNSET,
    agents: str | Unset = UNSET,
    family: str | Unset = UNSET,
    nature: str | Unset = UNSET,
    result: str | Unset = UNSET,
    since: str | Unset = UNSET,
    until: str | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    json_format_ = format_.value
    params["format"] = json_format_

    params["detail"] = detail

    params["agents"] = agents

    params["family"] = family

    params["nature"] = nature

    params["result"] = result

    params["since"] = since

    params["until"] = until

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/audit/export",
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ApiErrorBody | str | None:
    if response.status_code == 200:
        response_200 = response.text
        return response_200

    if response.status_code == 400:
        response_400 = ApiErrorBody.from_dict(response.json())

        return response_400

    if response.status_code == 403:
        response_403 = ApiErrorBody.from_dict(response.json())

        return response_403

    if response.status_code == 503:
        response_503 = ApiErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ApiErrorBody | str]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    format_: JournalExportFormat,
    detail: bool | Unset = UNSET,
    agents: str | Unset = UNSET,
    family: str | Unset = UNSET,
    nature: str | Unset = UNSET,
    result: str | Unset = UNSET,
    since: str | Unset = UNSET,
    until: str | Unset = UNSET,
) -> Response[ApiErrorBody | str]:
    """`GET /api/v1/audit/export`: the journal as a file, narrowed by the filters
    of `GET /api/v1/audit/journal`.

     Every entry that passes the filters is in the file, newest first: the
    export is not paged. Each entry is its summary (when, who, what, on what,
    with which result) and its place in the chain (run, sequence, kind, hash),
    without content. With `detail=true` each entry also holds its payload and,
    for a tool call, its arguments and its output. `csv` is UTF-8 with one
    header row, `json` is one document with the entries. The file is offered
    as `journal.<format>`, and the export is recorded in the journal. 400 on a
    format or a filter value that cannot be read, 403 when the organisation
    policy forbids the detail and it is asked, or requires it and it is not,
    503 when the journal is not configured.

    Args:
        format_ (JournalExportFormat): A file format the journal is exported in.
        detail (bool | Unset):
        agents (str | Unset):
        family (str | Unset):
        nature (str | Unset):
        result (str | Unset):
        since (str | Unset):
        until (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | str]
    """

    kwargs = _get_kwargs(
        format_=format_,
        detail=detail,
        agents=agents,
        family=family,
        nature=nature,
        result=result,
        since=since,
        until=until,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    format_: JournalExportFormat,
    detail: bool | Unset = UNSET,
    agents: str | Unset = UNSET,
    family: str | Unset = UNSET,
    nature: str | Unset = UNSET,
    result: str | Unset = UNSET,
    since: str | Unset = UNSET,
    until: str | Unset = UNSET,
) -> ApiErrorBody | str | None:
    """`GET /api/v1/audit/export`: the journal as a file, narrowed by the filters
    of `GET /api/v1/audit/journal`.

     Every entry that passes the filters is in the file, newest first: the
    export is not paged. Each entry is its summary (when, who, what, on what,
    with which result) and its place in the chain (run, sequence, kind, hash),
    without content. With `detail=true` each entry also holds its payload and,
    for a tool call, its arguments and its output. `csv` is UTF-8 with one
    header row, `json` is one document with the entries. The file is offered
    as `journal.<format>`, and the export is recorded in the journal. 400 on a
    format or a filter value that cannot be read, 403 when the organisation
    policy forbids the detail and it is asked, or requires it and it is not,
    503 when the journal is not configured.

    Args:
        format_ (JournalExportFormat): A file format the journal is exported in.
        detail (bool | Unset):
        agents (str | Unset):
        family (str | Unset):
        nature (str | Unset):
        result (str | Unset):
        since (str | Unset):
        until (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | str
    """

    return sync_detailed(
        client=client,
        format_=format_,
        detail=detail,
        agents=agents,
        family=family,
        nature=nature,
        result=result,
        since=since,
        until=until,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    format_: JournalExportFormat,
    detail: bool | Unset = UNSET,
    agents: str | Unset = UNSET,
    family: str | Unset = UNSET,
    nature: str | Unset = UNSET,
    result: str | Unset = UNSET,
    since: str | Unset = UNSET,
    until: str | Unset = UNSET,
) -> Response[ApiErrorBody | str]:
    """`GET /api/v1/audit/export`: the journal as a file, narrowed by the filters
    of `GET /api/v1/audit/journal`.

     Every entry that passes the filters is in the file, newest first: the
    export is not paged. Each entry is its summary (when, who, what, on what,
    with which result) and its place in the chain (run, sequence, kind, hash),
    without content. With `detail=true` each entry also holds its payload and,
    for a tool call, its arguments and its output. `csv` is UTF-8 with one
    header row, `json` is one document with the entries. The file is offered
    as `journal.<format>`, and the export is recorded in the journal. 400 on a
    format or a filter value that cannot be read, 403 when the organisation
    policy forbids the detail and it is asked, or requires it and it is not,
    503 when the journal is not configured.

    Args:
        format_ (JournalExportFormat): A file format the journal is exported in.
        detail (bool | Unset):
        agents (str | Unset):
        family (str | Unset):
        nature (str | Unset):
        result (str | Unset):
        since (str | Unset):
        until (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiErrorBody | str]
    """

    kwargs = _get_kwargs(
        format_=format_,
        detail=detail,
        agents=agents,
        family=family,
        nature=nature,
        result=result,
        since=since,
        until=until,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    format_: JournalExportFormat,
    detail: bool | Unset = UNSET,
    agents: str | Unset = UNSET,
    family: str | Unset = UNSET,
    nature: str | Unset = UNSET,
    result: str | Unset = UNSET,
    since: str | Unset = UNSET,
    until: str | Unset = UNSET,
) -> ApiErrorBody | str | None:
    """`GET /api/v1/audit/export`: the journal as a file, narrowed by the filters
    of `GET /api/v1/audit/journal`.

     Every entry that passes the filters is in the file, newest first: the
    export is not paged. Each entry is its summary (when, who, what, on what,
    with which result) and its place in the chain (run, sequence, kind, hash),
    without content. With `detail=true` each entry also holds its payload and,
    for a tool call, its arguments and its output. `csv` is UTF-8 with one
    header row, `json` is one document with the entries. The file is offered
    as `journal.<format>`, and the export is recorded in the journal. 400 on a
    format or a filter value that cannot be read, 403 when the organisation
    policy forbids the detail and it is asked, or requires it and it is not,
    503 when the journal is not configured.

    Args:
        format_ (JournalExportFormat): A file format the journal is exported in.
        detail (bool | Unset):
        agents (str | Unset):
        family (str | Unset):
        nature (str | Unset):
        result (str | Unset):
        since (str | Unset):
        until (str | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiErrorBody | str
    """

    return (
        await asyncio_detailed(
            client=client,
            format_=format_,
            detail=detail,
            agents=agents,
            family=family,
            nature=nature,
            result=result,
            since=since,
            until=until,
        )
    ).parsed
