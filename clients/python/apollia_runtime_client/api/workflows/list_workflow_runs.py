from http import HTTPStatus
from typing import Any
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.workflow_error_response import WorkflowErrorResponse
from ...models.workflow_run_list_view import WorkflowRunListView
from ...types import UNSET, Response, Unset


def _get_kwargs(
    package: str,
    workflow: str,
    *,
    limit: int | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["limit"] = limit

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "get",
        "url": "/api/v1/workflows/{package}/{workflow}/runs".format(
            package=quote(str(package), safe=""),
            workflow=quote(str(workflow), safe=""),
        ),
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> WorkflowErrorResponse | WorkflowRunListView | None:
    if response.status_code == 200:
        response_200 = WorkflowRunListView.from_dict(response.json())

        return response_200

    if response.status_code == 503:
        response_503 = WorkflowErrorResponse.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[WorkflowErrorResponse | WorkflowRunListView]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    package: str,
    workflow: str,
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
) -> Response[WorkflowErrorResponse | WorkflowRunListView]:
    """`GET /api/v1/workflows/{package}/{workflow}/runs`: the history of a
    workflow, newest first.

    Args:
        package (str):
        workflow (str):
        limit (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[WorkflowErrorResponse | WorkflowRunListView]
    """

    kwargs = _get_kwargs(
        package=package,
        workflow=workflow,
        limit=limit,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    package: str,
    workflow: str,
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
) -> WorkflowErrorResponse | WorkflowRunListView | None:
    """`GET /api/v1/workflows/{package}/{workflow}/runs`: the history of a
    workflow, newest first.

    Args:
        package (str):
        workflow (str):
        limit (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        WorkflowErrorResponse | WorkflowRunListView
    """

    return sync_detailed(
        package=package,
        workflow=workflow,
        client=client,
        limit=limit,
    ).parsed


async def asyncio_detailed(
    package: str,
    workflow: str,
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
) -> Response[WorkflowErrorResponse | WorkflowRunListView]:
    """`GET /api/v1/workflows/{package}/{workflow}/runs`: the history of a
    workflow, newest first.

    Args:
        package (str):
        workflow (str):
        limit (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[WorkflowErrorResponse | WorkflowRunListView]
    """

    kwargs = _get_kwargs(
        package=package,
        workflow=workflow,
        limit=limit,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    package: str,
    workflow: str,
    *,
    client: AuthenticatedClient | Client,
    limit: int | Unset = UNSET,
) -> WorkflowErrorResponse | WorkflowRunListView | None:
    """`GET /api/v1/workflows/{package}/{workflow}/runs`: the history of a
    workflow, newest first.

    Args:
        package (str):
        workflow (str):
        limit (int | Unset):

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        WorkflowErrorResponse | WorkflowRunListView
    """

    return (
        await asyncio_detailed(
            package=package,
            workflow=workflow,
            client=client,
            limit=limit,
        )
    ).parsed
