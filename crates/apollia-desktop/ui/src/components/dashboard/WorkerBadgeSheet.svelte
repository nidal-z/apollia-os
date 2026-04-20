<script lang="ts">
  import { t } from "svelte-i18n";
  import { Sheet } from "$lib/components/ui/sheet";
  import { Zap } from "lucide-svelte";

  interface Props {
    open: boolean;
    onclose: () => void;
  }

  let { open, onclose }: Props = $props();
</script>

<Sheet {open} {onclose} width="md">
  <div class="flex h-full flex-col gap-4 p-6" data-testid="worker-badge-sheet">
    <header class="flex items-center gap-2">
      <div class="flex h-8 w-8 items-center justify-center rounded-full bg-secondary/10 text-secondary">
        <Zap size={16} />
      </div>
      <h2 class="text-lg font-semibold">{$t("dashboard.worker_sheet_title")}</h2>
    </header>

    <p class="text-sm text-muted-foreground">{$t("dashboard.worker_sheet_intro")}</p>

    <div class="rounded-md border border-border/60 bg-muted/30 p-4">
      <h3 class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
        {$t("dashboard.worker_sheet_what_title")}
      </h3>
      <p class="mt-2 text-sm">{$t("dashboard.worker_sheet_what_body")}</p>
    </div>

    <div class="rounded-md border border-border/60 p-4">
      <h3 class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
        {$t("dashboard.worker_sheet_when_title")}
      </h3>
      <p class="mt-2 text-sm">{$t("dashboard.worker_sheet_when_body")}</p>
    </div>
  </div>
</Sheet>
