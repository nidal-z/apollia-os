<script lang="ts">
  /**
   * Scope disclosure for the "Always accept" tertiary action of an approval
   * card. Presented as a native <details> so it stays reachable with the
   * keyboard and requires no portal/popover primitive.
   *
   * The default scope is `this_session` - the safest sticky option.
   */

  import { t } from "svelte-i18n";
  import { ChevronDown } from "lucide-svelte";

  export type AlwaysAcceptScope =
    | "this_tool"
    | "this_session"
    | "this_agent"
    | "this_project"
    | "global";

  interface Props {
    value?: AlwaysAcceptScope;
    onchange?: (scope: AlwaysAcceptScope) => void;
    disabled?: boolean;
  }

  let {
    value = $bindable("this_session"),
    onchange,
    disabled = false,
  }: Props = $props();

  const SCOPES: Array<{ key: AlwaysAcceptScope; labelKey: string; descKey: string }> = [
    {
      key: "this_tool",
      labelKey: "approvals.scope.this_tool",
      descKey: "approvals.scope.this_tool_desc",
    },
    {
      key: "this_session",
      labelKey: "approvals.scope.this_session",
      descKey: "approvals.scope.this_session_desc",
    },
    {
      key: "this_agent",
      labelKey: "approvals.scope.this_agent",
      descKey: "approvals.scope.this_agent_desc",
    },
    {
      key: "this_project",
      labelKey: "approvals.scope.this_project",
      descKey: "approvals.scope.this_project_desc",
    },
    {
      key: "global",
      labelKey: "approvals.scope.global",
      descKey: "approvals.scope.global_desc",
    },
  ];

  function select(scope: AlwaysAcceptScope): void {
    if (disabled) return;
    value = scope;
    onchange?.(scope);
  }
</script>

<details class="group rounded-md glass-border-subtle" data-testid="approval-scope-select">
  <summary
    class="flex cursor-pointer items-center gap-1 px-2 py-1 text-caption text-muted-foreground hover:text-foreground"
    aria-label={$t("approvals.scope.open_disclosure")}
  >
    <ChevronDown
      class="h-3 w-3 transition-transform group-open:rotate-0 -rotate-90"
      aria-hidden="true"
    />
    <span>
      {$t("approvals.scope.current", {
        values: { scope: $t(`approvals.scope.${value}`) },
      })}
    </span>
  </summary>

  <div class="flex flex-col gap-0.5 border-t border-border/30 px-2 py-1.5">
    {#each SCOPES as scope (scope.key)}
      <label
        class="flex items-start gap-2 rounded px-1.5 py-1 text-caption hover:bg-muted/40 {value === scope.key ? 'bg-muted/60' : ''}"
      >
        <input
          type="radio"
          name="approval-scope"
          value={scope.key}
          checked={value === scope.key}
          {disabled}
          onchange={() => select(scope.key)}
          class="mt-0.5"
          data-testid="approval-scope-option-{scope.key}"
        />
        <span class="flex-1">
          <span class="block font-medium text-foreground">{$t(scope.labelKey)}</span>
          <span class="block text-micro text-muted-foreground">{$t(scope.descKey)}</span>
        </span>
      </label>
    {/each}
  </div>
</details>
