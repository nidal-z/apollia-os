<script lang="ts">
  /**
   * Top-level command palette orchestrator.
   *
   * Composes:
   *   - `paletteMode`            — all vs. actions-only
   *   - `commandPaletteOpen`     — dialog visibility
   *   - `paletteGroups` derived  — pages / actions / sessions / settings / help
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
    paletteMode,
    setPaletteActions,
  } from "$lib/command-palette/paletteIndex";
  import { buildPaletteActions } from "$lib/command-palette/actions";

  // Reset to "all" whenever the palette closes so the next Cmd+K opens
  // with the default (pages + actions) view.
  $effect(() => {
    if (!$commandPaletteOpen) {
      paletteMode.set("all");
    }
  });

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

  const placeholder = $derived(
    $paletteMode === "actions"
      ? $t("commandPalette.placeholder.actions")
      : $t("commandPalette.placeholder.all"),
  );

  function onExecute(id: string): void {
    // Telemetry hook — observability pipeline picks this up via console.
    // eslint-disable-next-line no-console
    console.debug("[command-palette] executed", id);
  }
</script>

<Command
  bind:open={$commandPaletteOpen}
  groups={$paletteGroups}
  {placeholder}
  onexecute={onExecute}
/>
