<!--
  Floating Action Button for the Tasks route on mobile.

  Shown only on small viewports (Tailwind `md:hidden`) so the desktop layout
  keeps the existing top-right "Nouvelle tâche" button as the primary CTA.
  Operator mode only.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import { Plus } from "lucide-svelte";
  import { uiMode } from "$lib/stores/mode";

  interface Props {
    onclick: () => void;
  }

  let { onclick }: Props = $props();
</script>

{#if $uiMode === "operator"}
  <button
    class="fixed bottom-6 right-6 z-40 flex h-12 w-12 items-center justify-center rounded-full bg-primary text-primary-foreground shadow-lg transition-transform hover:scale-105 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/60 md:hidden"
    onclick={onclick}
    data-testid="tasks-fab"
    aria-label={$t("tasks.fab_new_task_label")}
    title={$t("tasks.create_task_shortcut")}
  >
    <Plus size={22} strokeWidth={2.25} />
  </button>
{/if}
