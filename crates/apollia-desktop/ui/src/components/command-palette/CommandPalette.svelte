<script lang="ts">
  /**
   * Top-level command palette orchestrator.
   *
   * Composes:
   *   - `commandPaletteOpen`     - dialog visibility
   *   - `paletteGroups` derived  - pages / actions / sessions / settings / help
   *
   * The actual dialog chrome is delegated to the existing headless
   * `<Command>` primitive so we keep fuzzy search, keyboard navigation
   * and a11y (focus trap, aria-activedescendant) in one place.
   */
  import { onMount } from "svelte";
  import { isLoading, t } from "svelte-i18n";
  import { Command } from "$lib/components/ui/command";
  import { commandPaletteOpen } from "$lib/stores/commandPalette";
  import {
    paletteGroups,
    setPaletteActions,
  } from "$lib/command-palette/paletteIndex";
  import { buildPaletteActions } from "$lib/command-palette/actions";

  // Palette actions reference translated strings, so we can't build them
  // until svelte-i18n has loaded. Rebuild whenever loading flips.
  onMount(() => {
    const stop = isLoading.subscribe((loading) => {
      if (!loading) {
        setPaletteActions(buildPaletteActions());
      }
    });
    return stop;
  });

  const placeholder = $derived($t("commandPalette.placeholder.all"));

  function onExecute(id: string): void {
    // Telemetry hook - observability pipeline picks this up via console.
    console.debug("[command-palette] executed", id);
  }
</script>

<Command
  bind:open={$commandPaletteOpen}
  groups={$paletteGroups}
  {placeholder}
  onexecute={onExecute}
/>
