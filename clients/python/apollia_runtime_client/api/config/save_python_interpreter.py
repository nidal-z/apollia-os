from http import HTTPStatus
from typing import Any

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.config_error_body import ConfigErrorBody
from ...models.python_interpreter_request import PythonInterpreterRequest
from ...models.python_interpreter_setting import PythonInterpreterSetting
from ...types import Response


def _get_kwargs(
    *,
    body: PythonInterpreterRequest,
) -> dict[str, Any]:
    headers: dict[str, Any] = {}

    _kwargs: dict[str, Any] = {
        "method": "put",
        "url": "/api/v1/config/python-interpreter",
    }

    _kwargs["json"] = body.to_dict()

    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> ConfigErrorBody | PythonInterpreterSetting | None:
    if response.status_code == 200:
        response_200 = PythonInterpreterSetting.from_dict(response.json())

        return response_200

    if response.status_code == 403:
        response_403 = ConfigErrorBody.from_dict(response.json())

        return response_403

    if response.status_code == 422:
        response_422 = ConfigErrorBody.from_dict(response.json())

        return response_422

    if response.status_code == 500:
        response_500 = ConfigErrorBody.from_dict(response.json())

        return response_500

    if response.status_code == 503:
        response_503 = ConfigErrorBody.from_dict(response.json())

        return response_503

    if client.raise_on_unexpected_status:
        raise errors.UnexpectedStatus(response.status_code, response.content)
    else:
        return None


def _build_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Response[ConfigErrorBody | PythonInterpreterSetting]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: PythonInterpreterRequest,
) -> Response[ConfigErrorBody | PythonInterpreterSetting]:
    """`PUT /api/v1/config/python-interpreter`: chooses an interpreter, checked
    before it is written (it exists, starts, and has the minor version of the
    bundled standard library), or returns to the bundled one. Applies to the
    agents started after it.

    Args:
        body (PythonInterpreterRequest): The interpreter to use: an absolute path, or none to
            return to the bundled
            one.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ConfigErrorBody | PythonInterpreterSetting]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)


def sync(
    *,
    client: AuthenticatedClient | Client,
    body: PythonInterpreterRequest,
) -> ConfigErrorBody | PythonInterpreterSetting | None:
    """`PUT /api/v1/config/python-interpreter`: chooses an interpreter, checked
    before it is written (it exists, starts, and has the minor version of the
    bundled standard library), or returns to the bundled one. Applies to the
    agents started after it.

    Args:
        body (PythonInterpreterRequest): The interpreter to use: an absolute path, or none to
            return to the bundled
            one.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ConfigErrorBody | PythonInterpreterSetting
    """

    return sync_detailed(
        client=client,
        body=body,
    ).parsed


async def asyncio_detailed(
    *,
    client: AuthenticatedClient | Client,
    body: PythonInterpreterRequest,
) -> Response[ConfigErrorBody | PythonInterpreterSetting]:
    """`PUT /api/v1/config/python-interpreter`: chooses an interpreter, checked
    before it is written (it exists, starts, and has the minor version of the
    bundled standard library), or returns to the bundled one. Applies to the
    agents started after it.

    Args:
        body (PythonInterpreterRequest): The interpreter to use: an absolute path, or none to
            return to the bundled
            one.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ConfigErrorBody | PythonInterpreterSetting]
    """

    kwargs = _get_kwargs(
        body=body,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    *,
    client: AuthenticatedClient | Client,
    body: PythonInterpreterRequest,
) -> ConfigErrorBody | PythonInterpreterSetting | None:
    """`PUT /api/v1/config/python-interpreter`: chooses an interpreter, checked
    before it is written (it exists, starts, and has the minor version of the
    bundled standard library), or returns to the bundled one. Applies to the
    agents started after it.

    Args:
        body (PythonInterpreterRequest): The interpreter to use: an absolute path, or none to
            return to the bundled
            one.

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ConfigErrorBody | PythonInterpreterSetting
    """

    return (
        await asyncio_detailed(
            client=client,
            body=body,
        )
    ).parsed
