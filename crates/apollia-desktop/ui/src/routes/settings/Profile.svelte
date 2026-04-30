<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.profile",
    icon: "user",
    group: "settings.nav.cluster_personalization",
    cluster: "personalization",
  } as const;
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "svelte-i18n";
  import { User, BookOpen, ShieldCheck } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { addToast } from "$lib/components/ui/toast/store";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import { goToSettingsSubRoute } from "$lib/stores/settings";

  interface UserProfile {
    name: string | null;
    preferences: Record<string, string>;
    habits: Record<string, string>;
    context: Record<string, string>;
  }

  let profile = $state<UserProfile | null>(null);
  let loading = $state(true);
  let editName = $state("");
  let saving = $state(false);

  onMount(async () => {
    try {
      profile = await invoke<UserProfile>("get_user_profile");
      editName = profile?.name ?? "";
    } catch {
      // Non-critical — show empty state
    } finally {
      loading = false;
    }
  });

  async function saveName() {
    if (saving) return;
    saving = true;
    try {
      await invoke("update_user_profile", {
        input: { name: editName.trim() || null, preferences: null, habits: null, context: null },
      });
      addToast($t("settings.profile_saved"), "success");
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      saving = false;
    }
  }

  // ADR-086 — relancer un onboarding ciblé pour réviser les permissions
  // après modification d'une préférence (souveraineté, supervision HITL).
  let triggeringOnboarding = $state(false);
  async function triggerPermissionsOnboarding() {
    if (triggeringOnboarding) return;
    triggeringOnboarding = true;
    try {
      await invoke("trigger_onboarding", { topic: "permissions", profile: null });
      addToast(
        "L'onboarding-agent va te proposer les ajustements de permissions correspondant à ton profil actuel.",
        "success",
      );
    } catch (err: unknown) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      triggeringOnboarding = false;
    }
  }

  function countEntries(map: Record<string, string>): number {
    return Object.keys(map).length;
  }
</script>

{#if loading}
  <SettingSectionSkeleton />
{:else}
  <section class="space-y-6" data-testid="profile-section">
    <!-- Identity -->
    <div class="glass-card glass-border rounded-lg p-4 space-y-4">
      <div class="flex items-center gap-2 mb-1">
        <div class="flex h-10 w-10 items-center justify-center rounded-full bg-primary/10 text-primary">
          <User size={20} strokeWidth={1.75} />
        </div>
        <div>
          <p class="text-sm font-semibold">{editName || $t("settings.profile_name_placeholder")}</p>
          <p class="text-xs text-muted-foreground">{$t("settings.profile_subtitle")}</p>
        </div>
      </div>

      <div class="space-y-2">
        <label class="text-xs font-medium text-muted-foreground uppercase tracking-wider">
          {$t("settings.profile_name")}
        </label>
        <div class="flex items-center gap-2">
          <Input
            bind:value={editName}
            placeholder={$t("settings.profile_name_placeholder")}
            class="max-w-sm"
          />
          <Button size="sm" onclick={saveName} disabled={saving}>
            {saving ? $t("settings.profile_saving") : $t("common.save")}
          </Button>
        </div>
      </div>
    </div>

    <!-- Permissions banner — ADR-086 -->
    <div class="glass-card glass-border rounded-lg p-4 space-y-3 border-primary/20">
      <div class="flex items-start gap-3">
        <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-primary/10 text-primary">
          <ShieldCheck size={20} strokeWidth={1.75} />
        </div>
        <div class="flex-1 space-y-2">
          <p class="text-sm font-medium">Permissions dérivées du profil</p>
          <p class="text-xs text-muted-foreground">
            Si tu modifies ta préférence de souveraineté ou de supervision (HITL), tu peux
            demander à l'onboarding-agent de réviser les règles correspondantes. Chaque
            ajustement passera par une confirmation HITL standard.
          </p>
          <div class="flex flex-wrap items-center gap-2 pt-1">
            <Button
              size="sm"
              variant="outline"
              onclick={triggerPermissionsOnboarding}
              disabled={triggeringOnboarding}
              data-testid="profile-permissions-trigger"
            >
              {triggeringOnboarding ? "Démarrage..." : "Réviser les permissions"}
            </Button>
            <button
              type="button"
              class="text-[11px] text-primary hover:text-primary/80"
              onclick={() => goToSettingsSubRoute("memories/permissions")}
            >
              Voir les règles actuelles →
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- Memory summary — link to dedicated tab -->
    {#if profile}
      <div class="glass-card glass-border rounded-lg p-4 space-y-3">
        <div class="flex items-center justify-between">
          <h3 class="flex items-center gap-1.5 text-sm font-medium uppercase tracking-wider text-muted-foreground">
            <BookOpen size={14} />
            {$t("settings.memories")}
          </h3>
          <button
            class="text-[11px] text-primary hover:text-primary/80"
            onclick={() => goToSettingsSubRoute("memories")}
          >
            {$t("common.see_more")} →
          </button>
        </div>
        <div class="grid grid-cols-3 gap-3 text-center">
          <div class="rounded-md bg-muted/30 px-2 py-2">
            <p class="text-lg font-semibold">{countEntries(profile.preferences)}</p>
            <p class="text-[10px] text-muted-foreground">{$t("settings.profile_tab_preferences")}</p>
          </div>
          <div class="rounded-md bg-muted/30 px-2 py-2">
            <p class="text-lg font-semibold">{countEntries(profile.habits)}</p>
            <p class="text-[10px] text-muted-foreground">{$t("settings.profile_tab_habits")}</p>
          </div>
          <div class="rounded-md bg-muted/30 px-2 py-2">
            <p class="text-lg font-semibold">{countEntries(profile.context)}</p>
            <p class="text-[10px] text-muted-foreground">{$t("settings.profile_tab_context")}</p>
          </div>
        </div>
      </div>
    {/if}
  </section>
{/if}
