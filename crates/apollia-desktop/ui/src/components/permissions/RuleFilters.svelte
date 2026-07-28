<script lang="ts">
  /**
   * RuleFilters - scope + tool filter aside for the rules list.
   *
   * Each change re-queries the backend (setScopeFilter / setToolFilter) and
   * reloads the list; the tool options are derived from the loaded rules.
   */
  import { t } from "svelte-i18n";
  import { Select } from "$lib/components/ui/select";
  import {
    permissionRules,
    filterScope,
    filterTool,
    setScopeFilter,
    setToolFilter,
    type PermissionRuleScope,
  } from "$lib/stores/permissions";

  const toolOptions = $derived.by(() => {
    const names = new Set<string>();
    for (const r of $permissionRules) names.add(r.tool_name);
    return Array.from(names).sort();
  });

  function scopeFromValue(value: string): PermissionRuleScope | null {
    if (value === "session" || value === "project" || value === "agent" || value === "global")
      return value;
    return null;
  }

  async function onScope(event: Event): Promise<void> {
    await setScopeFilter(scopeFromValue((event.currentTarget as HTMLSelectElement).value));
  }

  async function onTool(event: Event): Promise<void> {
    const value = (event.currentTarget as HTMLSelectElement).value;
    await setToolFilter(value === "" ? null : value);
  }
</script>

<aside class="space-y-4 rounded-lg border border-border bg-muted/30 p-3">
  <div class="space-y-1.5">
    <label class="text-overline uppercase text-muted-foreground" for="permissions-filter-scope">
      {$t("settings.permissions.filter_scope_label")}
    </label>
    <Select
      id="permissions-filter-scope"
      value={$filterScope ?? ""}
      onchange={onScope}
      data-testid="permissions-filter-scope"
    >
      <option value="">{$t("settings.permissions.filter_scope_all")}</option>
      <option value="project">{$t("settings.permissions.scope_project")}</option>
      <option value="agent">{$t("settings.permissions.scope_agent")}</option>
      <option value="global">{$t("settings.permissions.scope_global")}</option>
    </Select>
  </div>
  <div class="space-y-1.5">
    <label class="text-overline uppercase text-muted-foreground" for="permissions-filter-tool">
      {$t("settings.permissions.filter_tool_label")}
    </label>
    <Select
      id="permissions-filter-tool"
      value={$filterTool ?? ""}
      onchange={onTool}
      data-testid="permissions-filter-tool"
    >
      <option value="">{$t("settings.permissions.filter_tool_all")}</option>
      {#each toolOptions as name (name)}
        <option value={name}>{name}</option>
      {/each}
    </Select>
  </div>
</aside>
