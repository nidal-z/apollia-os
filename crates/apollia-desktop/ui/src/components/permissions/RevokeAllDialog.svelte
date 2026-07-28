<script lang="ts">
  /**
   * RevokeAllDialog - scoped bulk revoke of persistent rules.
   *
   * Immediate and irreversible; the summary shows how many rules the chosen
   * scope affects. Focus rests on Cancel first (the least destructive option).
   */
  import { t } from "svelte-i18n";
  import { ShieldAlert } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Select } from "$lib/components/ui/select";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import { permissionRules, revokeAll } from "$lib/stores/permissions";

  type RevokeAllScope = "project" | "agent" | "global" | "all";

  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();

  const LABEL_KEYS: Record<RevokeAllScope, string> = {
    project: "settings.permissions.scope_project",
    agent: "settings.permissions.scope_agent",
    global: "settings.permissions.scope_global",
    all: "settings.permissions.scope_all",
  };

  let scope = $state<RevokeAllScope>("project");
  let submitting = $state(false);

  const count = $derived.by(() => {
    if (scope === "all") return $permissionRules.length;
    return $permissionRules.filter((r) => r.scope === scope).length;
  });

  function close(): void {
    if (!submitting) onClose();
  }

  async function confirm(): Promise<void> {
    submitting = true;
    try {
      const removed = await revokeAll(scope === "all" ? null : scope);
      addToast(
        $t("settings.permissions.toast_rules_revoked", { values: { count: removed } }),
        "success",
        { "data-testid": "permission-revoke-all-toast" },
      );
      onClose();
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      submitting = false;
    }
  }
</script>

<Dialog
  {open}
  onclose={close}
  size="sm"
  title={$t("settings.permissions.revoke_all")}
  data-testid="permissions-revoke-all-dialog"
>
  <div class="space-y-3 text-body-sm">
    <p class="text-muted-foreground">{$t("settings.permissions.bulk_intro")}</p>
    <div class="space-y-1.5">
      <label class="text-overline uppercase text-muted-foreground" for="permissions-revoke-all-scope">
        {$t("settings.permissions.filter_scope_label")}
      </label>
      <Select
        id="permissions-revoke-all-scope"
        bind:value={scope}
        disabled={submitting}
        data-testid="permissions-revoke-all-scope-select"
      >
        <option value="project">{$t("settings.permissions.scope_project")}</option>
        <option value="agent">{$t("settings.permissions.scope_agent")}</option>
        <option value="global">{$t("settings.permissions.scope_global")}</option>
        <option value="all">{$t("settings.permissions.scope_all")}</option>
      </Select>
    </div>
    <p class="flex items-center gap-1.5 rounded-md bg-muted/40 px-3 py-2 text-body-xs">
      <ShieldAlert size={14} class="shrink-0 text-warning-a11y" aria-hidden="true" />
      <span>
        <strong>{$t(LABEL_KEYS[scope])}</strong> · {$t("settings.permissions.bulk_summary", { values: { count } })}
      </span>
    </p>
  </div>

  <DialogFooter>
    <Button variant="ghost" size="sm" onclick={close} disabled={submitting}>
      {$t("common.cancel")}
    </Button>
    <Button
      variant="destructive"
      size="sm"
      loading={submitting}
      onclick={confirm}
      data-testid="permissions-revoke-all-confirm"
    >
      {$t("permissions.rules.revoke")}
    </Button>
  </DialogFooter>
</Dialog>
