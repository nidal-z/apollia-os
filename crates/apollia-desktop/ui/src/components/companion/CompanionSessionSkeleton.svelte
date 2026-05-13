<script lang="ts">
  import { t } from "svelte-i18n";
  import { Bot } from "lucide-svelte";
  import type { CompanionSessionStatus } from "$lib/stores/companion";
  import { Spinner } from "$lib/components/ui/progress";
  import { Skeleton } from "$lib/components/ui/skeleton";

  interface Props {
    status: CompanionSessionStatus;
  }

  let { status }: Props = $props();

  const label = $derived.by(() => {
    switch (status) {
      case "connecting":
        return $t("companion.session.connecting");
      case "creating":
        return $t("companion.session.creating");
      case "ready":
        return $t("companion.session.ready");
      default:
        return $t("companion.session.connecting");
    }
  });
</script>

<div
  class="flex h-full flex-col"
  data-testid="companion-session-skeleton"
  aria-busy="true"
  aria-live="polite"
>
  <!-- Skeleton header -->
  <div class="flex items-center gap-2 border-b border-border px-3 py-2">
    <Skeleton class="h-6 w-6 rounded-full" />
    <Skeleton class="h-3 w-28" />
  </div>

  <!-- Skeleton messages -->
  <div class="flex-1 space-y-4 overflow-hidden p-4">
    <div class="flex gap-2">
      <Bot size={18} class="mt-0.5 shrink-0 text-muted-foreground/60" />
      <div class="flex-1 space-y-2">
        <Skeleton class="h-3 w-3/4" />
        <Skeleton class="h-3 w-1/2" />
      </div>
    </div>
    <div class="flex justify-end">
      <Skeleton class="h-8 w-2/3" />
    </div>
    <div class="flex gap-2">
      <Bot size={18} class="mt-0.5 shrink-0 text-muted-foreground/60" />
      <div class="flex-1 space-y-2">
        <Skeleton class="h-3 w-2/3" />
      </div>
    </div>
  </div>

  <!-- Spinner + status text -->
  <div
    class="flex items-center gap-2 border-t border-border px-3 py-2 text-xs text-muted-foreground"
  >
    <Spinner size={14} />
    <span data-testid="companion-session-status">{label}</span>
  </div>

  <!-- Disabled input hint -->
  <div class="border-t border-border p-2">
    <div
      class="flex items-center rounded-md border border-border bg-muted/40 px-3 py-2 text-xs text-muted-foreground"
      aria-disabled="true"
    >
      {$t("companion.session.input_disabled_hint")}
    </div>
  </div>
</div>
