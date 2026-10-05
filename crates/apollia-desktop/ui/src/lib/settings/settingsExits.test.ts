/**
 * What leaves the settings and what stays: the OAuth integrations live on the
 * Connections screen, the built-in tools are a source of tools on that same
 * screen, the permission rules live on the Authorizations screen, the facts the agents retained stay in Settings > Profile,
 * clearing every memory stays in the danger zone of General, and the Memory
 * screen leads to the Profile section rather than to an address nothing serves.
 *
 * Components cannot be mounted under this Vitest configuration (node
 * environment, no rendering library), so the pure logic a component exports is
 * driven directly and the markup is read from the sources the bundle is built
 * from.
 */
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { get } from "svelte/store";
import { SETTINGS_NAV } from "$lib/settings/settingsNav";
import {
  DEFAULT_SETTINGS_SUB_ROUTE,
  SETTINGS_SUB_ROUTES,
  settingsSubRoute,
} from "$lib/stores/settings";
import { closeSettingsWindow, settingsWindow } from "$lib/stores/settingsWindow";
import { navigateToSettings } from "$lib/router";
import { currentRoute, navigateTo } from "$lib/stores/navigation";
import {
  factRowView,
  forgetFact,
} from "../../components/settings/profile/ProfileFactRow.svelte";

const SRC = resolve(__dirname, "../..");

function source(rel: string): string {
  return readFileSync(resolve(SRC, rel), "utf8");
}

function reset(): void {
  closeSettingsWindow();
  navigateTo("dashboard");
  settingsSubRoute.set(DEFAULT_SETTINGS_SUB_ROUTE);
}

beforeEach(reset);
afterEach(reset);

const FACT = {
  key: "prefs.schedule",
  value: "No meetings before 10 am",
};

describe("the OAuth integrations leave the settings (criterion 2)", () => {
  it("lists no OAuth section in the settings and ships no such page", () => {
    // GIVEN the settings sub-routes and the navigation column
    const keys = SETTINGS_NAV.flatMap((g) => g.entries.map((e) => e.key as string));

    // WHEN the former section is looked for
    // THEN neither the store, the column, the window nor the tree carries it
    expect(SETTINGS_SUB_ROUTES as string[]).not.toContain("integrations");
    expect(keys).not.toContain("integrations");
    expect(source("routes/Settings.svelte")).not.toContain("settings/Integrations.svelte");
    expect(existsSync(resolve(SRC, "routes/settings/Integrations.svelte"))).toBe(false);
  });

  it("shows and disconnects a connected account on the Connections screen", () => {
    // GIVEN the page of a native connection on the Connections screen
    const detail = source("components/connections/detail/NativeConnectorDetail.svelte");
    const accounts = source("components/connections/detail/NativeAccountsTab.svelte");

    // WHEN its accounts tab and its client section are read
    // THEN each account row carries its disconnect action, and the OAuth
    // client is entered on the same page
    expect(detail).toContain("<NativeAccountsTab");
    expect(detail).toContain("<NativeClientSection");
    expect(accounts).toContain("onclick={() => ondisconnect(account)}");
    expect(accounts).toContain('data-testid="oauth-disconnect-{account.account_id}"');
  });
});

describe("the facts the agents retained stay in Profile (criterion 3)", () => {
  it("shows the value alone with developer mode off", () => {
    // GIVEN a fact an agent retained
    // WHEN the row is built with developer mode off
    const view = factRowView(FACT, false);

    // THEN the value shows and the key does not
    expect(view).toEqual({ value: "No meetings before 10 am", key: null });
  });

  it("shows the key next to the value with developer mode on", () => {
    // GIVEN the same fact
    // WHEN the row is built with developer mode on
    const view = factRowView(FACT, true);

    // THEN the key is shown as well
    expect(view).toEqual({ value: "No meetings before 10 am", key: "prefs.schedule" });
  });

  it("forgets a fact in the store, then drops its row", async () => {
    // GIVEN a store that accepts the deletion, and the list of the card
    const calls: string[] = [];
    const remove = async (key: string): Promise<void> => {
      calls.push(`remove:${key}`);
    };
    const onDeleted = (key: string): void => {
      calls.push(`row:${key}`);
    };

    // WHEN the deletion is confirmed
    await forgetFact(FACT.key, remove, onDeleted);

    // THEN the store forgot it first, then the row left the list
    expect(calls).toEqual(["remove:prefs.schedule", "row:prefs.schedule"]);
  });

  it("keeps the row when the store refuses the deletion", async () => {
    // GIVEN a store that refuses
    const dropped: string[] = [];
    const remove = async (): Promise<void> => {
      throw new Error("NOT_FOUND");
    };

    // WHEN the deletion is confirmed
    const outcome = forgetFact(FACT.key, remove, (key) => dropped.push(key));

    // THEN the error surfaces and the row stays
    await expect(outcome).rejects.toThrow("NOT_FOUND");
    expect(dropped).toEqual([]);
  });

  it("is mounted by the Profile section with the facts of the profile, not by Memory", () => {
    // GIVEN the Profile section, the facts card and the Memory screen
    const profile = source("routes/settings/Profile.svelte");
    const card = source("components/settings/profile/ProfileMemoryFactsCard.svelte");
    const row = source("components/settings/profile/ProfileFactRow.svelte");
    const memory = source("routes/Memory.svelte");

    // WHEN the facts are traced from the profile to the delete confirmation
    // THEN Profile feeds the card the facts of the displayed profile, each row reads developer mode
    // and names the value in its confirmation, and Memory hosts none of it
    expect(profile).toContain("facts={detail.facts}");
    expect(profile).toContain("<ProfileMemoryFactsCard");
    expect(card).toContain("<ProfileFactRow {profileId} {fact} {onUpdated} {onDeleted} />");
    expect(row).toContain("factRowView(fact, $developerMode)");
    expect(row).toContain(
      "forgetFact(fact.key, (key) => deleteProfileFact(profileId, key), onDeleted)",
    );
    expect(row).toContain("values: { value: fact.value }");
    expect(memory).not.toContain("ProfileMemoryFactsCard");
  });
});

describe("clearing every memory stays in General (criterion 4)", () => {
  it("is an action of the danger zone General mounts, behind a typed confirmation", () => {
    // GIVEN General, its danger zone and the Memory screen
    const general = source("routes/settings/General.svelte");
    const danger = source("components/settings/DangerZone.svelte");
    const memory = source("routes/Memory.svelte");

    // WHEN the clear-memories action is located
    // THEN General mounts the danger zone, the action asks for a typed word
    // through the destructive dialog, and the Memory screen does not carry it
    expect(general).toContain("<DangerZone />");
    expect(danger).toMatch(
      /id: "clear-memories", key: "clear_memories", icon: Brain, confirmWord: "DELETE"/,
    );
    expect(danger).toContain("confirmWord={action.confirmWord}");
    expect(danger).toContain("const count = await clearAllMemories();");
    expect(memory).not.toContain("clearAllMemories");
  });
});

describe("the Memory screen leads to Settings > Profile (criterion 5)", () => {
  it("opens the settings window on Profile instead of a dead address", () => {
    // GIVEN the profile banner of the Memory screen
    const memory = source("routes/Memory.svelte");
    expect(memory).not.toContain('href="/settings/profile"');
    expect(memory).toContain('onclick={() => navigateToSettings("profile")}');
    expect(memory).toContain('data-testid="memory-profile-link"');

    // WHEN its link runs
    navigateToSettings("profile");

    // THEN the settings window is open on the Profile section
    expect(get(settingsWindow)).toEqual({ open: true, section: "profile" });
  });
});

describe("the tools and the permissions leave the settings", () => {
  it.each(["tools", "permissions"])("lists no %s section and ships no such page", (gone) => {
    // GIVEN the settings sub-routes, the navigation column and the window
    const keys = SETTINGS_NAV.flatMap((g) => g.entries.map((e) => e.key as string));
    const page = `${gone[0].toUpperCase()}${gone.slice(1)}.svelte`;

    // WHEN the former section is looked for
    // THEN neither the store, the column, the window nor the tree carries it
    expect(SETTINGS_SUB_ROUTES as string[]).not.toContain(gone);
    expect(keys).not.toContain(gone);
    expect(source("routes/Settings.svelte")).not.toContain(`settings/${page}`);
    expect(existsSync(resolve(SRC, `routes/settings/${page}`))).toBe(false);
  });

  it.each(["fr", "en"])("names neither section in the %s column labels", (locale) => {
    // GIVEN one catalogue
    const nav = JSON.parse(source(`lib/i18n/${locale}.json`)).settings.nav as Record<string, string>;

    // WHEN the labels of the two former sections are looked for
    // THEN the column has no word left for them
    expect(nav).not.toHaveProperty("tools");
    expect(nav).not.toHaveProperty("permissions");
  });

  it("keeps every action of the tools section on the Connections screen", () => {
    // GIVEN the page of the built-in tools on the Connections screen, and the catalogue it mounts
    const detail = source("components/connections/detail/BuiltinToolsDetail.svelte");
    const catalog = source("components/tools/BuiltinToolsCatalog.svelte");
    const row = source("components/settings/tools/ToolRow.svelte");

    // WHEN the actions the settings section offered are traced there
    // THEN the catalogue is mounted, with the contract in developer mode, and
    // it still filters, reloads, turns a tool on, configures it and lists the credentials
    expect(detail).toContain("<BuiltinToolsCatalog showContract={$developerMode} />");
    expect(catalog).toContain('placeholder={$t("settings.tools_page.filter_placeholder")}');
    expect(catalog).toContain('{$t("settings.tools_page.reload")}');
    expect(catalog).toContain("<ToolRow");
    expect(catalog).toContain("<CredentialInventory");
    expect(catalog).toContain("<ToolContractSheet");
    expect(row).toContain('$t("settings.tools_page.configure")');
    expect(row).toContain('$t("settings.tools_page.ctx_enable")');
  });

  it("creates and withdraws a rule on the Authorizations screen", () => {
    // GIVEN the Authorizations screen and the row of a fine rule
    const screen = source("components/authorizations/AuthorizationsScreen.svelte");
    const rule = source("components/authorizations/FineRuleRow.svelte");

    // WHEN the two rule actions the settings section offered are looked for
    // THEN the screen adds a rule and each row withdraws its own
    expect(screen).toContain('data-testid="authorizations-add-rule"');
    expect(rule).toContain('data-testid="authorizations-rule-withdraw"');
  });

  it.each([
    "components/integrations/ConnectorWizard.svelte",
    "components/integrations/MailboxDialog.svelte",
    "components/packages/PackagePreviewDialog.svelte",
    "routes/Connections.svelte",
  ])("%s leads a refusal to the Authorizations screen, not to the settings", (file) => {
    // GIVEN a surface that shows the rule behind a refusal
    const text = source(file);

    // WHEN its link to the rule is read
    // THEN it opens the Authorizations screen and no longer names a settings section
    expect(text).toContain('navigateTo("authorizations")');
    expect(text).not.toContain('openSettingsWindow("permissions")');
  });

  it("lands a caller that still names a former section on the default one", () => {
    // GIVEN a caller that still asks for the permissions section, from the dashboard
    expect(get(currentRoute)).toBe("dashboard");

    // WHEN it opens the settings on it
    navigateToSettings("permissions");

    // THEN the window opens on the default section rather than on a missing page
    expect(get(settingsWindow)).toEqual({ open: true, section: DEFAULT_SETTINGS_SUB_ROUTE });
  });
});
