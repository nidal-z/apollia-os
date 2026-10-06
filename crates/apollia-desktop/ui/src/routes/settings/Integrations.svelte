<script lang="ts">
  /**
   * Settings > Integrations: the defaults Apollia grants (the accesses of a
   * new agent created without code, the profile in a new conversation, and,
   * with the developer mode on, the MCP servers a request may bring), a
   * read-only summary of what is always authorized, and the links to the
   * Authorizations and Connections screens, where the detail lives.
   *
   * Each default is written as soon as it is changed. A value the
   * organization policy imposes is shown locked.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Bot, Code, Lock, MessageSquare, Plug, ShieldCheck } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { addToast } from "$lib/components/ui/toast";
  import { listFamilyConsents, type FamilyConsent } from "$lib/ipc/permissions";
  import { describeTool, listTools } from "$lib/ipc/tools";
  import { agents } from "$lib/stores/agents";
  import { developerMode } from "$lib/stores/developerMode";
  import { shareProfileDefault } from "$lib/stores/shareProfileDefault";
  import {
    INTEGRATIONS_BLOCKS,
    INTEGRATIONS_TEST_IDS,
    INTEGRATION_DEFAULTS,
    alwaysSummary,
    defaultAccessRows,
    openAuthorizations,
    openConnections,
    readIntegrationDefaults,
    rowLocked,
    toggledAccess,
    writeAllowRequestMcp,
    writeDefaultAccess,
    writeShareProfile,
    type DefaultAccessRow,
    type DefaultAccessTool,
    type IntegrationDefaults,
  } from "$lib/settings/integrations";
  import { APOLLIA_AGENT_ID, readableName } from "../../components/agents/sheet/agentSheet";
  import SettingsFieldRow from "../../components/settings/SettingsFieldRow.svelte";
  import SettingsSection from "../../components/settings/SettingsSection.svelte";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingsToggle from "../../components/settings/SettingsToggle.svelte";

  let defaults = $state<IntegrationDefaults>(INTEGRATION_DEFAULTS);
  let tools = $state<DefaultAccessTool[]>([]);
  let toolsRead = $state<"loading" | "read" | "failed">("loading");
  let consents = $state<FamilyConsent[]>([]);
  let writing = $state(false);

  const rows = $derived(defaultAccessRows(tools, defaults.families, defaults.access));
  const accessLocked = $derived(defaults.accessSource === "policy");
  const always = $derived(
    alwaysSummary(consents, (id) =>
      id === APOLLIA_AGENT_ID
        ? $t("authorizations.apollia_name")
        : $agents.find((agent) => agent.name === id)?.name,
    ),
  );

  /**
   * The name an access is shown by: its catalogue label, or its identifier
   * made readable the way an agent sheet shows it.
   */
  function labelOf(id: string): string {
    const key = `tools.labels.${id}`;
    const label = $t(key);
    return label === key ? readableName(id) : label;
  }

  async function loadTools(): Promise<void> {
    try {
      const listed = await listTools();
      const native = listed.filter((tool) => tool.kind === "native");
      tools = await Promise.all(
        native.map(async (tool): Promise<DefaultAccessTool> => {
          const descriptor = await describeTool(tool.name).catch(() => null);
          return { name: tool.name, family: descriptor?.family ?? "unclassified" };
        }),
      );
      toolsRead = "read";
    } catch {
      toolsRead = "failed";
    }
  }

  onMount(() => {
    void readIntegrationDefaults().then((read) => {
      defaults = read;
      shareProfileDefault.set(read.shareProfile);
    });
    void loadTools();
    // Without the list the summary shows its empty state; the link still leads
    // to the screen that holds the truth.
    void listFamilyConsents()
      .then((listed) => (consents = listed))
      .catch(() => (consents = []));
  });

  /**
   * Shows `next` at once, then writes it; a refused write puts the section
   * back as it was and says so.
   */
  async function change(
    next: IntegrationDefaults,
    write: (current: IntegrationDefaults) => Promise<IntegrationDefaults>,
  ): Promise<void> {
    if (writing) return;
    const previous = defaults;
    defaults = next;
    writing = true;
    try {
      defaults = await write(previous);
      shareProfileDefault.set(defaults.shareProfile);
    } catch {
      defaults = previous;
      addToast($t("settings.integrations_section.write_error"), "error", {
        "data-testid": "integrations-write-error",
      });
    } finally {
      writing = false;
    }
  }

  function toggleAccess(row: DefaultAccessRow, on: boolean): void {
    if (rowLocked(row, defaults.accessSource)) return;
    void change({ ...defaults, access: toggledAccess(defaults.access, row.id, on) }, (current) =>
      writeDefaultAccess(current, row.id, on),
    );
  }

  function toggleShareProfile(on: boolean): void {
    void change({ ...defaults, shareProfile: on }, (current) => writeShareProfile(current, on));
  }

  function toggleRequestMcp(on: boolean): void {
    void change({ ...defaults, allowRequestMcp: on }, (current) =>
      writeAllowRequestMcp(current, on),
    );
  }
</script>

<SettingsSubPage route="integrations" data-testid={INTEGRATIONS_TEST_IDS.section}>
  <SettingsSection title={$t(INTEGRATIONS_BLOCKS[0])}>
    {#snippet icon()}<Bot size={15} strokeWidth={1.75} />{/snippet}
    <div class="flex flex-col gap-3" data-testid={INTEGRATIONS_TEST_IDS.access}>
      <div>
        <p class="text-label-md text-foreground">
          {$t("settings.integrations_section.access_label")}
        </p>
        <p class="mt-0.5 text-caption text-muted-foreground">
          {$t("settings.integrations_section.access_hint")}
        </p>
        {#if accessLocked}
          <p
            class="mt-1 inline-flex items-center gap-1.5 text-caption text-muted-foreground"
            title={$t("settings.policy_locked_hint")}
            data-testid={INTEGRATIONS_TEST_IDS.accessPolicy}
          >
            <Lock class="h-3 w-3" aria-hidden="true" />
            {$t("settings.policy_locked")}
          </p>
        {/if}
      </div>
      {#if toolsRead === "failed"}
        <p class="text-caption text-muted-foreground" data-testid="integrations-access-error">
          {$t("settings.integrations_section.access_error")}
        </p>
      {:else if toolsRead === "read" && rows.length === 0}
        <p class="text-caption text-muted-foreground" data-testid="integrations-access-empty">
          {$t("settings.integrations_section.access_empty")}
        </p>
      {:else}
        <ul class="flex flex-col border-t border-border/60">
          {#each rows as row (row.id)}
            <li
              class="flex items-center gap-3 border-b border-border/60 py-2 last:border-b-0"
              title={row.byFamily ? $t("settings.integrations_section.access_by_family") : undefined}
            >
              <Checkbox
                id="integrations-access-{row.id}"
                checked={row.checked}
                disabled={rowLocked(row, defaults.accessSource) || writing}
                onchange={(on) => toggleAccess(row, on)}
                data-testid="integrations-access-{row.id}"
              />
              <label
                for="integrations-access-{row.id}"
                class="min-w-0 flex-1 truncate text-body-sm text-foreground"
              >
                {labelOf(row.id)}
              </label>
              {#if row.sensitive}
                <Badge variant="outline" data-testid="integrations-access-{row.id}-sensitive">
                  {$t("settings.integrations_section.access_sensitive")}
                </Badge>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </SettingsSection>

  <SettingsSection title={$t(INTEGRATIONS_BLOCKS[1])}>
    {#snippet icon()}<MessageSquare size={15} strokeWidth={1.75} />{/snippet}
    <SettingsToggle
      id="integrations-share-profile"
      label={$t("settings.integrations_section.share_profile_label")}
      description={$t("settings.integrations_section.share_profile_hint")}
      checked={defaults.shareProfile}
      source={defaults.shareProfileSource}
      disabled={writing}
      onChange={toggleShareProfile}
      data-testid={INTEGRATIONS_TEST_IDS.shareProfile}
    />
  </SettingsSection>

  <SettingsSection title={$t(INTEGRATIONS_BLOCKS[2])} bodyLayout="flush">
    {#snippet icon()}<ShieldCheck size={15} strokeWidth={1.75} />{/snippet}
    {#if always.length === 0}
      <div class="border-b border-border/60 py-3" data-testid={INTEGRATIONS_TEST_IDS.alwaysEmpty}>
        <p class="text-label-md text-foreground">
          {$t("settings.integrations_section.always_empty_label")}
        </p>
        <p class="mt-0.5 text-caption text-muted-foreground">
          {$t("settings.integrations_section.always_empty_hint")}
        </p>
      </div>
    {:else}
      <ul data-testid={INTEGRATIONS_TEST_IDS.always}>
        {#each always as line (line.agentId)}
          <li
            class="flex items-center justify-between gap-5 border-b border-border/60 py-3"
            data-testid="integrations-always-{line.agentId}"
          >
            <span class="min-w-0 flex-1 truncate text-label-md text-foreground">{line.name}</span>
            <span class="flex-none text-body-sm text-muted-foreground">
              {$t("settings.integrations_section.always_count", { values: { count: line.count } })}
            </span>
          </li>
        {/each}
      </ul>
    {/if}
    <SettingsFieldRow
      label={$t("settings.integrations_section.always_manage_label")}
      hint={$t("settings.integrations_section.always_manage_hint")}
      border={false}
    >
      {#snippet control()}
        <Button
          variant="outline"
          size="sm"
          onclick={openAuthorizations}
          data-testid={INTEGRATIONS_TEST_IDS.openAuthorizations}
        >
          {$t("settings.integrations_section.always_manage_action")}
        </Button>
      {/snippet}
    </SettingsFieldRow>
  </SettingsSection>

  <SettingsSection title={$t(INTEGRATIONS_BLOCKS[3])} bodyLayout="flush">
    {#snippet icon()}<Plug size={15} strokeWidth={1.75} />{/snippet}
    <SettingsFieldRow
      label={$t("settings.integrations_section.connections_label")}
      hint={$t("settings.integrations_section.connections_hint")}
      border={false}
    >
      {#snippet control()}
        <Button
          variant="outline"
          size="sm"
          onclick={openConnections}
          data-testid={INTEGRATIONS_TEST_IDS.openConnections}
        >
          {$t("settings.integrations_section.connections_action")}
        </Button>
      {/snippet}
    </SettingsFieldRow>
  </SettingsSection>

  {#if $developerMode}
    <SettingsSection title={$t(INTEGRATIONS_BLOCKS[4])}>
      {#snippet icon()}<Code size={15} strokeWidth={1.75} />{/snippet}
      <SettingsToggle
        id="integrations-allow-request-mcp"
        label={$t("dev.integrations.request_mcp_label")}
        description={$t("dev.integrations.request_mcp_hint")}
        checked={defaults.allowRequestMcp}
        source={defaults.allowRequestMcpSource}
        disabled={writing}
        onChange={toggleRequestMcp}
        data-testid={INTEGRATIONS_TEST_IDS.requestMcp}
      />
    </SettingsSection>
  {/if}
</SettingsSubPage>
