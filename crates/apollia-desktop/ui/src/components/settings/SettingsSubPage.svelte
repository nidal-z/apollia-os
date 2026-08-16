<script lang="ts">
  /**
   * SettingsSubPage - the canonical frame every settings sub-page renders into.
   *
   * It sits inside the settings shell content pane (the shell owns the top-level
   * PageHeader and the unsaved affordance). A sub-page wraps its section cards in
   * this frame to get, for free:
   *
   *   1. an optional in-body PageHeader (kicker / title / subtitle) for standalone
   *      or embedded use - omit these props when the shell already renders the
   *      header, which is the default in the settings hub;
   *   2. a `children` snippet holding the stacked <SettingsSection> cards;
   *   3. an optional save / reset footer wired to the settings dirty-state store
   *      and registered with the shell via `__apolliaRegisterSettingsForm`, so the
   *      footer buttons, the Cmd+S shortcut, and the unsaved-changes nav guard all
   *      drive the same `onSave` / `onReset` handlers.
   *
   * Explicit-save contract (imported by every sub-page that persists on demand):
   *
   *   <SettingsSubPage route="profile" onSave={save} onReset={reset}>
   *     <SettingsSection ...>...</SettingsSection>
   *   </SettingsSubPage>
   *
   *   // save: () => Promise<boolean>  - persist; resolve true on success and set
   *   //                                 the route saving state; never toasts.
   *   // reset: () => void            - revert local edits to last saved values.
   *
   * Auto-save (debounced) pages omit `onSave` / `onReset`: the footer is hidden
   * and the sub-page manages persistence itself.
   *
   * Props:
   *   route        required. The SettingsSubRoute this page owns.
   *   onSave       optional. Presence makes this an explicit-save page (footer +
   *                Cmd+S + nav guard). Persists and resolves true on success.
   *   onReset      optional. Reverts local edits.
   *   kicker       optional. In-body header kicker (standalone use only).
   *   title        optional. In-body header title; when set, an in-body
   *                PageHeader is rendered above the body.
   *   subtitle     optional. In-body header subtitle.
   *   children     required. The section cards.
   *   data-testid  forwarded to the frame wrapper.
   */
  import { onMount } from "svelte";
  import type { Snippet } from "svelte";
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import PageHeader from "$lib/components/operator/PageHeader.svelte";
  import { addToast } from "$lib/components/ui/toast";
  import {
    settingsDirtyStore,
    setRouteState,
    getRouteState,
  } from "$lib/stores/settingsDirty";
  import type { SettingsSubRoute } from "$lib/stores/settings";

  type SaveFn = () => Promise<boolean>;

  interface Props {
    route: SettingsSubRoute;
    onSave?: SaveFn;
    onReset?: () => void;
    kicker?: string;
    title?: string;
    subtitle?: string;
    children: Snippet;
    "data-testid"?: string;
  }

  let {
    route,
    onSave,
    onReset,
    kicker,
    title,
    subtitle,
    children,
    "data-testid": testid,
  }: Props = $props();

  const explicit = $derived(onSave !== undefined);

  const state = $derived($settingsDirtyStore[route]);
  const dirty = $derived(state?.dirty === true);
  const saving = $derived(state?.savingState === "saving");

  // Mark the route as explicit-save so the nav guard prompts before leaving.
  $effect(() => {
    if (explicit) setRouteState(route, { autoSave: "explicit" });
  });

  // Register the save/reset handlers with the shell (Cmd+S + nav guard).
  onMount(() => {
    if (!onSave) return;
    const register = (
      globalThis as unknown as {
        __apolliaRegisterSettingsForm?: (
          r: SettingsSubRoute,
          h: { save: SaveFn; reset: () => void },
        ) => () => void;
      }
    ).__apolliaRegisterSettingsForm;
    return register?.(route, { save: onSave, reset: onReset ?? (() => {}) });
  });

  async function runSave(): Promise<void> {
    if (!onSave || !dirty || saving) return;
    const ok = await onSave();
    if (ok) {
      addToast($t("settings.save_toast_success"), "success", {
        "data-testid": "settings-save-toast-success",
      });
    } else {
      const err = getRouteState(route).lastError ?? "";
      addToast($t("settings.save_toast_error", { values: { error: err } }), "error", {
        "data-testid": "settings-save-toast-error",
      });
    }
  }
</script>

<div class="flex flex-col gap-4" data-testid={testid}>
  {#if title}
    <PageHeader {kicker} {title} {subtitle} />
  {/if}

  <div class="flex flex-col gap-4">
    {@render children()}
  </div>

  {#if explicit}
    <footer
      class="flex flex-wrap items-center gap-3 border-t border-border/60 pt-4"
      data-testid="settings-subpage-footer"
    >
      <span class="inline-flex items-center gap-2 text-caption text-muted-foreground">
        <kbd
          class="rounded-md border border-border bg-muted px-1.5 py-0.5 font-mono text-caption text-muted-foreground"
        >
          {$t("settings.save_shortcut_hint")}
        </kbd>
        {$t("settings.save_hint")}
      </span>
      <div class="ml-auto flex items-center gap-2">
        <Button
          variant="destructive"
          size="sm"
          disabled={!dirty || saving}
          onclick={() => onReset?.()}
          data-testid="settings-subpage-reset"
        >
          {$t("settings.reset")}
        </Button>
        <Button
          variant="default"
          size="sm"
          loading={saving}
          disabled={!dirty || saving}
          onclick={runSave}
          data-testid="settings-subpage-save"
        >
          {$t("settings.save")}
        </Button>
      </div>
    </footer>
  {/if}
</div>
