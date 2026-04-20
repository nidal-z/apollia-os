<script lang="ts">
  import { onDestroy } from "svelte";
  import { formatCombo, detectPlatform } from "$lib/keyboard/hotkeyCapture";
  import KeyChip from "./KeyChip.svelte";

  interface Props {
    hotkey: string;
    placeholder?: string;
    debounceMs?: number;
    "data-testid"?: string;
  }

  let { hotkey, placeholder = "", debounceMs = 100, ...rest }: Props = $props();

  // Debounce display updates so that rapid parent changes do not re-render the
  // surrounding section unnecessarily (the surrounding `<SettingsSection>`
  // cannot bust its own re-render; but this component caches the displayed
  // combo and updates at most once per `debounceMs`).
  let displayedCombo = $state(hotkey);
  let timer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    const next = hotkey;
    if (next === displayedCombo) return;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      displayedCombo = next;
      timer = null;
    }, debounceMs);
  });

  onDestroy(() => {
    if (timer) clearTimeout(timer);
  });

  const platform = detectPlatform();

  const chips = $derived.by(() => {
    if (!displayedCombo) return [] as string[];
    const tokens = displayedCombo.split("+");
    return tokens.map((tok) => formatCombo(tok, platform));
  });
</script>

<span class="inline-flex items-center gap-1 font-mono text-sm" {...rest}>
  {#if chips.length === 0}
    <span class="text-muted-foreground">{placeholder}</span>
  {:else}
    {#each chips as chip, i (i + chip)}
      <KeyChip label={chip} variant={i < chips.length - 1 ? "modifier" : "key"} />
    {/each}
  {/if}
</span>
