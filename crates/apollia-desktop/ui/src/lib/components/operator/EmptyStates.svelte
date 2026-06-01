<script lang="ts">
  import {
    MessageCircle,
    Folder,
    Bot,
    Activity,
    Link as LinkIcon,
    List,
    Check,
  } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";

  /**
   * Gallery component showing the 4+ empty-state archetypes.
   * Standalone display utility - for production usage prefer EmptyState.svelte.
   */

  type Archetype = {
    icon: typeof MessageCircle;
    title: string;
    desc: string;
    action?: string;
    ok?: boolean;
    tone: "primary" | "secondary" | "info" | "success" | "neutral";
  };

  const ARCHETYPES: Archetype[] = [
    {
      icon: MessageCircle,
      title: "Aucune conversation",
      desc: "Lancez un chat pour démarrer - vos échanges s'afficheront ici.",
      action: "+ Nouveau chat",
      tone: "primary",
    },
    {
      icon: Folder,
      title: "Aucun projet",
      desc: "Regroupez vos chats et tâches dans un projet pour garder le fil.",
      action: "+ Nouveau projet",
      tone: "secondary",
    },
    {
      icon: Bot,
      title: "Aucun assistant",
      desc: "Créez un premier assistant ou importez-en depuis la bibliothèque.",
      action: "+ Nouvel assistant",
      tone: "primary",
    },
    {
      icon: Activity,
      title: "Boîte vide - tout est à jour",
      desc: "Rien n'attend votre attention. Les agents vous préviendront ici.",
      ok: true,
      tone: "success",
    },
    {
      icon: LinkIcon,
      title: "Aucune connexion",
      desc: "Connectez un service pour qu'Apollia puisse l'utiliser (Drive, Slack…).",
      action: "Parcourir",
      tone: "info",
    },
    {
      icon: List,
      title: "Aucune tâche en cours",
      desc: "Les tâches assignées aux agents apparaîtront ici en temps réel.",
      tone: "neutral",
    },
  ];

  const TONE_CLASS: Record<Archetype["tone"], string> = {
    primary: "bg-primary/10 text-primary",
    secondary: "bg-secondary/10 text-secondary-foreground",
    info: "bg-info/10 text-info",
    success: "bg-success/10 text-success-a11y",
    neutral: "bg-muted text-muted-foreground",
  };
</script>

<div class="grid grid-cols-3 gap-3.5">
  {#each ARCHETYPES as e}
    {@const Ic = e.icon}
    <div
      class="bg-card border border-border rounded-xl px-6 py-9 text-center"
    >
      <div
        class="w-[52px] h-[52px] rounded-2xl inline-flex items-center justify-center mx-auto mb-2.5 {TONE_CLASS[
          e.tone
        ]}"
      >
        <Ic size={22} />
      </div>
      <div class="text-sm font-semibold mb-1 text-foreground">{e.title}</div>
      <p class="m-0 text-[11.5px] text-muted-foreground leading-[1.5]">{e.desc}</p>
      {#if e.action}
        <div class="mt-2.5 inline-flex">
          <Button variant="primary-solid" size="sm">{e.action}</Button>
        </div>
      {/if}
      {#if e.ok}
        <div
          class="mt-2.5 text-[11px] text-success-a11y font-medium inline-flex items-center gap-1"
        >
          <Check size={11} /> Tout est traité
        </div>
      {/if}
    </div>
  {/each}
</div>
