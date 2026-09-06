"""seed-planner, seed orchestrated agent for the plan-gate automation book.

A minimal but VALID Apollia orchestrated agent: the boot auto-loader imports it
and requires the ``@agent`` decorator (``__apollia_manifest__`` and
``__apollia_dispatch__``), and ``@agent`` refuses a class with no handler, so
``@orchestrated`` is the handler here. It declares no skill and no
``on_message``: ORIA plans and executes every task itself, and the plan waits
at the approval gate the book drives. The system prompt pins the plan shape
(three linear reasoning steps, no tool) so an approved run executes LLM steps
only. The same execution_mode and system_prompt sit in the agents.db row the
backend reads (tests/cli/seed/planner/fragments/agents.sql).
"""

from apollia import agent, orchestrated

SYSTEM_PROMPT = (
    "You are the seed planner of the Apollia automation suite. Every plan you "
    "produce has exactly three steps, s1, s2 and s3, each a pure reasoning step "
    'with tool_hint "llm" and no args. s2 depends on s1 and s3 depends on s2. '
    "Each description is one short sentence. Never call a tool."
)


@agent(
    name="seed-planner",
    version="0.1.0",
    description="Orchestrated planner worker.",
    tags=("worker", "planner", "seed"),
    memory_namespace="seed-planner",
    agent_type="worker",
)
@orchestrated(system_prompt=SYSTEM_PROMPT)
class SeedPlanner:
    """Metadata only: the execution loop belongs to ORIA."""


agent = SeedPlanner()
