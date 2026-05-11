<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.memories",
    icon: "brain",
    group: "settings.nav.cluster_system",
    cluster: "system",
  } as const;
</script>

<script lang="ts">
  import { uiMode } from "$lib/stores/mode";
  import { Brain, ExternalLink } from "lucide-svelte";
  import { t } from "svelte-i18n";
  import UserMemoryDashboard from "../../components/memory/UserMemoryDashboard.svelte";
  import { Card } from "$lib/components/operator";
  import { navigateTo } from "$lib/stores/navigation";

  // Liens utiles pour comprendre la mémoire utilisateur
  const learnLinks = [
    {
      key: "preferences",
      labelKey: "settings.memories.learn_preferences",
      defaultLabel: "Préférences",
      defaultDesc: "Goûts personnels (ex: prefère le ton direct, préfère le français).",
    },
    {
      key: "habits",
      labelKey: "settings.memories.learn_habits",
      defaultLabel: "Habitudes",
      defaultDesc: "Routines et patterns (ex: revue chaque matin, run daily à 7h).",
    },
    {
      key: "context",
      labelKey: "settings.memories.learn_context",
      defaultLabel: "Contexte",
      defaultDesc: "Profil et environnement (ex: CTO, stack Rust + Python).",
    },
  ];
</script>

<section data-testid="memories-section-system" class="space-y-5">
  <!-- Header explicatif — donne du contexte sur la mémoire utilisateur -->
  <Card class="overflow-hidden">
    <div class="px-5 py-4 flex items-start gap-3">
      <div class="w-9 h-9 rounded-md bg-primary/10 inline-flex items-center justify-center shrink-0">
        <Brain size={16} class="text-primary" />
      </div>
      <div class="flex-1 min-w-0">
        <h3 class="text-[13px] font-semibold text-foreground">
          {$t('settings.memories.section_title', { default: 'Votre mémoire personnelle' })}
        </h3>
        <p class="text-[11.5px] text-muted-foreground mt-1 leading-relaxed">
          {$t('settings.memories.section_subtitle', {
            default: "Les agents Apollia se souviennent de vos préférences, habitudes et contexte pour adapter leur comportement. Vous gardez le contrôle : éditez, validez ou supprimez chaque entrée individuellement.",
          })}
        </p>
        <div class="mt-3 grid gap-2 sm:grid-cols-3">
          {#each learnLinks as link}
            <div class="rounded-md border border-border/40 bg-surface-1/50 px-3 py-2">
              <div class="flex items-center gap-1.5 mb-0.5">
                <span class="inline-block w-1.5 h-1.5 rounded-full bg-muted-foreground/40"></span>
                <span class="text-[11px] font-medium text-foreground capitalize">
                  {$t(link.labelKey, { default: link.defaultLabel })}
                </span>
              </div>
              <p class="text-[10.5px] text-muted-foreground leading-snug">
                {$t(`${link.labelKey}_desc`, { default: link.defaultDesc })}
              </p>
            </div>
          {/each}
        </div>
        <button
          type="button"
          onclick={() => navigateTo("memory")}
          class="mt-3 inline-flex items-center gap-1 text-[11px] text-primary hover:underline focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 rounded-sm"
          data-testid="settings-memories-go-explorer"
        >
          <ExternalLink size={11} />
          {$t('settings.memories.see_full_explorer', { default: 'Voir l\'explorateur de mémoire complet' })}
        </button>
      </div>
    </div>
  </Card>

  <!-- Dashboard principal — édition fine des entrées user.* -->
  <UserMemoryDashboard mode={$uiMode} />
</section>
