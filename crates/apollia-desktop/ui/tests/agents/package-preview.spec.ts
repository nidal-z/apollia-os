import { test, expect, type Page } from "@playwright/test";

/**
 * The preview of a `.apo` package before it is installed or updated.
 *
 * Boots the UI bundle with the Tauri bridge stubbed, reaches the Agents
 * screen, chooses a package file through its button (the system file chooser
 * answers the path the case names) and reads the dialog: what the package
 * brings, the accesses by family, the publisher, the connections, and the
 * variants an unsigned package, an invalid signature and an update take.
 * The stub records the installation and the trust calls it receives.
 */

type InvokeStub = (cmd: string, args: unknown) => unknown;

interface TauriInternalsStub {
  invoke: InvokeStub;
  transformCallback: (cb: (payload: unknown) => void, once: boolean) => number;
  convertFileSrc: (path: string) => string;
}

async function installTauriStub(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const signed = {
      status: "signed",
      publisher: "Acme",
      declaredPublisher: "Acme",
      keyFingerprint: "0123456789abcdef0123456789abcdef",
      publicKey: "ab".repeat(32),
      trusted: true,
      reason: null,
    };
    const base = {
      name: "crm",
      version: "1.0.0",
      description: "Qualifies the incoming requests",
      sdk: ">=0.3",
      signature: signed,
      signatureRequired: false,
      agents: [{
        id: "crm-agent", name: "Qualifier", mission: "Qualify the requests", examples: [],
        access: [
          { accessId: "pkg:crm/lookup", permits: "Look up a lead in the CRM", sensitive: false, family: "read" },
          { accessId: "mail_send", permits: "Send a mail from your account", sensitive: true, family: "send_external" },
        ],
      }],
      workflows: [{ name: "weekly", description: "Weekly digest", family: "send_external", sensitive: true, steps: [] }],
      tools: [{ name: "lookup", description: "Look up a lead", family: "read", sensitive: false }],
      requires: [
        { type: "gmail", reason: "Read the requests", present: true, broughtByPackage: false },
        { type: "notion", reason: "File the leads", present: false, broughtByPackage: false },
      ],
      mcpServers: [{ name: "ledger", transport: "stdio", command: "uvx", url: null, remote: false, alreadyPresent: false }],
      pip: [],
      triggers: [],
      installedVersion: null,
      update: null,
      notices: [],
      refusal: null,
    };
    const previews: Record<string, unknown> = {
      "/files/crm.apo": base,
      "/files/unsigned.apo": {
        ...base,
        signature: { status: "unsigned", publisher: "Acme", declaredPublisher: "Acme", trusted: false },
      },
      "/files/altered.apo": {
        ...base,
        signature: {
          status: "invalid", publisher: "Acme", declaredPublisher: "Acme", trusted: false,
          reason: "the signature does not match the content of the package",
        },
        refusal: { code: "invalid_signature", reason: "the signature does not match the content of the package" },
      },
      "/files/crm-1.1.0.apo": {
        ...base,
        version: "1.1.0",
        installedVersion: "1.0.0",
        update: {
          fromVersion: "1.0.0", toVersion: "1.1.0", downgrade: false,
          addedAccess: [{ agentId: "crm-agent", accessId: "mail_send", permits: "Send a mail from your account", sensitive: true, family: "send_external" }],
          addedRequires: ["notion"],
          addedMcpServers: [],
        },
      },
      "/files/policy.apo": {
        ...base,
        signature: { status: "unsigned", publisher: "Acme", declaredPublisher: "Acme", trusted: false },
        signatureRequired: true,
        refusal: { code: "signature_required" },
      },
      "/files/stranger.apo": {
        ...base,
        signature: { ...signed, trusted: false, keyFingerprint: "f00df00df00df00d" },
      },
    };
    const calls: { cmd: string; args: unknown }[] = [];
    (globalThis as unknown as { __packageCalls: typeof calls }).__packageCalls = calls;
    const handlers: Record<string, (args: unknown) => unknown> = {
      get_onboarding_state: () => ({ completed: true, skipped: false, started_at: null }),
      read_config_file: () => ({ path: "~/.apollia/apollia.toml", exists: true, text: "", sha256: "", diagnostics: [], locked: [], pendingRestart: [] }),
      get_system_info: () => ({}),
      "plugin:dialog|open": () => (globalThis as unknown as { __nextFile: string }).__nextFile,
      preview_package_file: (args) => {
        const path = (args as { path: string }).path;
        const preview = previews[path];
        if (!preview) throw new Error(`not a package: ${path}`);
        return preview;
      },
      install_package_file: (args) => {
        calls.push({ cmd: "install_package_file", args });
        return { name: "crm", version: "1.1.0" };
      },
      trust_package_publisher: (args) => {
        calls.push({ cmd: "trust_package_publisher", args });
        return { fingerprint: "f00d", publicKey: "ab".repeat(32), label: "Acme", origin: "user", addedAt: "" };
      },
    };
    let nextCallbackId = 1;
    (globalThis as unknown as { __TAURI_INTERNALS__?: TauriInternalsStub }).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, args: unknown) => {
        const fn = handlers[cmd];
        if (fn) {
          try {
            return Promise.resolve(fn(args));
          } catch (err) {
            return Promise.reject(String(err));
          }
        }
        if (cmd.startsWith("list_")) return Promise.resolve([]);
        return Promise.resolve(null);
      },
      transformCallback: (cb: (payload: unknown) => void, once: boolean) => {
        const id = nextCallbackId++;
        Object.defineProperty(globalThis, `_${id}`, {
          value: (payload: unknown) => {
            if (once) Reflect.deleteProperty(globalThis, `_${id}`);
            return cb(payload);
          },
          writable: false,
          configurable: true,
        });
        return id;
      },
      convertFileSrc: (path: string) => path,
    };
  });
}

async function openPreview(page: Page, file: string): Promise<void> {
  await page.goto("/");
  await page.getByTestId("nav-agents").first().click();
  await expect(page.getByTestId("agents-page")).toBeVisible();
  await page.evaluate((f) => {
    (globalThis as unknown as { __nextFile: string }).__nextFile = f;
  }, file);
  await page.getByTestId("agents-install-apo-button").click();
  await expect(page.getByTestId("package-preview-dialog")).toBeVisible();
}

async function calls(page: Page): Promise<{ cmd: string; args: Record<string, unknown> }[]> {
  return page.evaluate(
    () => (globalThis as unknown as { __packageCalls: { cmd: string; args: Record<string, unknown> }[] }).__packageCalls,
  );
}

test.describe("package preview", () => {
  test.beforeEach(async ({ page }) => {
    await installTauriStub(page);
  });

  test("a signed package shows what it brings, asks, who made it and its connections", async ({ page }) => {
    // GIVEN a package signed by a trusted publisher, chosen from the Agents screen
    await openPreview(page, "/files/crm.apo");

    // WHEN its preview is read
    const dialog = page.getByTestId("package-preview-dialog");

    // THEN the title names it, the content is grouped and counted
    await expect(page.getByTestId("package-preview-title")).toContainText("crm");
    await expect(page.getByTestId("package-preview-content-agent")).toContainText("Qualifier");
    await expect(page.getByTestId("package-preview-content-workflow")).toContainText("weekly");
    await expect(page.getByTestId("package-preview-content-tool")).toContainText("lookup");
    // AND the accesses are grouped by family with what each permits
    await expect(page.getByTestId("package-preview-family-read")).toContainText("Look up a lead in the CRM");
    await expect(page.getByTestId("package-preview-family-send_external")).toContainText("Send a mail");
    // AND the publisher is trusted, with its fingerprint
    await expect(page.getByTestId("package-preview-publisher")).toHaveAttribute("data-state", "trusted");
    await expect(page.getByTestId("package-preview-fingerprint")).toBeVisible();
    // AND the connections read present, missing with "Add", and brought
    await expect(page.getByTestId("package-preview-connection-gmail")).toHaveAttribute("data-state", "present");
    await expect(page.getByTestId("package-preview-connection-notion")).toHaveAttribute("data-state", "missing");
    await expect(page.getByTestId("package-preview-connection-add-notion")).toBeVisible();
    await expect(page.getByTestId("package-preview-connection-ledger")).toHaveAttribute("data-state", "brought");
    // AND no technical identifier is shown
    await expect(dialog).not.toContainText("pkg:crm/lookup");

    // WHEN the user installs it
    await page.getByTestId("package-preview-install").click();

    // THEN the installation is sent with no change to confirm, and the dialog closes
    await expect(dialog).toBeHidden();
    const sent = await calls(page);
    expect(sent).toEqual([
      { cmd: "install_package_file", args: { path: "/files/crm.apo", depsConfirmed: false, changesConfirmed: false } },
    ]);
  });

  test("an unsigned package warns and installs only when asked to anyway", async ({ page }) => {
    // GIVEN an unsigned package
    await openPreview(page, "/files/unsigned.apo");

    // WHEN its preview is read
    // THEN the warning shows, the publisher is unknown, and the action says "anyway"
    await expect(page.getByTestId("package-preview-unsigned-warning")).toBeVisible();
    await expect(page.getByTestId("package-preview-publisher")).toHaveAttribute("data-state", "unsigned");
    await expect(page.getByTestId("package-preview-install")).toBeEnabled();
    await expect(page.getByTestId("package-preview-install")).not.toHaveText("Installer");
  });

  test("an invalid signature is refused readably, with nothing to install", async ({ page }) => {
    // GIVEN a package altered after it was signed
    await openPreview(page, "/files/altered.apo");

    // WHEN its preview is read
    // THEN the refusal says why, offers to copy the detail, and there is no install action
    await expect(page.getByTestId("package-preview-refusal")).toHaveAttribute("data-code", "invalid_signature");
    await expect(page.getByTestId("package-preview-copy-detail")).toBeVisible();
    await expect(page.getByTestId("package-preview-install")).toHaveCount(0);
  });

  test("an unsigned package the policy refuses points to the policy", async ({ page }) => {
    // GIVEN an unsigned package under a policy that requires a signature
    await openPreview(page, "/files/policy.apo");

    // WHEN its preview is read
    // THEN it reads as the policy refusal, with the way to the policy and no install action
    await expect(page.getByTestId("package-preview-refusal")).toHaveAttribute("data-code", "signature_required");
    await expect(page.getByTestId("package-preview-see-policy")).toBeVisible();
    await expect(page.getByTestId("package-preview-install")).toHaveCount(0);
  });

  test("a signed publisher not trusted yet is trusted from the preview", async ({ page }) => {
    // GIVEN a package signed by a key the keyring does not hold
    await openPreview(page, "/files/stranger.apo");
    await expect(page.getByTestId("package-preview-publisher")).toHaveAttribute("data-state", "signed_untrusted");

    // WHEN the user trusts its publisher
    await page.getByTestId("package-preview-trust-button").click();

    // THEN the key that signed is imported under the publisher's name
    await expect.poll(async () => (await calls(page)).map((c) => c.cmd)).toEqual(["trust_package_publisher"]);
    const [trust] = await calls(page);
    expect(trust.args).toEqual({ publicKey: "ab".repeat(32), label: "Acme" });
  });

  test("an update puts forward what it adds, to accept before updating", async ({ page }) => {
    // GIVEN a newer version of an installed package that adds an access and a connection
    await openPreview(page, "/files/crm-1.1.0.apo");
    const action = page.getByTestId("package-preview-install");

    // WHEN nothing is accepted yet
    // THEN the additions are put forward and the update is disabled
    await expect(page.getByTestId("package-preview-update-changes")).toBeVisible();
    await expect(action).toBeDisabled();

    // WHEN the new access alone is accepted
    await page.getByTestId("package-preview-accept-access:mail_send").click();
    // THEN the update stays disabled
    await expect(action).toBeDisabled();

    // WHEN the new connection is accepted too
    await page.getByTestId("package-preview-accept-connection:notion").click();
    // THEN the update proceeds with the changes confirmed
    await expect(action).toBeEnabled();
    await action.click();
    await expect(page.getByTestId("package-preview-dialog")).toBeHidden();
    const sent = await calls(page);
    expect(sent).toEqual([
      { cmd: "install_package_file", args: { path: "/files/crm-1.1.0.apo", depsConfirmed: false, changesConfirmed: true } },
    ]);
  });
});
