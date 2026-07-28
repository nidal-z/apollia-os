<script lang="ts">
  /**
   * AddRuleForm - create a persistent permission rule (global / project scope).
   *
   * Renders the code-executor invariant as a hard client guard: an "allow"
   * rule for bash / python with an empty argument prefix is refused before it
   * ever reaches the backend, the prefix field is flagged, and a clear error
   * toast explains why. Session / agent rules are not creatable here (they are
   * born of HITL), matching the `add_permission_prefix_rule` contract.
   */
  import { tick } from "svelte";
  import { t } from "svelte-i18n";
  import { Check, XCircle } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import {
    addPermissionPrefixRule,
    isCodeExecutor,
  } from "$lib/ipc/permissions";

  interface Props {
    availableTools: string[];
    initial?: {
      tool: string;
      action: "allow" | "deny";
      scope: "global" | "project";
      argPrefix: string;
      projectPath: string;
    } | null;
    onCancel: () => void;
    onCreated: () => void;
  }

  let { availableTools, initial = null, onCancel, onCreated }: Props = $props();

  // The form is remounted (keyed on a seed in the parent) whenever `initial`
  // changes, so seeding local state from the prop once is intentional.
  // svelte-ignore state_referenced_locally
  let tool = $state(initial?.tool ?? "");
  // svelte-ignore state_referenced_locally
  let action = $state<"allow" | "deny">(initial?.action ?? "allow");
  // svelte-ignore state_referenced_locally
  let scope = $state<"global" | "project">(initial?.scope ?? "global");
  // svelte-ignore state_referenced_locally
  let argPrefix = $state(initial?.argPrefix ?? "");
  // svelte-ignore state_referenced_locally
  let projectPath = $state(initial?.projectPath ?? "");
  let submitting = $state(false);
  let prefixInvalid = $state(false);

  const executorBlanket = $derived(
    isCodeExecutor(tool) && action === "allow" && argPrefix.trim() === "",
  );

  const canSubmit = $derived(
    tool.trim() !== "" && (scope !== "project" || projectPath.trim() !== ""),
  );

  async function submit(): Promise<void> {
    if (!canSubmit || submitting) return;
    if (executorBlanket) {
      prefixInvalid = true;
      addToast(
        $t("settings.permissions.executor_blanket_error", { values: { tool } }),
        "error",
        { "data-testid": "permission-rule-executor-error-toast" },
      );
      await tick();
      (
        document.getElementById(
          "permission-rule-add-arg-prefix",
        ) as HTMLInputElement | null
      )?.focus();
      return;
    }
    prefixInvalid = false;
    submitting = true;
    try {
      await addPermissionPrefixRule({
        toolName: tool,
        argPrefix: argPrefix.trim() === "" ? null : argPrefix.trim(),
        action,
        scope,
        projectPath: scope === "project" ? projectPath.trim() : null,
      });
      addToast($t("settings.permissions.add.created_toast"), "success", {
        "data-testid": "permission-rule-add-toast",
      });
      onCreated();
    } catch (err) {
      reportError(err, {
        surface: "toast",
        testid: "permission-rule-add-error-toast",
      });
    } finally {
      submitting = false;
    }
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      onCancel();
    } else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      void submit();
    }
  }

  const segBase =
    "flex-1 rounded-md px-3 py-1.5 text-body-xs font-medium outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/60";
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<form
  class="grid gap-3 rounded-lg border border-border bg-muted/30 p-4 sm:grid-cols-2"
  data-testid="permission-rule-add-form"
  onsubmit={(e) => {
    e.preventDefault();
    void submit();
  }}
  onkeydown={onKeydown}
>
  <div class="space-y-1.5">
    <label
      class="text-overline uppercase text-muted-foreground"
      for="permission-rule-add-tool"
    >
      {$t("settings.permissions.add.tool_label")}
    </label>
    <Select id="permission-rule-add-tool" bind:value={tool} data-testid="permission-rule-add-tool">
      <option value="" disabled>{$t("settings.permissions.add.tool_placeholder")}</option>
      {#each availableTools as name (name)}
        <option value={name}>{name}</option>
      {/each}
    </Select>
  </div>

  <div class="space-y-1.5">
    <span class="text-overline uppercase text-muted-foreground">
      {$t("settings.permissions.add.action_label")}
    </span>
    <div
      class="flex gap-1 rounded-lg border border-border bg-surface-2 p-1"
      role="group"
      aria-label={$t("settings.permissions.add.action_label")}
      data-testid="permission-rule-add-action"
    >
      <button
        type="button"
        class="{segBase} {action === 'allow'
          ? 'bg-success/15 text-success'
          : 'text-muted-foreground hover:text-foreground'}"
        aria-pressed={action === "allow"}
        onclick={() => (action = "allow")}
        data-testid="permission-rule-add-action-allow"
      >
        <Check size={12} class="mr-1 inline" aria-hidden="true" />
        {$t("settings.permissions.add.action_allow")}
      </button>
      <button
        type="button"
        class="{segBase} {action === 'deny'
          ? 'bg-destructive/15 text-destructive'
          : 'text-muted-foreground hover:text-foreground'}"
        aria-pressed={action === "deny"}
        onclick={() => (action = "deny")}
        data-testid="permission-rule-add-action-deny"
      >
        <XCircle size={12} class="mr-1 inline" aria-hidden="true" />
        {$t("settings.permissions.add.action_deny")}
      </button>
    </div>
  </div>

  <div class="space-y-1.5">
    <label
      class="text-overline uppercase text-muted-foreground"
      for="permission-rule-add-scope"
    >
      {$t("settings.permissions.add.scope_label")}
    </label>
    <Select id="permission-rule-add-scope" bind:value={scope} data-testid="permission-rule-add-scope">
      <option value="global">{$t("settings.permissions.scope_global")}</option>
      <option value="project">{$t("settings.permissions.scope_project")}</option>
    </Select>
  </div>

  <div class="space-y-1.5">
    <label
      class="text-overline uppercase text-muted-foreground"
      for="permission-rule-add-arg-prefix"
    >
      {$t("settings.permissions.add.arg_prefix_label")}
    </label>
    <Input
      id="permission-rule-add-arg-prefix"
      bind:value={argPrefix}
      oninput={() => (prefixInvalid = false)}
      class={prefixInvalid ? "border-destructive focus-visible:ring-destructive" : ""}
      aria-invalid={prefixInvalid || undefined}
      placeholder={$t("settings.permissions.add.arg_prefix_placeholder")}
      data-testid="permission-rule-add-arg-prefix"
    />
  </div>

  {#if executorBlanket}
    <p
      class="flex items-start gap-1.5 text-caption text-warning-a11y sm:col-span-2"
      data-testid="permission-rule-executor-warning"
    >
      <XCircle size={13} class="mt-px shrink-0" aria-hidden="true" />
      <span>{$t("settings.permissions.executor_blanket_error", { values: { tool } })}</span>
    </p>
  {/if}

  {#if scope === "project"}
    <div class="space-y-1.5 sm:col-span-2">
      <label
        class="text-overline uppercase text-muted-foreground"
        for="permission-rule-add-project-path"
      >
        {$t("settings.permissions.add.project_path_label")}
      </label>
      <Input
        id="permission-rule-add-project-path"
        bind:value={projectPath}
        placeholder={$t("settings.permissions.add.project_path_placeholder")}
        data-testid="permission-rule-add-project-path"
      />
    </div>
  {/if}

  <div class="flex justify-end gap-2 sm:col-span-2">
    <Button
      type="button"
      variant="ghost"
      size="sm"
      onclick={onCancel}
      data-testid="permission-rule-add-cancel"
    >
      {$t("common.cancel")}
    </Button>
    <Button
      type="submit"
      size="sm"
      loading={submitting}
      disabled={!canSubmit || submitting}
      data-testid="permission-rule-add-submit"
    >
      {$t("settings.permissions.add.submit")}
    </Button>
  </div>
</form>
