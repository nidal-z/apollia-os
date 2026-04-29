<script lang="ts">
  import { t } from "svelte-i18n";
  import { navigateTo } from "$lib/stores/navigation";
  import { ShieldCheck, ChevronRight } from "lucide-svelte";

  interface Props {
    count: number;
  }

  let { count }: Props = $props();
</script>

<button
  class="group hover-lift flex w-full items-center gap-3 rounded-lg glass-card glass-border px-4 py-3 hover:bg-warning/5"
  onclick={() => navigateTo("inbox")}
  data-testid="pending-actions"
>
  <div class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-warning/10">
    <ShieldCheck size={14} class="text-warning" />
  </div>
  <div class="flex-1 text-left">
    <span class="text-xs font-medium">{$t('dashboard.approvals_waiting', { values: { count } })}</span>
  </div>
  <ChevronRight size={14} class="text-muted-foreground/40 group-hover:text-foreground transition-colors" />
</button>
