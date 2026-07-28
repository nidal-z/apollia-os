<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { RotateCw, Search, Wrench } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { ErrorBanner, EmptyState } from "$lib/components/operator";
  import { listNavigation } from "$lib/components/operator/listNavigation";
  import { listFlip, rowIn, rowOut } from "$lib/design/listMotion";
  import { reportError } from "$lib/errors/reportError";
  import { addToast } from "$lib/components/ui/toast";
  import { humanizeToolName } from "$lib/tools/tool-display";
  import SettingsSubPage from "../../components/settings/SettingsSubPage.svelte";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import ToolRow from "../../components/settings/tools/ToolRow.svelte";
  import ToolConfigDrawer from "../../components/settings/ToolConfigDrawer.svelte";
  import {
    TOOL_CATEGORIES,
    isConfigurable,
    toolTitleKey,
    toolDescKey,
    defaultToolConfig,
  } from "../../components/settings/tools/toolCatalog";
  import {
    tools,
    loadingTools,
    toolsError,
    loadTools,
    toggleTool,
    updateToolConfig,
    type ToolStatusDto,
  } from "$lib/stores/toolGovernance";

  let drawerOpen = $state(false);
  let drawerTool = $state<ToolStatusDto | null>(null);
  let drawerTitle = $state("");
  let initialized = $state(false);
  let filter = $state("");

  onMount(() => {
    void loadTools().finally(() => {
      initialized = true;
    });
  });

  function displayTitle(name: string): string {
    const key = toolTitleKey(name);
    const value = $t(key);
    return value === key ? humanizeToolName(name) : value;
  }

  function displayDesc(name: string): string {
    const key = toolDescKey(name);
    const value = $t(key);
    return value === key ? "" : value;
  }

  function warningFor(tool: ToolStatusDto): string | null {
    if (tool.name !== "web_search") return null;
    const cfg = tool.config as { backend?: unknown } | null;
    const backend = typeof cfg?.backend === "string" ? cfg.backend : "auto";
    const hasBraveKey = tool.credential_keys.includes("brave.api_key");
    if (backend === "brave" && !hasBraveKey) {
      return $t("settings.tools_page.warning_brave_forced_no_key");
    }
    if (backend === "auto" && !hasBraveKey) {
      return $t("settings.tools_page.warning_brave_auto_fallback");
    }
    return null;
  }

  interface RowView {
    tool: ToolStatusDto;
    title: string;
    description: string;
  }
  interface GroupView {
    id: string;
    labelKey: string;
    rows: RowView[];
  }

  const byName = $derived(new Map($tools.map((tool) => [tool.name, tool] as const)));
  const query = $derived(filter.trim().toLowerCase());

  function rowMatches(tool: ToolStatusDto, title: string, description: string): boolean {
    if (!query) return true;
    return (
      tool.name.toLowerCase().includes(query) ||
      title.toLowerCase().includes(query) ||
      description.toLowerCase().includes(query)
    );
  }

  const groups = $derived.by<GroupView[]>(() => {
    const seen = new Set<string>();
    const out: GroupView[] = [];
    for (const cat of TOOL_CATEGORIES) {
      const rows: RowView[] = [];
      for (const name of cat.tools) {
        const tool = byName.get(name);
        if (!tool) continue;
        seen.add(name);
        const title = displayTitle(name);
        const description = displayDesc(name);
        if (rowMatches(tool, title, description)) rows.push({ tool, title, description });
      }
      if (rows.length) out.push({ id: cat.id, labelKey: cat.labelKey, rows });
    }
    const extra: RowView[] = [];
    for (const tool of $tools) {
      if (seen.has(tool.name)) continue;
      const title = displayTitle(tool.name);
      const description = displayDesc(tool.name);
      if (rowMatches(tool, title, description)) extra.push({ tool, title, description });
    }
    if (extra.length) out.push({ id: "other", labelKey: "settings.tools_page.cat_other", rows: extra });
    return out;
  });

  const activeCount = $derived($tools.filter((tool) => tool.enabled).length);
  const totalCount = $derived($tools.length);
  const visibleCount = $derived(groups.reduce((n, g) => n + g.rows.length, 0));

  const listError = $derived(
    $toolsError ? reportError($toolsError, { surface: "inline" }) : null,
  );

  async function handleToggle(tool: ToolStatusDto, enabled: boolean): Promise<void> {
    try {
      await toggleTool(tool.name, enabled);
    } catch (err) {
      reportError(err, { surface: "toast", testid: "tool-toggle-error-toast" });
    }
  }

  async function resetConfig(tool: ToolStatusDto): Promise<void> {
    const def = defaultToolConfig(tool.name);
    if (!def) return;
    try {
      await updateToolConfig(tool.name, def);
      addToast($t("settings.tools_page.reset_toast"), "success", {
        "data-testid": "tool-reset-toast",
      });
    } catch (err) {
      reportError(err, { surface: "toast", testid: "tool-reset-error-toast" });
    }
  }

  async function copyId(tool: ToolStatusDto): Promise<void> {
    try {
      await navigator.clipboard.writeText(tool.name);
      addToast($t("settings.tools_page.copied_toast"), "info", {
        "data-testid": "tool-copied-toast",
      });
    } catch (err) {
      reportError(err, { surface: "toast", testid: "tool-copy-error-toast" });
    }
  }

  function openDrawer(tool: ToolStatusDto): void {
    drawerTool = tool;
    drawerTitle = displayTitle(tool.name);
    drawerOpen = true;
  }

  function closeDrawer(): void {
    drawerOpen = false;
    void loadTools();
  }

  async function reload(): Promise<void> {
    await loadTools();
    if (!get(toolsError)) {
      addToast($t("settings.tools_page.reloaded_toast"), "info", {
        "data-testid": "tools-reloaded-toast",
      });
    }
  }
</script>

<SettingsSubPage route="tools" data-testid="settings-tools">
  {#if !initialized && $loadingTools}
    <SettingSectionSkeleton rows={5} />
  {:else if listError}
    <ErrorBanner
      message={listError.friendly_message}
      onretry={reload}
      retryLabel={$t("settings.tools_page.reload")}
      loading={$loadingTools}
      data-testid="tools-error"
    />
  {:else if totalCount === 0}
    <EmptyState title={$t("settings.tools_page.empty_title")} desc={$t("settings.tools_page.empty")} tone="neutral">
      {#snippet icon()}<Wrench size={22} strokeWidth={1.75} aria-hidden="true" />{/snippet}
      {#snippet action()}
        <Button variant="outline" size="sm" onclick={reload} loading={$loadingTools} data-testid="tools-empty-reload">
          {#snippet icon()}<RotateCw size={14} strokeWidth={1.9} aria-hidden="true" />{/snippet}
          {$t("settings.tools_page.reload")}
        </Button>
      {/snippet}
    </EmptyState>
  {:else}
    <div class="flex items-center gap-2.5" data-testid="tools-toolbar">
      <div class="max-w-xs flex-1">
        <Input
          icon={Search}
          size="sm"
          type="search"
          placeholder={$t("settings.tools_page.filter_placeholder")}
          aria-label={$t("settings.tools_page.filter_aria")}
          bind:value={filter}
          data-testid="tools-filter"
        />
      </div>
      <span
        class="ml-auto whitespace-nowrap rounded-full border border-border/60 bg-muted/50 px-2.5 py-1 text-caption text-muted-foreground"
        data-testid="tools-count"
      >
        {$t("settings.tools_page.count_active", { values: { active: activeCount, total: totalCount } })}
      </span>
      <Button variant="outline" size="sm" onclick={reload} loading={$loadingTools && initialized} data-testid="tools-reload">
        {#snippet icon()}<RotateCw size={14} strokeWidth={1.9} aria-hidden="true" />{/snippet}
        {$t("settings.tools_page.reload")}
      </Button>
    </div>

    {#if visibleCount === 0}
      <EmptyState title={$t("settings.tools_page.no_results")} tone="neutral">
        {#snippet icon()}<Search size={22} strokeWidth={1.75} aria-hidden="true" />{/snippet}
      </EmptyState>
    {:else}
      <div
        use:listNavigation={{ rowSelector: "[data-tool-row]", idPrefix: "tool-row" }}
        role="group"
        aria-label={$t("settings.tools_page.heading")}
        class="flex flex-col gap-4"
        data-testid="tools-list"
      >
        {#each groups as group (group.id)}
          <section class="flex flex-col gap-1.5" data-testid="tool-group-{group.id}">
            <div class="flex items-center gap-2 px-0.5">
              <h4 class="text-overline font-semibold uppercase text-muted-foreground">
                {$t(group.labelKey)}
              </h4>
              <span class="rounded-full bg-muted/60 px-1.5 text-overline font-semibold text-muted-foreground/80">
                {group.rows.length}
              </span>
              <span class="h-px flex-1 bg-gradient-to-r from-border to-transparent" aria-hidden="true"></span>
            </div>
            {#each group.rows as row (row.tool.name)}
              <div animate:flip={listFlip()} in:fly={rowIn()} out:fly={rowOut()}>
                <ToolRow
                  tool={row.tool}
                  title={row.title}
                  description={row.description}
                  warning={warningFor(row.tool)}
                  canConfigure={isConfigurable(row.tool.name)}
                  busy={$loadingTools && initialized}
                  onToggle={(enabled) => handleToggle(row.tool, enabled)}
                  onConfigure={() => openDrawer(row.tool)}
                  onReset={() => void resetConfig(row.tool)}
                  onCopyId={() => void copyId(row.tool)}
                />
              </div>
            {/each}
          </section>
        {/each}
      </div>
    {/if}
  {/if}
</SettingsSubPage>

<ToolConfigDrawer open={drawerOpen} tool={drawerTool} title={drawerTitle} onclose={closeDrawer} />
