"""Tests for ``python -m apollia describe``."""

import shutil
import sys
from pathlib import Path

import pytest
from apollia.cli.__main__ import main
from apollia.cli.describe import GENERATED_MARKER, rewrite_manifest, toml_value
from apollia.errors import AgentConfigError

# The writer is checked by reading back; the standard reader is 3.11+.
tomllib = pytest.importorskip("tomllib")

FIXTURE = Path(__file__).parent / "fixtures" / "invoice_pkg"

# The keys apollia.toml accepts on a tool and on a workflow descriptor. The
# Rust reader refuses any other key, so the writer may not produce one.
TOOL_KEYS = {"name", "module", "description", "action", "inputs"}
WORKFLOW_KEYS = TOOL_KEYS | {"steps"}


@pytest.fixture
def package(tmp_path: Path) -> Path:
    target = tmp_path / "invoice_pkg"
    shutil.copytree(FIXTURE, target)
    return target


def test_describe_writes_tools_and_workflows_into_the_manifest(
    package: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    # GIVEN the example package, whose manifest holds a stale tool table
    # WHEN describe runs on its folder
    code = main(["describe", str(package)])

    # THEN it succeeds and reports what it wrote
    assert code == 0
    assert "2 tool(s), 1 workflow(s) described" in capsys.readouterr().out

    # AND the manifest carries the descriptors in the apollia.toml format
    manifest = tomllib.loads((package / "apollia.toml").read_text(encoding="utf-8"))
    assert [t["name"] for t in manifest["tools"]] == ["extract_totals", "file_totals"]
    assert all(set(t) == TOOL_KEYS for t in manifest["tools"])
    extract, file_totals = manifest["tools"]
    assert extract["module"] == "invoice_tools/tools.py"
    assert extract["action"] == "read"
    assert extract["inputs"]["properties"]["path"] == {
        "type": "string",
        "description": "path of the invoice",
    }
    assert file_totals["description"] == 'File totals in the "ledger" sheet.'
    assert file_totals["action"] == "write_or_delete"
    assert file_totals["inputs"]["required"] == ["totals"]

    (close_month,) = manifest["workflows"]
    assert set(close_month) == WORKFLOW_KEYS
    assert close_month["name"] == "close-month"
    assert close_month["module"] == "invoice_tools/flows.py"
    assert close_month["steps"] == ["extract", "file"]
    assert close_month["inputs"]["required"] == ["folder"]
    inputs = close_month["inputs"]["properties"]
    assert inputs["closing_day"] == {"type": "string", "format": "date"}
    assert inputs["note"] == {
        "type": "string",
        "format": "long-text",
        "description": "a note filed with the totals",
    }
    assert inputs["ledger"] == {"type": "string", "format": "file-path"}
    assert close_month["inputs"]["x-order"] == ["folder", "notify", "closing_day", "note", "ledger"]

    # AND every other table is kept as written, the stale tool gone
    assert manifest["package"]["name"] == "invoices"
    assert manifest["agents"][0]["id"] == "accountant"
    assert manifest["requires"] == [{"type": "gmail", "reason": "Send the monthly summary."}]
    assert 'name = "stale"' not in (package / "apollia.toml").read_text(encoding="utf-8")


def test_describe_output_is_the_fixture_the_rust_reader_parses(package: Path) -> None:
    # GIVEN the example package
    # WHEN describe runs on it
    assert main(["describe", str(package)]) == 0

    # THEN the manifest is byte for byte the fixture the apollia.toml reader
    # of the runtime parses in its own tests
    written = (package / "apollia.toml").read_text(encoding="utf-8")
    assert written == (FIXTURE.parent / "invoice_pkg_described.toml").read_text(encoding="utf-8")


def test_describe_keeps_comments_and_is_idempotent(package: Path) -> None:
    # GIVEN a package described once
    assert main(["describe", str(package)]) == 0
    first = (package / "apollia.toml").read_text(encoding="utf-8")

    # WHEN describe runs again
    assert main(["describe", str(package)]) == 0
    second = (package / "apollia.toml").read_text(encoding="utf-8")

    # THEN nothing moves, the author's comments are kept, the marker appears once
    assert second == first
    assert "# Connections the package needs." in second
    assert second.count(GENERATED_MARKER) == 1


def test_describe_never_imports_tests_and_leaves_no_module_behind(package: Path) -> None:
    # GIVEN a package whose tests folder raises on import
    assert (package / "tests" / "never_imported.py").is_file()

    # WHEN describe runs
    code = main(["describe", str(package)])

    # THEN it succeeds and the package's modules are not left in the interpreter
    assert code == 0
    assert not any(name.startswith("invoice_tools") for name in sys.modules)
    assert str(package.resolve()) not in sys.path


def test_describe_reads_a_folder_with_no_init_file(tmp_path: Path) -> None:
    # GIVEN a package whose tools sit in two modules of a folder with no __init__.py
    (tmp_path / "apollia.toml").write_text('[package]\nname = "ns"\n', encoding="utf-8")
    (tmp_path / "nstools").mkdir()
    for stem in ("first", "second"):
        (tmp_path / "nstools" / f"{stem}.py").write_text(
            "from typing import Any\n\n"
            "from apollia import tool\n\n\n"
            f'@tool("{stem}", "Tool {stem}.", action="read")\n'
            f"async def {stem}(ctx: Any) -> None:\n"
            "    return None\n",
            encoding="utf-8",
        )

    # WHEN describe runs
    code = main(["describe", str(tmp_path)])

    # THEN both tools are described
    assert code == 0
    manifest = tomllib.loads((tmp_path / "apollia.toml").read_text(encoding="utf-8"))
    assert [t["name"] for t in manifest["tools"]] == ["first", "second"]


def test_describe_refuses_a_folder_without_manifest(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    # GIVEN a folder with no apollia.toml
    # WHEN describe runs on it
    code = main(["describe", str(tmp_path)])

    # THEN it fails and says what is missing
    assert code == 1
    assert "apollia.toml not found" in capsys.readouterr().err


def test_describe_refuses_two_declarations_with_one_name(
    package: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    # GIVEN a second module declaring a tool under an existing name
    (package / "invoice_tools" / "copy.py").write_text(
        "from typing import Any\n\n"
        "from apollia import tool\n\n\n"
        '@tool("extract_totals", "Duplicate.", action="read")\n'
        "async def again(ctx: Any) -> None:\n"
        "    return None\n",
        encoding="utf-8",
    )
    before = (package / "apollia.toml").read_text(encoding="utf-8")

    # WHEN describe runs
    code = main(["describe", str(package)])

    # THEN it fails, names both modules, and leaves the manifest untouched
    assert code == 1
    err = capsys.readouterr().err
    assert "'extract_totals' is declared twice" in err
    assert "invoice_tools/copy.py" in err
    assert (package / "apollia.toml").read_text(encoding="utf-8") == before


def test_rewrite_keeps_the_comment_that_introduces_the_next_table() -> None:
    # GIVEN a manifest where a tools table sits between two other tables
    text = (
        '[package]\nname = "p"\n\n'
        '[[tools]]\nname = "old"\n\n'
        "# Why the package needs gmail.\n"
        '[[requires]]\ntype = "gmail"\n'
    )

    # WHEN its tables are rewritten with nothing to write
    out = rewrite_manifest(text, "")

    # THEN the old table is gone and the comment above requires is kept
    assert "old" not in out
    assert "# Why the package needs gmail.\n[[requires]]" in out


def test_toml_value_refuses_what_toml_cannot_carry() -> None:
    # GIVEN a schema fragment holding None
    # WHEN it is rendered
    # THEN it is refused rather than written as an invalid manifest
    with pytest.raises(AgentConfigError, match="cannot be written to TOML"):
        toml_value({"default": None})
    assert toml_value({"a b": [1, 2.5, True]}) == '{ "a b" = [1, 2.5, true] }'
