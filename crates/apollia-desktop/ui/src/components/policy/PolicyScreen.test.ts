import { describe, test, expect, vi } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { render as renderToString } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

// Stores read the browser storage at import; node has none.
vi.hoisted(() => {
  const map = new Map<string, string>();
  globalThis.localStorage = {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
    key: () => null,
    length: 0,
  } as Storage;
});

import "$lib/i18n";
import fr from "$lib/i18n/fr.json";
import type { OrgPolicyBody, PolicyView } from "$lib/ipc/policy";
import { formatSignedAt } from "$lib/policy/policyRules";
import PolicyScreen from "./PolicyScreen.svelte";

const HERE = fileURLToPath(new URL(".", import.meta.url));
const SRC = join(HERE, "../..");

async function french(): Promise<void> {
  locale.set("fr");
  await waitLocale("fr");
}

function emptyPolicy(): OrgPolicyBody {
  return {
    schema_version: 1,
    signed_at: "2026-10-02T09:30:00Z",
    connections: { allow: [], deny: [], remote: null },
    families: { deny: [] },
    inference: { allowed_servers: [], remote_endpoints: null, require_model_hash: false },
    settings: {},
    packages: { require_signature: false, trusted_publishers: [] },
    journal: { required: false, retention_days: null, export: { detail: null, daily_summary_dir: null } },
  };
}

const FINGERPRINT = "7f3a91c24b0de86521af0c7d9e34b6f17f3a91c24b0de86521af0c7d9e34b6f1";

function activeView(): PolicyView {
  const policy = emptyPolicy();
  policy.connections.remote = "deny";
  policy.families.deny = ["execute_code"];
  policy.inference.require_model_hash = true;
  policy.settings = { "desktop.developer_mode": false };
  policy.packages.require_signature = true;
  policy.journal.retention_days = 365;
  return {
    state: "active",
    dir: "/etc/apollia/policy",
    key_fingerprint: FINGERPRINT,
    publisher: "Example IT",
    signed_at: "2026-10-02T09:30:00Z",
    policy,
  };
}

/** Every form control a person could type into or toggle. */
const EDITABLE = /<(input|select|textarea)\b|contenteditable/i;

describe("the policy screen", () => {
  test("with no policy, says the person is free and nothing else is imposed", async () => {
    // GIVEN a runtime that found no policy
    await french();
    const view: PolicyView = { state: "none", dir: "/etc/apollia/policy" };

    // WHEN the screen is rendered
    const { body } = renderToString(PolicyScreen, { props: { view } });

    // THEN it says so in the words of the criterion, without the read-only notice
    expect(body).toContain('data-policy-state="none"');
    expect(body).toContain(fr.policy.none.title);
    expect(body).toContain(fr.policy.none.body);
    expect(body).not.toContain(fr.policy.badge.read_only);
    expect(body).not.toContain(fr.policy.subtitle.slice(0, 30));
    expect(body).not.toMatch(EDITABLE);
  });

  test("with an active policy, shows the key, the signing date and the rules by section", async () => {
    // GIVEN a runtime that applies a valid policy
    await french();

    // WHEN the screen is rendered
    const { body } = renderToString(PolicyScreen, { props: { view: activeView() } });

    // THEN the signing date, the publisher and every touched section are there
    expect(body).toContain('data-policy-state="active"');
    expect(body).toContain(formatSignedAt("2026-10-02T09:30:00Z", "fr"));
    expect(body).toContain("Example IT");
    expect(body).toContain(fr.policy.signature.signed_at);
    for (const id of ["signature", "connections", "families", "inference", "settings", "packages", "journal"]) {
      expect(body).toContain(`data-testid="policy-section-${id}"`);
    }
    // AND each rule reads in plain words
    expect(body).toContain(fr.policy.rule.connections_remote);
    expect(body).toContain(fr.agent_sheet.family_execute_code);
    expect(body).toContain(fr.policy.rule.inference_hash);
    expect(body).toContain("desktop.developer_mode");
    expect(body).toContain(fr.policy.rule.packages_signature);
    expect(body).toContain('data-testid="policy-rule-journal-retention"');
    expect(body).toContain(">365 ");
    // AND the key fingerprint sits under the folded details line
    expect(body).toContain('data-testid="policy-details-signature-toggle"');
    expect(body).toContain(fr.policy.badge.read_only);
    expect(body).not.toMatch(EDITABLE);
  });

  test("an active policy that imposes nothing says so", async () => {
    // GIVEN a valid policy with no rule
    await french();
    const view: PolicyView = { ...(activeView() as Extract<PolicyView, { state: "active" }>), policy: emptyPolicy() };

    // WHEN the screen is rendered
    const { body } = renderToString(PolicyScreen, { props: { view } });

    // THEN only the signature card is shown, with the sentence of no rule
    expect(body).toContain('data-testid="policy-section-signature"');
    expect(body).not.toContain('data-testid="policy-section-connections"');
    expect(body).toContain(fr.policy.no_rule);
  });

  test("with an invalid policy, gives the reason and what holds meanwhile", async () => {
    // GIVEN a runtime that refused the policy for its signature
    await french();
    const view: PolicyView = {
      state: "invalid",
      dir: "/etc/apollia/policy",
      reason: "signature",
      detail: "the signature does not hold for org.pub",
    };

    // WHEN the screen is rendered
    const { body } = renderToString(PolicyScreen, { props: { view } });

    // THEN the reason is the plain sentence of its class, not the raw detail
    expect(body).toContain('data-policy-state="invalid"');
    expect(body).toContain(fr.policy.invalid.title);
    expect(body).toContain(fr.policy.reason.signature);
    expect(body).not.toContain(view.detail);
    expect(body).toContain('data-testid="policy-section-meanwhile"');
    expect(body).toContain('data-testid="policy-copy-reason"');
    expect(body).not.toMatch(EDITABLE);
  });

  test("each refusal class has its own sentence", async () => {
    // GIVEN the four reasons a policy is refused for
    await french();
    const reasons = ["schema", "signature", "key", "owner"] as const;

    // WHEN each is rendered
    const sentences = reasons.map((reason) => {
      const { body } = renderToString(PolicyScreen, {
        props: { view: { state: "invalid", dir: "/x", reason, detail: "d" } },
      });
      return reasons.find((r) => body.includes(fr.policy.reason[r]));
    });

    // THEN each one shows its own sentence
    expect(sentences).toEqual([...reasons]);
  });
});

describe("nothing on the policy screen writes", () => {
  test("the screen, its components and its wrapper call no command but the read", () => {
    // GIVEN every source of the policy screen
    const files = [
      ...readdirSync(HERE)
        .filter((name) => name.endsWith(".svelte"))
        .map((name) => join(HERE, name)),
      join(SRC, "routes/Policy.svelte"),
      join(SRC, "lib/ipc/policy.ts"),
      join(SRC, "lib/policy/policyRules.ts"),
    ];

    // WHEN the Tauri commands they name are collected (the bracket keeps this
    // pattern from reading as a call to the IPC guard)
    const commands = files.flatMap((file) =>
      [...readFileSync(file, "utf8").matchAll(/invok[e](?:<[^>]*>)?\(\s*"([\w]+)"/g)].map((m) => m[1]),
    );
    const imports = files.flatMap((file) =>
      [...readFileSync(file, "utf8").matchAll(/from "\$lib\/ipc\/(\w+)"/g)].map((m) => m[1]),
    );

    // THEN the only command is the read of the policy, and no other IPC module is used
    expect(commands).toEqual(["get_org_policy"]);
    expect(new Set(imports)).toEqual(new Set(["policy"]));
  });
});
