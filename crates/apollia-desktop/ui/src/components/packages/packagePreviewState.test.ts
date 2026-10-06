import { describe, test, expect } from "vitest";

import type { PackagePreview } from "$lib/ipc/packageFiles";
import {
  accessGroups,
  canProceed,
  canTrustPublisher,
  changesToAccept,
  connectionLines,
  contentCounts,
  contentLines,
  fileName,
  installConfirmations,
  previewVariant,
  publisherState,
  refusalDetail,
} from "./packagePreviewState";

function preview(over: Partial<PackagePreview> = {}): PackagePreview {
  return {
    name: "crm",
    version: "1.0.0",
    description: "Qualify the requests",
    sdk: ">=0.3",
    signature: {
      status: "signed",
      publisher: "Acme",
      declaredPublisher: "Acme",
      keyFingerprint: "0123456789abcdef0123",
      publicKey: "ab".repeat(32),
      trusted: true,
      reason: null,
    },
    signatureRequired: false,
    agents: [
      {
        id: "crm-agent",
        name: "Qualifier",
        mission: "Qualify the requests",
        examples: [],
        access: [
          { accessId: "pkg:crm/lookup", permits: "Look up a lead", sensitive: false, family: "read" },
          { accessId: "mail_send", permits: "Send a mail", sensitive: true, family: "send_external" },
        ],
      },
      {
        id: "crm-helper",
        name: "Helper",
        mission: "Help",
        examples: [],
        access: [
          { accessId: "pkg:crm/lookup", permits: "Look up a lead", sensitive: false, family: "read" },
          { accessId: "mcp:notion/search", permits: "use it", sensitive: true, family: "unclassified" },
        ],
      },
    ],
    workflows: [{ name: "weekly", description: "Weekly digest", family: "send_external", sensitive: true, steps: [] }],
    tools: [{ name: "lookup", description: "Look up a lead", family: "read", sensitive: false }],
    requires: [
      { type: "gmail", reason: "Read the mails", present: true, broughtByPackage: false },
      { type: "outlook", reason: "Send the follow-ups", present: false, broughtByPackage: false },
      { type: "ledger", reason: "Books", present: false, broughtByPackage: true },
    ],
    mcpServers: [
      { name: "ledger", transport: "stdio", command: "uvx", url: null, remote: false, alreadyPresent: false },
    ],
    pip: [],
    triggers: [],
    installedVersion: null,
    update: null,
    notices: [],
    refusal: null,
    ...over,
  };
}

describe("the variant of the preview", () => {
  test("a trusted signed package installs plainly", () => {
    // GIVEN a package signed by a trusted publisher
    const p = preview();
    // WHEN its variant is read
    // THEN it is a plain installation, enabled, with a trusted publisher
    expect(previewVariant(p)).toBe("install");
    expect(publisherState(p)).toBe("trusted");
    expect(canProceed(p, new Set())).toBe(true);
    expect(canTrustPublisher(p)).toBe(false);
  });

  test("an unsigned package warns and still installs", () => {
    // GIVEN an unsigned package that governance does not refuse
    const p = preview({
      signature: { status: "unsigned", publisher: "Acme", declaredPublisher: "Acme", trusted: false },
    });
    // WHEN its variant is read
    // THEN it is the unsigned variant, installable, with no publisher to trust
    expect(previewVariant(p)).toBe("unsigned");
    expect(publisherState(p)).toBe("unsigned");
    expect(canProceed(p, new Set())).toBe(true);
    expect(canTrustPublisher(p)).toBe(false);
  });

  test("an invalid signature is refused, readably, with nothing to install", () => {
    // GIVEN a package whose signature does not hold
    const p = preview({
      signature: {
        status: "invalid",
        publisher: "Acme",
        declaredPublisher: "Acme",
        trusted: false,
        reason: "the signature does not match the content of the package",
      },
      refusal: { code: "invalid_signature", reason: "the signature does not match" },
    });
    // WHEN its variant is read
    // THEN it is refused and the main action is disabled; the detail names the reason
    expect(previewVariant(p)).toBe("invalid");
    expect(publisherState(p)).toBe("invalid");
    expect(canProceed(p, new Set())).toBe(false);
    expect(refusalDetail(p, "/tmp/crm.apo", "does not match")).toContain("does not match");
  });

  test("an unsigned package the policy refuses has its own variant", () => {
    // GIVEN an unsigned package under a policy that requires a signature
    const p = preview({
      signature: { status: "unsigned", publisher: "Acme", declaredPublisher: "Acme", trusted: false },
      signatureRequired: true,
      refusal: { code: "signature_required" },
    });
    // WHEN its variant is read
    // THEN it is the policy refusal, with nothing to install
    expect(previewVariant(p)).toBe("policy_refused");
    expect(canProceed(p, new Set())).toBe(false);
  });

  test("a signed publisher not yet trusted can be trusted from the preview", () => {
    // GIVEN a package signed by a key absent from the keyring
    const p = preview({
      signature: {
        status: "signed",
        publisher: "Acme",
        declaredPublisher: "Acme",
        keyFingerprint: "f00d",
        publicKey: "cd".repeat(32),
        trusted: false,
      },
    });
    // WHEN its publisher is read
    // THEN it reads signed, not trusted, and the trust gesture is offered
    expect(publisherState(p)).toBe("signed_untrusted");
    expect(canTrustPublisher(p)).toBe(true);
  });
});

describe("the content, accesses and connections", () => {
  test("the content is grouped by kind and the accesses by family, each once", () => {
    // GIVEN two agents that share one access
    const p = preview();
    // WHEN the content and accesses are laid out
    const lines = contentLines(p);
    const groups = accessGroups(p);
    // THEN agents, workflows and tools come in that order, counted
    expect(lines.map((l) => l.kind)).toEqual(["agent", "agent", "workflow", "tool"]);
    expect(contentCounts(p)).toEqual({ agent: 2, workflow: 1, tool: 1 });
    // AND each access appears once, under its family, families in order
    expect(groups.map((g) => g.family)).toEqual(["read", "send_external", "unclassified"]);
    expect(groups[0].accesses).toHaveLength(1);
  });

  test("a package without agent shows its content and asks for no access", () => {
    // GIVEN a package of one tool and one workflow, no agent
    const p = preview({ agents: [] });
    // WHEN it is laid out
    // THEN its content is there, no access is asked, and it installs
    expect(contentLines(p).map((l) => l.kind)).toEqual(["workflow", "tool"]);
    expect(accessGroups(p)).toEqual([]);
    expect(previewVariant(p)).toBe("install");
    expect(canProceed(p, new Set())).toBe(true);
  });

  test("required connections read present or missing, brought ones once", () => {
    // GIVEN a package that requires gmail (present), outlook (missing) and brings ledger
    const p = preview();
    // WHEN the connections are laid out
    const lines = connectionLines(p);
    // THEN each reads in its state, and ledger is listed once, as brought
    expect(lines).toEqual([
      { name: "gmail", reason: "Read the mails", state: "present" },
      { name: "outlook", reason: "Send the follow-ups", state: "missing" },
      { name: "ledger", reason: "Books", state: "brought" },
    ]);
  });
});

describe("the update", () => {
  const update = () =>
    preview({
      installedVersion: "1.0.0",
      version: "1.1.0",
      pip: ["requests"],
      update: {
        fromVersion: "1.0.0",
        toVersion: "1.1.0",
        downgrade: false,
        addedAccess: [
          { agentId: "crm-agent", accessId: "mail_send", permits: "Send a mail", sensitive: true, family: "send_external" },
          { agentId: "crm-helper", accessId: "mail_send", permits: "Send a mail", sensitive: true, family: "send_external" },
        ],
        addedRequires: ["outlook"],
        addedMcpServers: [],
      },
    });

  test("what an update adds is put forward and must be accepted before updating", () => {
    // GIVEN an update that adds one access (asked by two agents) and one missing connection
    const p = update();
    const changes = changesToAccept(p);
    // WHEN nothing is accepted yet
    // THEN the update variant lists each addition once and the action is disabled
    expect(previewVariant(p)).toBe("update");
    expect(changes.map((c) => c.key)).toEqual(["access:mail_send", "connection:outlook"]);
    expect(changes[1].missing).toBe(true);
    expect(canProceed(p, new Set(["access:mail_send"]))).toBe(false);
    // AND once every addition is accepted, the update proceeds and confirms them
    const all = new Set(changes.map((c) => c.key));
    expect(canProceed(p, all)).toBe(true);
    expect(installConfirmations(p, all)).toEqual({ depsConfirmed: true, changesConfirmed: true });
  });

  test("an install confirms no change, and only the requirements it carries", () => {
    // GIVEN a fresh install with no Python requirement
    const p = preview();
    // WHEN its confirmations are computed
    // THEN neither is sent
    expect(installConfirmations(p, new Set())).toEqual({ depsConfirmed: false, changesConfirmed: false });
  });

  test("a downgrade is a refusal, not an update", () => {
    // GIVEN an older version than the installed one
    const p = preview({
      refusal: { code: "downgrade", installed: "2.0.0", incoming: "1.0.0" },
      update: { fromVersion: "2.0.0", toVersion: "1.0.0", downgrade: true, addedAccess: [], addedRequires: [], addedMcpServers: [] },
    });
    // WHEN its variant is read
    // THEN it is refused
    expect(previewVariant(p)).toBe("refused");
    expect(canProceed(p, new Set())).toBe(false);
  });
});

test("the file name is the last segment of either separator", () => {
  // GIVEN a POSIX and a Windows path
  // WHEN their file names are read
  // THEN each is the last segment
  expect(fileName("/Users/me/Downloads/crm-1.0.0.apo")).toBe("crm-1.0.0.apo");
  expect(fileName("C:\\Users\\me\\crm.apo")).toBe("crm.apo");
});

describe("a package the application carries", () => {
  const unsigned = {
    status: "unsigned" as const,
    publisher: "Apollia",
    declaredPublisher: "Apollia",
    trusted: false,
  };

  test("an example is unsigned and is not warned about for it", () => {
    // GIVEN the preview of an unsigned package
    const p = preview({ signature: unsigned });
    // WHEN it is read as a file, then as an example the application carries
    // THEN only the file takes the unsigned variant and the unknown publisher
    expect(previewVariant(p)).toBe("unsigned");
    expect(publisherState(p)).toBe("unsigned");
    expect(previewVariant(p, true)).toBe("install");
    expect(publisherState(p, true)).toBe("shipped");
    expect(canProceed(p, new Set())).toBe(true);
  });

  test("a refusal still wins over the origin", () => {
    // GIVEN an example whose agent another package already owns
    const p = preview({ signature: unsigned, refusal: { code: "agent_taken", agent: "crm-agent", owner: "other" } });
    // WHEN its variant is read as an example
    // THEN it is refused like any package
    expect(previewVariant(p, true)).toBe("refused");
    expect(canProceed(p, new Set())).toBe(false);
  });

  test("the detail of a refusal leaves out the path an example does not have", () => {
    // GIVEN an example, which has no file
    const p = preview({ signature: unsigned });
    // WHEN the detail of its refusal is written
    // THEN it names the package and the reason, with no empty line
    expect(refusalDetail(p, "", "already installed elsewhere")).toBe("crm 1.0.0\nalready installed elsewhere");
  });
});
