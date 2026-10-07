<script lang="ts">
  /**
   * The advanced settings of the loading window, folded until asked for.
   * An empty field leaves the setting as the model has it.
   */
  import { t } from "svelte-i18n";
  import { Toggle } from "$lib/components/ui/toggle";
  import type { LoadForm } from "$lib/chat/modelLoad";

  interface Props {
    form: LoadForm;
    disabled?: boolean;
  }

  let { form = $bindable(), disabled = false }: Props = $props();

  type NumberField = "batch" | "threads" | "seed";

  const FIELDS: { field: NumberField; label: string; min: number; testid: string }[] = [
    { field: "batch", label: "chat.model_load.advanced.batch", min: 1, testid: "model-load-batch" },
    {
      field: "threads",
      label: "chat.model_load.advanced.threads",
      min: 1,
      testid: "model-load-threads",
    },
    { field: "seed", label: "chat.model_load.advanced.seed", min: 0, testid: "model-load-seed" },
  ];

  function set(field: NumberField, min: number, event: Event): void {
    const raw = (event.currentTarget as HTMLInputElement).value.trim();
    const value = Number(raw);
    form[field] = raw === "" || !Number.isInteger(value) || value < min ? null : value;
  }
</script>

<div class="v4-hair space-y-2 rounded-lg border px-3 py-2.5" data-testid="model-load-advanced">
  <p class="text-body-xs font-medium text-foreground">{$t("chat.model_load.advanced.title")}</p>
  {#each FIELDS as entry (entry.field)}
    <div class="flex items-center justify-between gap-3">
      <label for={entry.testid} class="text-body-xs text-foreground">
        {$t(entry.label)}
      </label>
      <input
        id={entry.testid}
        type="number"
        class="v4-hair h-7 w-28 rounded-md border bg-surface-1 px-2 text-right text-body-xs text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        min={entry.min}
        step="1"
        value={form[entry.field] ?? ""}
        placeholder={$t("chat.model_load.advanced.automatic")}
        onchange={(event) => set(entry.field, entry.min, event)}
        {disabled}
        data-testid={entry.testid}
      />
    </div>
  {/each}
  <div class="flex items-center justify-between gap-3">
    <span class="text-body-xs text-foreground">{$t("chat.model_load.advanced.kv_offload")}</span>
    <Toggle
      checked={form.kvOffload ?? true}
      onchange={(checked) => (form.kvOffload = checked)}
      size="sm"
      {disabled}
      aria-label={$t("chat.model_load.advanced.kv_offload")}
      data-testid="model-load-kv-offload"
    />
  </div>
</div>
