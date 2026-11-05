from http import HTTPStatus
from typing import Any, cast
from urllib.parse import quote

import httpx

from ... import errors
from ...client import AuthenticatedClient, Client
from ...models.engine_removal_error_body import EngineRemovalErrorBody
from ...types import UNSET, Response, Unset


def _get_kwargs(
    engine: str,
    tag: str,
    variant: str,
    *,
    confirm: bool | Unset = UNSET,
) -> dict[str, Any]:

    params: dict[str, Any] = {}

    params["confirm"] = confirm

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    _kwargs: dict[str, Any] = {
        "method": "delete",
        "url": "/api/v1/engines/{engine}/{tag}/{variant}".format(
            engine=quote(str(engine), safe=""),
            tag=quote(str(tag), safe=""),
            variant=quote(str(variant), safe=""),
        ),
        "params": params,
    }

    return _kwargs


def _parse_response(
    *, client: AuthenticatedClient | Client, response: httpx.Response
) -> Any | EngineRemovalErrorBody | None:
    if response.status_code == 204:
        response_204 = cast(Any, None)
        return response_204

    if response.status_code == 403:
        response_403 = EngineRemovalErrorBody.from_dict(response.json())

        return response_403

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
    confirm: bool | Unset = UNSET,
) -> Response[Any | EngineRemovalErrorBody]:
    """`DELETE /api/v1/engines/{engine}/{tag}/{variant}`: removes one installed
    build of the engine store.

     The embedded engine has no tag of its own: `GET /api/v1/engines` lists it
    with `tag` null, and this route addresses it by the reserved tag
    `embedded`, always to refuse it (`embedded`), confirmation or not.

    The build `engines.gguf` names, which no process runs, is refused without
    `confirm=true` (`chosen`). With it the build goes; when no other build of
    the chosen variant is left, `engines.gguf` is first written back to
    `auto`, and the `engine.removed` event says so (`choice_reset`). A build a
    process runs stays refused (`in_service`) whatever the confirmation.

    Args:
        engine (str):
        tag (str):
        variant (str):
        confirm (bool | Unset):

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
        confirm=confirm,
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
    confirm: bool | Unset = UNSET,
) -> Any | EngineRemovalErrorBody | None:
    """`DELETE /api/v1/engines/{engine}/{tag}/{variant}`: removes one installed
    build of the engine store.

     The embedded engine has no tag of its own: `GET /api/v1/engines` lists it
    with `tag` null, and this route addresses it by the reserved tag
    `embedded`, always to refuse it (`embedded`), confirmation or not.

    The build `engines.gguf` names, which no process runs, is refused without
    `confirm=true` (`chosen`). With it the build goes; when no other build of
    the chosen variant is left, `engines.gguf` is first written back to
    `auto`, and the `engine.removed` event says so (`choice_reset`). A build a
    process runs stays refused (`in_service`) whatever the confirmation.

    Args:
        engine (str):
        tag (str):
        variant (str):
        confirm (bool | Unset):

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
        confirm=confirm,
    ).parsed


async def asyncio_detailed(
    engine: str,
    tag: str,
    variant: str,
    *,
    client: AuthenticatedClient | Client,
    confirm: bool | Unset = UNSET,
) -> Response[Any | EngineRemovalErrorBody]:
    """`DELETE /api/v1/engines/{engine}/{tag}/{variant}`: removes one installed
    build of the engine store.

     The embedded engine has no tag of its own: `GET /api/v1/engines` lists it
    with `tag` null, and this route addresses it by the reserved tag
    `embedded`, always to refuse it (`embedded`), confirmation or not.

    The build `engines.gguf` names, which no process runs, is refused without
    `confirm=true` (`chosen`). With it the build goes; when no other build of
    the chosen variant is left, `engines.gguf` is first written back to
    `auto`, and the `engine.removed` event says so (`choice_reset`). A build a
    process runs stays refused (`in_service`) whatever the confirmation.

    Args:
        engine (str):
        tag (str):
        variant (str):
        confirm (bool | Unset):

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
        confirm=confirm,
    )

    response = await client.get_async_httpx_client().request(**kwargs)

    return _build_response(client=client, response=response)


async def asyncio(
    engine: str,
    tag: str,
    variant: str,
    *,
    client: AuthenticatedClient | Client,
    confirm: bool | Unset = UNSET,
) -> Any | EngineRemovalErrorBody | None:
    """`DELETE /api/v1/engines/{engine}/{tag}/{variant}`: removes one installed
    build of the engine store.

     The embedded engine has no tag of its own: `GET /api/v1/engines` lists it
    with `tag` null, and this route addresses it by the reserved tag
    `embedded`, always to refuse it (`embedded`), confirmation or not.

    The build `engines.gguf` names, which no process runs, is refused without
    `confirm=true` (`chosen`). With it the build goes; when no other build of
    the chosen variant is left, `engines.gguf` is first written back to
    `auto`, and the `engine.removed` event says so (`choice_reset`). A build a
    process runs stays refused (`in_service`) whatever the confirmation.

    Args:
        engine (str):
        tag (str):
        variant (str):
        confirm (bool | Unset):

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
            confirm=confirm,
        )
    ).parsed
