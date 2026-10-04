import { describe, test, expect, beforeAll, afterEach } from "vitest";
import { render } from "svelte/server";
import { addMessages, init, waitLocale } from "svelte-i18n";
import fr from "$lib/i18n/fr.json";
import { developerMode } from "$lib/stores/developerMode";
import ApprovalCard from "./ApprovalCard.svelte";
import OperatorApprovalCard from "./OperatorApprovalCard.svelte";
import { isSingleSimpleCommand, offeredAnswers } from "./codeConsent";
import { liveApprovalCall } from "./liveApprovalCall";

// Vitest runs in `node`, where a component cannot be mounted; the cards are
// rendered to HTML on the server side instead, which is what they show
// before the first click.

beforeAll(async () => {
  addMessages("fr", fr);
  init({ fallbackLocale: "fr", initialLocale: "fr" });
  await waitLocale();
});

afterEach(() => {
  developerMode.set(false);
});

const BASE = { sessionId: "s", messageId: "m", toolCallId: "c" };

/** The text inside the element carrying `testid`, `null` when it is absent. */
function textOf(html: string, testid: string): string | null {
  const open = html.indexOf(`data-testid="${testid}"`);
  if (open < 0) return null;
  const start = html.indexOf(">", open) + 1;
  const tag = html.lastIndexOf("<", open);
  const name = /^<([a-z0-9]+)/i.exec(html.slice(tag))?.[1] ?? "div";
  const end = html.indexOf(`</${name}>`, start);
  return html.slice(start, end).replace(/<!--[^]*?-->/g, "");
}

/** The opening tag of the element carrying `testid`. */
function tagOf(html: string, testid: string): string {
  const at = html.indexOf(`data-testid="${testid}"`);
  return html.slice(html.lastIndexOf("<", at), html.indexOf(">", at) + 1);
}

describe("ApprovalCard - running code", () => {
  test("a command of 5,000 characters is shown whole, unwrapped, with its folder", () => {
    // GIVEN a shell command of 5,000 characters and the folder it runs in
    const command = `echo ${"x".repeat(4_995)}`;
    expect(command.length).toBe(5_000);

    // WHEN the card renders the request
    const { body } = render(ApprovalCard, {
      props: {
        ...BASE,
        toolName: "bash_executor",
        inputPreview: command,
        code: command,
        workingDir: "/srv/data",
        answers: ["once", "refuse"],
      },
    });

    // THEN the command is there in full, on lines that do not wrap
    expect(textOf(body, "approval-code-bash_executor")).toBe(command);
    expect(tagOf(body, "approval-code-bash_executor")).toContain("whitespace-pre ");
    expect(tagOf(body, "approval-code-bash_executor")).toContain("overflow-auto");
    // AND the folder is named
    expect(textOf(body, "approval-workdir-bash_executor")).toContain("/srv/data");
  });

  test("Python source is shown whole as well", () => {
    // GIVEN Python source of 5,000 characters, several lines long
    const source = `import json\n${"print(1)\n".repeat(554)}`.slice(0, 5_000);
    // WHEN the card renders the request
    const { body } = render(ApprovalCard, {
      props: {
        ...BASE,
        toolName: "python_executor",
        inputPreview: source,
        code: source,
        workingDir: "/home/me",
        answers: ["once", "refuse"],
      },
    });
    // THEN every line is there
    expect(textOf(body, "approval-code-python_executor")).toBe(source);
  });

  test("outside developer mode, the card offers once and refuse only", () => {
    // GIVEN a simple command, the developer mode off, two answers offered
    // WHEN the card renders
    const { body } = render(ApprovalCard, {
      props: {
        ...BASE,
        toolName: "bash_executor",
        inputPreview: "ls -la",
        code: "ls -la",
        workingDir: "/tmp",
        answers: ["once", "refuse"],
      },
    });
    // THEN "once" and "refuse" are there and "always" is not
    expect(body).toContain('data-testid="approval-accept-bash_executor"');
    expect(body).toContain('data-testid="approval-refuse-bash_executor"');
    expect(body).not.toContain('data-testid="approval-always-bash_executor"');
    // AND the hint says every run asks
    expect(textOf(body, "approval-hint-bash_executor")).toContain(
      fr.approval.code.each_time_hint,
    );
  });

  test("in developer mode, a simple command offers to always allow that command", () => {
    // GIVEN the developer mode on and the three answers offered
    developerMode.set(true);
    // WHEN the card renders a simple command
    const { body } = render(ApprovalCard, {
      props: {
        ...BASE,
        toolName: "bash_executor",
        inputPreview: "git log --oneline",
        code: "git log --oneline",
        workingDir: "/tmp",
        answers: ["once", "always", "refuse"],
      },
    });
    // THEN "always" names the command, not the agent or the tool
    expect(textOf(body, "approval-always-bash_executor")).toContain(fr.approval.code.always);
    expect(textOf(body, "approval-hint-bash_executor")).toContain(
      fr.approval.code.always_hint,
    );
  });

  test("in developer mode, a compound command is never offered always", () => {
    // GIVEN the developer mode on, a chained command, and two answers offered
    developerMode.set(true);
    // WHEN the card renders
    const { body } = render(ApprovalCard, {
      props: {
        ...BASE,
        toolName: "bash_executor",
        inputPreview: "cd build && rm -f a.tmp",
        code: "cd build && rm -f a.tmp",
        workingDir: "/tmp",
        answers: ["once", "refuse"],
      },
    });
    // THEN "always" is absent and the hint says why
    expect(body).not.toContain('data-testid="approval-always-bash_executor"');
    expect(textOf(body, "approval-hint-bash_executor")).toContain(
      fr.approval.code.compound_hint,
    );
  });
});

describe("ApprovalCard - other families", () => {
  test("a short preview, the full detail behind see more", () => {
    // GIVEN a file write whose content runs past the preview
    const content = "z".repeat(1_000);
    const detail = JSON.stringify({ path: "notes.txt", content }, null, 2);
    // WHEN the card renders the request
    const { body } = render(ApprovalCard, {
      props: {
        ...BASE,
        toolName: "file_write",
        inputPreview: `Tool 'file_write' asks to run with: ${detail.slice(0, 200)}...`,
        detail,
        answers: ["once", "always", "refuse"],
      },
    });
    // THEN the preview is short, no code block is shown, and the detail waits
    // behind "see more"
    expect((textOf(body, "approval-preview-file_write") ?? "").length).toBeLessThan(300);
    expect(body).not.toContain("approval-code-");
    expect(textOf(body, "approval-see-more-file_write")).toContain(fr.approval.see_more);
    expect(body).not.toContain('data-testid="approval-detail-file_write"');
    // AND the three answers are offered
    expect(body).toContain('data-testid="approval-always-file_write"');
  });
});

describe("OperatorApprovalCard - running code", () => {
  test("a pending call to the shell shows its whole command and two answers", () => {
    // GIVEN a stored pending call to the shell with a long command
    const command = `echo ${"y".repeat(1_000)}`;
    // WHEN the operator card renders it
    const { body } = render(OperatorApprovalCard, {
      props: {
        sessionId: "s",
        messageId: "m",
        toolCall: {
          tool_name: "bash_executor",
          input: { command, working_dir: "/srv" },
          output: null,
          status: "pending",
        },
      },
    });
    // THEN the command is whole, the folder named, and "always" absent
    expect(textOf(body, "operator-approval-code-bash_executor")).toBe(command);
    expect(textOf(body, "operator-approval-workdir-bash_executor")).toContain("/srv");
    expect(body).not.toContain('data-testid="operator-approval-always-bash_executor"');
  });
});

describe("OperatorApprovalCard - a live request that runs code", () => {
  test("the code and the folder of the request are shown whole, with two answers", () => {
    // GIVEN a live request for a shell command of 5,000 characters, whose
    // prompt is the command itself and whose detail holds the arguments
    const command = `echo ${"z".repeat(4_995)}`;
    const detail = JSON.stringify({ command, timeout_secs: 10 }, null, 2);
    // WHEN the card of the default mode renders it
    const { body } = render(OperatorApprovalCard, {
      props: {
        sessionId: "s",
        messageId: "m",
        toolCallId: "c",
        toolCall: liveApprovalCall("bash_executor", command, detail),
        answers: ["once", "refuse"],
        code: command,
        workingDir: "/srv/runtime",
      },
    });
    // THEN the command is whole, the folder is the one the request names,
    // and "always" is not offered
    expect(textOf(body, "operator-approval-code-bash_executor")).toBe(command);
    expect(textOf(body, "operator-approval-workdir-bash_executor")).toContain("/srv/runtime");
    expect(body).toContain('data-testid="operator-approval-accept-bash_executor"');
    expect(body).not.toContain('data-testid="operator-approval-always-bash_executor"');
  });

  test("the arguments are read from the detail when the prompt is the code", () => {
    // GIVEN a prompt that is a bare command, and the full arguments beside it
    const detail = JSON.stringify({ command: "git status", working_dir: "/repo" }, null, 2);
    // WHEN the pending call is rebuilt
    const call = liveApprovalCall("bash_executor", "git status", detail);
    // THEN the arguments come from the detail
    expect(call.input).toEqual({ command: "git status", working_dir: "/repo" });
  });
});

describe("codeConsent - the rules the card mirrors", () => {
  test("a request without answers falls back on the tool", () => {
    // GIVEN no answers named by the request
    // WHEN the offered answers are derived
    // THEN code gets two, any other tool three
    expect(offeredAnswers("bash_executor", undefined)).toEqual(["once", "refuse"]);
    expect(offeredAnswers("file_write", [])).toEqual(["once", "always", "refuse"]);
    expect(offeredAnswers("bash_executor", ["refuse", "once"])).toEqual(["once", "refuse"]);
  });

  test("a single simple command is told apart from a chained one", () => {
    // GIVEN plain and chained commands
    // WHEN checked
    // THEN only the plain ones pass
    expect(isSingleSimpleCommand("git status --short")).toBe(true);
    expect(isSingleSimpleCommand("git status; rm -rf /")).toBe(false);
    expect(isSingleSimpleCommand("echo $(whoami)")).toBe(false);
    expect(isSingleSimpleCommand("ls | wc -l")).toBe(false);
  });
});
