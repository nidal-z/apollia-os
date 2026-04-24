<script lang="ts">
  import { t } from "svelte-i18n";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";

  interface Props {
    value: string;
    onchange: (val: string) => void;
  }

  let { value, onchange }: Props = $props();

  const PRESETS = ["15m", "30m", "1h", "6h", "12h", "24h"];

  type Unit = "m" | "h" | "d";

  let selectedPreset = $state<string | null>(null);
  let showCustom = $state(false);
  let customNumber = $state("1");
  let customUnit = $state<Unit>("h");
  let initialized = $state(false);

  function parseValue(val: string): { preset: string | null; n: string; unit: Unit } {
    if (!val) return { preset: null, n: "1", unit: "h" };
    if (PRESETS.includes(val)) return { preset: val, n: "1", unit: "h" };
    const m = val.match(/^(\d+)(s|m|h|d)$/);
    if (m) {
      const unit = m[2] === "s" ? "m" : (m[2] as Unit);
      return { preset: null, n: m[1], unit };
    }
    return { preset: null, n: "1", unit: "h" };
  }

  $effect(() => {
    if (!initialized && value) {
      const parsed = parseValue(value);
      if (parsed.preset) {
        selectedPreset = parsed.preset;
        showCustom = false;
      } else {
        selectedPreset = null;
        showCustom = true;
        customNumber = parsed.n;
        customUnit = parsed.unit;
      }
      initialized = true;
    }
  });

  function selectPreset(p: string) {
    if (selectedPreset === p) {
      // Toggle off → show custom
      selectedPreset = null;
      showCustom = true;
      onchange(`${customNumber}${customUnit}`);
    } else {
      selectedPreset = p;
      showCustom = false;
      onchange(p);
    }
  }

  function onCustomChange() {
    onchange(`${customNumber}${customUnit}`);
  }

  const UNIT_OPTIONS: { value: Unit; labelKey: string }[] = [
    { value: "m", labelKey: "triggers.interval_unit_minutes" },
    { value: "h", labelKey: "triggers.interval_unit_hours" },
    { value: "d", labelKey: "triggers.interval_unit_days" },
  ];
</script>

<div class="space-y-3">
  <!-- Preset chips -->
  <div class="flex flex-wrap gap-1.5">
    {#each PRESETS as p}
      <button
        type="button"
        class="rounded-full border px-3 py-1 text-xs font-medium transition-colors
          {selectedPreset === p && !showCustom
            ? 'border-primary bg-primary/10 text-primary'
            : 'border-border text-muted-foreground hover:border-border/60 hover:text-foreground'}"
        onclick={() => selectPreset(p)}
      >
        {p}
      </button>
    {/each}
    <button
      type="button"
      class="rounded-full border px-3 py-1 text-xs font-medium transition-colors
        {showCustom
          ? 'border-primary bg-primary/10 text-primary'
          : 'border-border text-muted-foreground hover:border-border/60 hover:text-foreground'}"
      onclick={() => { selectedPreset = null; showCustom = true; }}
    >
      {$t("triggers.interval_custom")}
    </button>
  </div>

  <!-- Custom input: number + unit -->
  {#if showCustom}
    <div class="flex items-center gap-2">
      <Input
        type="number"
        min={1}
        class="w-20 text-sm"
        bind:value={customNumber}
        oninput={onCustomChange}
      />
      <Select
        bind:value={customUnit}
        class="w-32 text-sm"
        onchange={onCustomChange}
      >
        {#each UNIT_OPTIONS as opt}
          <option value={opt.value}>{$t(opt.labelKey)}</option>
        {/each}
      </Select>
    </div>
  {/if}
</div>
