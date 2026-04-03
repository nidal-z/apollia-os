"""Unit tests for browser-worker agent."""

from __future__ import annotations

import importlib.util
import pathlib
import sys
from typing import Any

_spec = importlib.util.spec_from_file_location(
    "browser_worker",
    pathlib.Path(__file__).parent / "browser-worker.py",
)
assert _spec is not None and _spec.loader is not None
_module = importlib.util.module_from_spec(_spec)
sys.modules["browser_worker"] = _module
_spec.loader.exec_module(_module)  # type: ignore[union-attr]

from browser_worker import agent, manifest  # noqa: E402


def test_manifest_contains_required_fields() -> None:
    # GIVEN browser-worker is imported
    # WHEN manifest() is called
    m = manifest()
    # THEN it contains all required AIP fields
    assert m["name"] == "browser-worker"
    assert m["version"] == "0.1.0"
    assert isinstance(m["description"], str) and m["description"]
    assert m["supports_a2a"] is True


def test_manifest_declares_python_executor_tool() -> None:
    # GIVEN browser-worker manifest
    # WHEN tools_required is inspected
    m = manifest()
    # THEN python_executor is listed
    assert "python_executor" in m["tools_required"]


def test_manifest_declares_network_restricted_sandbox() -> None:
    # GIVEN browser-worker manifest
    # WHEN sandbox_profile is inspected
    m = manifest()
    # THEN the sandbox profile is NetworkRestricted
    assert m.get("sandbox_profile") == "NetworkRestricted"


def test_manifest_declares_expected_skills() -> None:
    # GIVEN browser-worker manifest
    # WHEN skills are inspected
    m = manifest()
    skill_ids = [s["id"] for s in m["skills"]]
    # THEN both browse-url and screenshot-url are declared
    assert "browse-url" in skill_ids
    assert "screenshot-url" in skill_ids


def test_manifest_skills_have_descriptions() -> None:
    # GIVEN browser-worker manifest
    # WHEN each skill is inspected
    m = manifest()
    # THEN every skill has a non-empty name and description
    for skill in m["skills"]:
        assert skill.get("name"), f"Skill {skill.get('id')} missing name"
        assert skill.get("description"), f"Skill {skill.get('id')} missing description"


def test_manifest_no_tools_requiring_approval() -> None:
    # GIVEN browser-worker manifest
    # WHEN tools_requiring_approval is inspected
    m = manifest()
    # THEN no tools require approval (browser-worker is read-only)
    assert m.get("tools_requiring_approval", []) == []


def test_manifest_packages_include_required_deps() -> None:
    # GIVEN browser-worker manifest
    # WHEN packages is inspected
    m = manifest()
    packages = m.get("packages", [])
    # THEN requests and beautifulsoup4 are listed
    assert "requests" in packages
    assert "beautifulsoup4" in packages


def test_agent_instance_has_correct_manifest_name() -> None:
    # GIVEN the module-level agent instance
    # WHEN manifest() is called on the instance
    m: dict[str, Any] = agent.manifest()
    # THEN the name matches the expected worker name
    assert m["name"] == "browser-worker"
