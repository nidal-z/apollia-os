"""Mock implementations of all ``ctx.*`` services for unit testing agents.

Each ``Mock<X>`` mirrors the corresponding Protocol from
:mod:`apollia.context` and adds tracking attributes (``*_calls`` lists,
queues, ``values`` dicts) so tests can drive scenarios and assert what
happened.

These mocks intentionally implement only the surface methods agents use
at runtime.  They are *not* full duck-types — calling a method that has
not been implemented will raise :class:`AttributeError`, which keeps the
test surface small and intentional.
"""

from __future__ import annotations

from typing import Any

# NOTE on `# NOSONAR S7503` / `# NOSONAR S1172` markers below:
# Every mock here implements an async Protocol defined in
# `apollia.context.*`. The runtime call sites use `await`, so the
# `async` keyword must be preserved even when the body has no `await`.
# Likewise, parameters such as `timeout_secs` are part of the Protocol
# signature (callers pass them by keyword) and cannot be renamed or
# removed.


# ctx.a2a
class MockA2A:
    """In-memory mock of the ``A2AProxy`` Protocol.

    Pre-configure via attributes:
        ``invoke_responses``: ``{skill_id: response_dict}`` returned by
            :meth:`invoke`.  Default response is
            ``{"status": "completed"}``.
        ``skill_cards``: ``{skill_id: card_dict}`` returned by
            :meth:`discover` / used by :meth:`skill_as_tool`.
        ``skills_list``: list returned by :meth:`list_skills`.

    Track via attributes:
        ``invoke_calls``: ordered list of ``(skill_id, merged_input)``.
    """

    def __init__(self) -> None:
        self.invoke_calls: list[tuple[str, dict[str, Any]]] = []
        self.invoke_responses: dict[str, dict[str, Any]] = {}
        self.skill_cards: dict[str, dict[str, Any]] = {}
        self.skills_list: list[dict[str, Any]] = []

    async def invoke(  # NOSONAR S7503,S1172 — Protocol contract
        self,
        skill_id: str,
        input: dict[str, Any] | None = None,
        *,
        timeout_secs: int = 120,
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Record the call and return the configured response.

        ``input`` and ``**kwargs`` are merged (kwargs first, then input
        overrides) so tests can use either calling convention.
        """
        merged: dict[str, Any] = dict(kwargs)
        if input:
            merged.update(input)
        self.invoke_calls.append((skill_id, merged))
        return self.invoke_responses.get(skill_id, {"status": "completed"})

    async def discover(self, skill_id: str) -> dict[str, Any] | None:  # NOSONAR S7503
        return self.skill_cards.get(skill_id)

    async def list_skills(self) -> list[dict[str, Any]]:  # NOSONAR S7503
        return list(self.skills_list)

    async def skill_as_tool(self, skill_id: str) -> dict[str, Any]:  # NOSONAR S7503
        card = self.skill_cards.get(skill_id, {})
        return {
            "name": f"a2a__{skill_id.replace('.', '__')}",
            "description": card.get("description", ""),
            "input_schema": card.get("input_schema", {}),
        }


# ctx.datasources
class MockDatasources:
    """In-memory mock of the ``Datasources`` Protocol.

    Pre-configure ``values`` with ``{name: loaded_value}``.  Calling
    :meth:`get` on an unknown name raises :class:`FileNotFoundError`,
    mirroring the real loader's behaviour.
    """

    def __init__(self) -> None:
        self.values: dict[str, Any] = {}

    def get(self, name: str) -> Any:
        if name not in self.values:
            raise FileNotFoundError(
                f"Datasource '{name}' not configured in mock"
            )
        return self.values[name]

    def list_names(self) -> list[str]:
        return list(self.values.keys())


# ctx.templates
class MockTemplates:
    """In-memory mock of the ``Templates`` Protocol.

    Pre-configure ``templates`` with ``{name: template_string}``.  The
    mock applies a minimal ``{{ var }}`` / ``{{var}}`` substitution
    instead of running a real Jinja2 engine — sufficient for unit tests
    that only care about which template was rendered with what context.

    Track via ``render_calls``: ordered list of ``(name, context_dict)``.
    """

    def __init__(self) -> None:
        self.templates: dict[str, str] = {}
        self.render_calls: list[tuple[str, dict[str, Any]]] = []

    def render(self, name: str, **context: Any) -> str:
        self.render_calls.append((name, dict(context)))
        if name not in self.templates:
            raise FileNotFoundError(
                f"Template '{name}' not configured in mock"
            )
        result = self.templates[name]
        for key, value in context.items():
            result = result.replace(f"{{{{ {key} }}}}", str(value))
            result = result.replace(f"{{{{{key}}}}}", str(value))
        return result

    def list_names(self) -> list[str]:
        return list(self.templates.keys())


# ctx.secrets
class MockSecrets:
    """In-memory mock of the ``Secrets`` Protocol.

    Pre-configure ``values`` with ``{key: secret_value}``.
    """

    def __init__(self) -> None:
        self.values: dict[str, str] = {}

    def get(self, key: str) -> str | None:
        return self.values.get(key)

    def has(self, key: str) -> bool:
        return key in self.values


# ctx.events
class MockEvents:
    """In-memory mock of the ``Events`` Protocol.

    Every emit method records to a list for assertion in tests.
    """

    def __init__(self) -> None:
        self.tokens: list[str] = []
        self.thoughts: list[tuple[str, int]] = []
        self.retries: list[tuple[int, str, int]] = []
        self.action_parse_errors: list[tuple[int, str, bool]] = []

    def emit_token(self, delta: str) -> None:
        self.tokens.append(delta)

    def emit_thought(self, text: str, *, step: int) -> None:
        self.thoughts.append((text, step))

    def emit_retry(self, *, step: int, reason: str, count: int) -> None:
        self.retries.append((step, reason, count))

    def emit_action_parse_error(
        self, *, step: int, raw: str, fatal: bool = False
    ) -> None:
        self.action_parse_errors.append((step, raw, fatal))


# ctx.profile
class MockProfile:
    """In-memory mock of the ``Profile`` Protocol.

    The default profile is read-only (``writable=False``); pass
    ``writable=True`` to allow :meth:`set` / :meth:`update`.
    """

    def __init__(
        self,
        writable: bool = False,
        initial: dict[str, str] | None = None,
    ) -> None:
        self._writable = writable
        self._values: dict[str, str] = dict(initial or {})

    @property
    def writable(self) -> bool:
        return self._writable

    async def get(self, key: str) -> str | None:  # NOSONAR S7503
        return self._values.get(key)

    async def has(self, key: str) -> bool:  # NOSONAR S7503
        return key in self._values

    async def all(self) -> dict[str, str]:  # NOSONAR S7503
        return dict(self._values)

    def schema_keys(self) -> list[str]:
        return list(self._values.keys())

    async def set(self, key: str, value: str) -> None:  # NOSONAR S7503
        if not self._writable:
            raise RuntimeError("MockProfile is read-only (writable=False)")
        self._values[key] = value

    async def update(self, entries: dict[str, str]) -> None:  # NOSONAR S7503
        if not self._writable:
            raise RuntimeError("MockProfile is read-only (writable=False)")
        self._values.update(entries)


# ctx.workspace
class MockWorkspace:
    """In-memory mock of the ``Workspace`` Protocol.

    Pre-configure ``rules_content`` to expose ``ctx.workspace.rules`` /
    ``ctx.workspace.apollia_md``.  Pre-configure ``section_map`` with
    ``{title: section_content}`` for :meth:`get` and :attr:`sections`.
    """

    def __init__(self) -> None:
        self.rules_content: str | None = None
        self.section_map: dict[str, str] = {}

    @property
    def rules(self) -> str | None:
        return self.rules_content

    @property
    def apollia_md(self) -> str | None:
        return self.rules_content

    def get(self, title: str) -> str | None:
        return self.section_map.get(title)

    @property
    def sections(self) -> list[dict[str, str]]:
        return [
            {"title": k, "content": v}
            for k, v in self.section_map.items()
        ]


# ctx.stt
class MockStt:
    """In-memory mock of the ``Stt`` Protocol.

    Queue responses via ``transcribe_responses``; calls drain the queue
    FIFO.  Track via ``transcribe_calls``.
    """

    def __init__(self) -> None:
        self.transcribe_calls: list[tuple[str, str | None, str | None]] = []
        self.transcribe_responses: list[str] = []

    async def transcribe(  # NOSONAR S7503 — Protocol contract
        self,
        path: str,
        *,
        language: str | None = None,
        backend: str | None = None,
    ) -> str:
        self.transcribe_calls.append((path, language, backend))
        if self.transcribe_responses:
            return self.transcribe_responses.pop(0)
        return ""

    async def status(self) -> dict[str, Any]:  # NOSONAR S7503
        return {"enabled": True, "model": "mock", "language": None}


# ctx.notify
class MockNotify:
    """In-memory mock of the ``Notify`` Protocol.

    Track via ``published``: ordered list of message dicts.
    """

    def __init__(self) -> None:
        self.published: list[dict[str, Any]] = []

    async def publish(  # NOSONAR S7503 — Protocol contract
        self,
        message: str,
        *,
        severity: str = "info",
        title: str | None = None,
        channel: str | None = None,
    ) -> None:
        self.published.append(
            {
                "message": message,
                "severity": severity,
                "title": title,
                "channel": channel,
            }
        )


# ctx.budget
class MockBudget:
    """In-memory mock of the ``Budget`` Protocol.

    All fields are public attributes — assign before calling the agent
    to simulate budget constraints.  Defaults imply "no limit".
    """

    def __init__(
        self,
        steps_remaining: int = -1,
        tool_calls_remaining: int = -1,
        elapsed_seconds: float = 0.0,
        wall_clock_remaining: float | None = None,
    ) -> None:
        self.steps_remaining = steps_remaining
        self.tool_calls_remaining = tool_calls_remaining
        self.elapsed_seconds = elapsed_seconds
        self.wall_clock_remaining = wall_clock_remaining


__all__ = [
    "MockA2A",
    "MockDatasources",
    "MockTemplates",
    "MockSecrets",
    "MockEvents",
    "MockProfile",
    "MockWorkspace",
    "MockStt",
    "MockNotify",
    "MockBudget",
]
