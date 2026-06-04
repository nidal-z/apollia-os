<script lang="ts">
  /**
   * Always-accept rules management page.
   *
   * Route: `/settings/memories/permissions`. Lists every active
   * auto-approval rule with its scope and owners, exposes per-rule
   * revoke plus a "revoke all" action gated by typing the user's name.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { PageLayout } from "$lib/components/operator";
  import { Input } from "$lib/components/ui/input";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import EmptyState from "../components/common/EmptyState.svelte";
  import { ShieldCheck, Trash2 } from "lucide-svelte";
  import {
    alwaysAcceptRules,
    loadAlwaysAcceptRules,
    revokeAlwaysAcceptRule,
    revokeAllAlwaysAcceptRules,
    type AlwaysAcceptRule,
  } from "$lib/stores/permissions/alwaysAcceptRules";
  import { addToast } from "$lib/components/ui/toast/store";

  let loading = $state(true);
  let confirmingAll = $state(false);
  let confirmText = $state("");

  // Filtre rapide par auteur. Permet à l'opérateur de retrouver
  // les règles posées par l'onboarding-agent vs ses propres règles HITL vs
  // celles importées du config.
  let creatorFilter = $state<string>("all");

  const FILTER_PRESETS: Array<{ value: string; label: string }> = [
    { value: "all", label: "Tous" },
    { value: "onboarding-agent", label: "Onboarding" },
    { value: "user-hitl", label: "Vous (HITL)" },
    { value: "user-settings", label: "Vous (Settings)" },
    { value: "config-import", label: "Config import" },
  ];

  const visibleRules = $derived(
    creatorFilter === "all"
      ? $alwaysAcceptRules
      : $alwaysAcceptRules.filter((r) => (r.created_by ?? "") === creatorFilter),
  );

  onMount(async () => {
    await loadAlwaysAcceptRules();
    loading = false;
  });

  async function handleRevoke(rule: AlwaysAcceptRule) {
    try {
      await revokeAlwaysAcceptRule(rule.id);
      addToast($t("permissions.rules.toast.revoked"), "success");
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  async function handleRevokeAll() {
    try {
      await revokeAllAlwaysAcceptRules();
      addToast($t("permissions.rules.toast.revoked_all"), "success");
      confirmingAll = false;
      confirmText = "";
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    }
  }

  function scopeLabel(scope: AlwaysAcceptRule["scope"]): string {
    return $t(`permissions.always.scope.${scope}.label`);
  }

  function formatDate(iso: string): string {
    try {
      return new Date(iso).toLocaleString();
    } catch {
      return iso;
    }
  }
</script>

<PageLayout
  title={$t("permissions.rules.title")}
  subtitle={$t("permissions.rules.subtitle")}
  maxWidth="max-w-4xl"
  data-testid="settings-permission-rules"
>
  <div class="px-8 pt-6 pb-8 space-y-4">
  {#if loading}
    <p class="text-sm text-muted-foreground">{$t("common.loading")}</p>
  {:else if $alwaysAcceptRules.length === 0}
    <EmptyState
      icon={ShieldCheck}
      title={$t("permissions.rules.empty_title")}
      subtitle={$t("permissions.rules.empty_subtitle")}
      page="approvals"
    />
  {:else}
    <div class="flex flex-wrap items-center gap-2 text-xs" data-testid="permission-rule-creator-filter">
      <span class="text-muted-foreground">Filtrer par auteur :</span>
      {#each FILTER_PRESETS as preset}
        <button
          type="button"
          class="rounded-full border px-2.5 py-0.5 transition-colors {creatorFilter === preset.value
            ? 'border-primary bg-primary/10 text-primary'
            : 'border-border bg-card text-muted-foreground hover:bg-muted'}"
          onclick={() => (creatorFilter = preset.value)}
        >
          {preset.label}
        </button>
      {/each}
    </div>

    <div class="rounded-lg border border-border bg-card">
      <table class="w-full text-sm">
        <thead class="text-left text-[11px] uppercase tracking-wide text-muted-foreground">
          <tr>
            <th class="px-3 py-2">{$t("permissions.rules.col_tool")}</th>
            <th class="px-3 py-2">{$t("permissions.rules.col_scope")}</th>
            <th class="px-3 py-2">{$t("permissions.rules.col_agent")}</th>
            <!-- Colonne auteur (created_by) -->
            <th class="px-3 py-2" title="Auteur ayant créé la règle (HITL, agent, config…)">
              Auteur
            </th>
            <th class="px-3 py-2">{$t("permissions.rules.col_created")}</th>
            <th class="px-3 py-2"></th>
          </tr>
        </thead>
        <tbody>
          {#if visibleRules.length === 0}
            <tr>
              <td colspan="6" class="px-3 py-6 text-center text-xs text-muted-foreground">
                Aucune règle pour ce filtre.
              </td>
            </tr>
          {/if}
          {#each visibleRules as rule (rule.id)}
            <tr class="border-t border-border">
              <td class="px-3 py-2 font-mono text-[12px]">{rule.tool}</td>
              <td class="px-3 py-2">{scopeLabel(rule.scope)}</td>
              <td class="px-3 py-2 text-muted-foreground">{rule.agent_id ?? "-"}</td>
              <td class="px-3 py-2 text-muted-foreground font-mono text-[11px]">{rule.created_by ?? "-"}</td>
              <td class="px-3 py-2 text-muted-foreground">{formatDate(rule.created_at)}</td>
              <td class="px-3 py-2 text-right">
                <Button
                  size="sm"
                  variant="ghost"
                  class="text-destructive hover:bg-destructive/10"
                  onclick={() => handleRevoke(rule)}
                  data-testid="permission-rule-revoke"
                >
                  <Trash2 size={14} class="mr-1" />
                  {$t("permissions.rules.revoke")}
                </Button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    <div class="flex justify-end">
      <Button
        variant="outline"
        size="sm"
        class="text-destructive"
        onclick={() => (confirmingAll = true)}
        data-testid="permission-rule-revoke-all"
      >
        {$t("permissions.rules.revoke_all")}
      </Button>
    </div>
  {/if}

  </div>
</PageLayout>

<Dialog
  open={confirmingAll}
  onclose={() => { confirmingAll = false; confirmText = ""; }}
  size="sm"
  title={$t("permissions.rules.revoke_all_confirm_title")}
>
  <p class="text-sm text-muted-foreground">
    {$t("permissions.rules.revoke_all_confirm_body")}
  </p>
  <label class="mt-3 block text-xs text-muted-foreground" for="confirm-type-input">
    {$t("permissions.rules.revoke_all_type_prompt")}
  </label>
  <Input
    id="confirm-type-input"
    bind:value={confirmText}
    type="text"
    class="mt-1"
  />

  <DialogFooter>
    <Button
      variant="ghost"
      onclick={() => { confirmingAll = false; confirmText = ""; }}
    >
      {$t("common.cancel")}
    </Button>
    <Button
      variant="destructive"
      disabled={confirmText.trim().length === 0}
      onclick={handleRevokeAll}
    >
      {$t("permissions.rules.revoke_all")}
    </Button>
  </DialogFooter>
</Dialog>
