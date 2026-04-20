<script lang="ts" module>
  import type { AlwaysAcceptScope as _Scope } from "$lib/permissions/telemetry";

  export const SCOPES: _Scope[] = [
    "session_only",
    "agent_tool_pair",
    "tool_global",
    "all_always",
  ];
</script>

<script lang="ts">
  /**
   * Scope chooser for "always allow" rules (US-SP42-054, B.51).
   *
   * Four scopes, from safest default (session_only) to most dangerous
   * (all_always — gated by a confirm modal).
   */
  import { t } from "svelte-i18n";
  import { AlertTriangle } from "lucide-svelte";
  import { Select } from "$lib/components/ui/select";
  import type { AlwaysAcceptScope } from "$lib/permissions/telemetry";

  interface Props {
    value: AlwaysAcceptScope;
    onchange: (next: AlwaysAcceptScope) => void;
    disabled?: boolean;
  }

  let { value = $bindable("session_only"), onchange, disabled = false }: Props = $props();

  function labelFor(s: AlwaysAcceptScope): string {
    return $t(`permissions.always.scope.${s}.label`);
  }
  function helpFor(s: AlwaysAcceptScope): string {
    return $t(`permissions.always.scope.${s}.help`);
  }
  function exampleFor(s: AlwaysAcceptScope): string {
    return $t(`permissions.always.scope.${s}.example`);
  }
</script>

<div class="space-y-2" data-testid="always-accept-scope-chooser">
  <label class="text-[11px] uppercase tracking-wide text-muted-foreground" for="always-accept-scope">
    {$t("permissions.always.scope.label")}
  </label>
  <Select
    id="always-accept-scope"
    bind:value
    onchange={(e) => onchange((e.currentTarget as HTMLSelectElement).value as AlwaysAcceptScope)}
    {disabled}
    data-testid="always-accept-scope-select"
  >
    {#each SCOPES as s (s)}
      <option value={s}>{labelFor(s)}</option>
    {/each}
  </Select>

  <p class="text-[12px] text-muted-foreground">{helpFor(value)}</p>
  <p class="text-[11px] text-muted-foreground/80"><em>{$t("permissions.always.scope.example_label")}</em> {exampleFor(value)}</p>

  {#if value === "all_always"}
    <div class="flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/5 px-2 py-1.5 text-[12px]">
      <AlertTriangle size={14} class="mt-0.5 shrink-0 text-destructive" />
      <p class="text-destructive">{$t("permissions.always.scope.all_always.danger")}</p>
    </div>
  {/if}
</div>
