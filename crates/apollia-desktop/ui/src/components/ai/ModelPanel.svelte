<script lang="ts">
  /**
   * ModelPanel - the panel of an installed model, in the detail sidebar of
   * the AI screen: its name and its two actions, then its tabs. Infos is
   * there for everyone; Loading and Inference come with the developer mode.
   * Everything is automatic by default and the sections of the last two are
   * folded: nobody has to open them.
   *
   * The settings of the last two tabs are those of a preset of the model.
   * In developer mode, two more sections correct the type of the model and
   * the weights file it loads.
   */
  import { untrack } from "svelte";
  import { get } from "svelte/store";
  import { locale, t } from "svelte-i18n";
  import { MessageSquarePlus, Play, Square } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { TabBar } from "$lib/components/ui/tabs";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import { listModelPresets } from "$lib/ipc/agentAdjust";
  import { createChatSession } from "$lib/ipc/agents";
  import { unloadLoadedModel } from "$lib/ipc/llm";
  import type { ModelCategory } from "$lib/ipc/modelHub";
  import {
    createModelPreset,
    getInstalledModelDetail,
    setInstalledModelCategoryOverride,
    setInstalledModelSourceOverride,
    updateModelPreset,
    type ModelDetail,
    type PresetDetail,
    type PresetWrite,
  } from "$lib/ipc/modelDetail";
  import { startRequest } from "$lib/chat/applyTargetChoice";
  import { aiModelSelection, notifyAiModelsChanged } from "$lib/stores/aiModelPanel";
  import { pendingChatSessionId } from "$lib/stores/chat";
  import { developerMode } from "$lib/stores/developerMode";
  import { DEVELOPER_SETTING_KEYS, developerSettings } from "$lib/stores/developerSettings";
  import { askModelLoad } from "$lib/stores/modelResidency";
  import { navigateTo } from "$lib/stores/navigation";
  import ModelDevSections from "./ModelDevSections.svelte";
  import ModelInferenceTab from "./ModelInferenceTab.svelte";
  import ModelInfoTab from "./ModelInfoTab.svelte";
  import ModelLoadTab from "./ModelLoadTab.svelte";
  import ModelPresetBar from "./ModelPresetBar.svelte";
  import {
    chatTargetOf,
    defaultPresetWrite,
    formChanged,
    formFromPreset,
    formSettings,
    headActions,
    initialPreset,
    isLoadSettingsRefusal,
    newPresetWrite,
    panelTabs,
    presetUpdateWrite,
    presetsOf,
    relaunchWarning,
    shownPanelTab,
    tabLabelKey,
    type PanelForm,
    type PanelTab,
  } from "./modelDetail";

  const selection = $derived($aiModelSelection);
  const model = $derived(selection?.model ?? null);
  const modelId = $derived(model?.id ?? null);
  const lang = $derived($locale ?? "fr");
  const loadSettingsKept = $derived(
    $developerSettings[DEVELOPER_SETTING_KEYS.presetLoadSettings].value,
  );

  let asked = $state<PanelTab>("info");
  // Turning the developer mode off while Loading or Inference is open brings
  // the panel back to Infos.
  const tab = $derived(shownPanelTab(asked, $developerMode));
  let detail = $state<ModelDetail | null>(null);
  let presets = $state<PresetDetail[]>([]);
  let presetId = $state<number | null>(null);
  let form = $state<PanelForm>(formFromPreset(null));
  let baseline = $state<PanelForm>(formFromPreset(null));
  let errors = $state<Record<string, string>>({});
  let working = $state(false);
  let run = 0;

  const chosen = $derived(presets.find((p) => p.id === presetId) ?? null);
  const changed = $derived(formChanged(form, baseline));
  const head = $derived(model ? headActions(model, selection?.loaded ?? false) : null);
  const tabs = $derived(
    panelTabs($developerMode).map((key) => ({ key, label: $t(tabLabelKey(key)) })),
  );

  function show(preset: PresetDetail | null): void {
    presetId = preset?.id ?? null;
    form = formFromPreset(preset);
    baseline = formFromPreset(preset);
    errors = {};
  }

  async function readDetail(id: string, mine: number): Promise<void> {
    try {
      const answer = await getInstalledModelDetail(id, lang);
      if (mine === run) detail = answer;
    } catch (err: unknown) {
      if (mine === run) reportError(err, { surface: "toast" });
    }
  }

  async function readPresets(id: string, mine: number, keep: number | null): Promise<void> {
    try {
      const all = presetsOf(await listModelPresets(), id);
      if (mine !== run) return;
      presets = all;
      show(all.find((p) => p.id === keep) ?? initialPreset(all));
    } catch {
      // Without a preset store the panel shows the automatic settings.
      if (mine === run) presets = [];
    }
  }

  // Another model: the panel opens on Infos, with the default preset of that
  // model, or the automatic settings when it has none.
  $effect(() => {
    const id = modelId;
    untrack(() => {
      const mine = ++run;
      asked = "info";
      detail = null;
      presets = [];
      show(null);
      if (id === null) return;
      void readDetail(id, mine);
      void readPresets(id, mine, null);
    });
  });

  async function act(action: () => Promise<void>): Promise<void> {
    if (working) return;
    working = true;
    try {
      await action();
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      if (isLoadSettingsRefusal(message)) {
        addToast(get(t)("ai.panel.preset.load_settings_refused"), "error");
      } else {
        reportError(err, { surface: "toast" });
      }
    } finally {
      working = false;
    }
  }

  /** The settings on screen, or nothing when a field cannot be read. */
  function settings(): Extract<ReturnType<typeof formSettings>, { ok: true }> | null {
    const read = formSettings(form);
    if (read.ok) {
      errors = {};
      return read;
    }
    errors = read.errors;
    addToast(get(t)("ai.panel.preset.invalid"), "error");
    return null;
  }

  async function write(id: string, save: () => Promise<PresetDetail>, okKey: string): Promise<void> {
    const saved = await save();
    addToast(get(t)(okKey, { values: { name: saved.name } }), "success");
    await readPresets(id, run, saved.id);
  }

  function saveAs(name: string): void {
    const id = modelId;
    const read = settings();
    if (id === null || read === null) return;
    const preset: PresetWrite = newPresetWrite(name, read, loadSettingsKept);
    void act(() => write(id, () => createModelPreset(id, preset), "ai.panel.preset.saved"));
  }

  function save(): void {
    const id = modelId;
    const preset = chosen;
    const read = settings();
    if (id === null || preset === null || read === null) return;
    const body = presetUpdateWrite(preset, read, loadSettingsKept);
    void act(() => write(id, () => updateModelPreset(preset.id, body), "ai.panel.preset.saved"));
  }

  function setDefault(): void {
    const id = modelId;
    const preset = chosen;
    if (id === null || preset === null) return;
    void act(() =>
      write(
        id,
        () => updateModelPreset(preset.id, defaultPresetWrite(preset)),
        "ai.panel.preset.default_set",
      ),
    );
  }

  function choose(id: number | null): void {
    show(presets.find((p) => p.id === id) ?? null);
  }

  function useInChat(): void {
    if (!model) return;
    const request = startRequest(chatTargetOf(model));
    void act(async () => {
      const session = await createChatSession(request);
      pendingChatSessionId.set(session.id);
      navigateTo("chat");
    });
  }

  function loadOrUnload(): void {
    if (!model || !head) return;
    if (head.load.kind === "load") {
      askModelLoad({ modelId: model.id, format: model.format, onloaded: notifyAiModelsChanged });
      return;
    }
    const { id, path } = model;
    void act(async () => {
      await unloadLoadedModel(path);
      addToast(get(t)("ai.loaded.unloaded_toast", { values: { name: id } }), "success");
      notifyAiModelsChanged();
    });
  }

  function overrideType(category: ModelCategory | null): void {
    const id = modelId;
    if (id === null) return;
    void act(async () => {
      await setInstalledModelCategoryOverride(id, category);
      notifyAiModelsChanged();
      await readDetail(id, run);
    });
  }

  function overrideSource(path: string | null): void {
    const id = modelId;
    if (id === null) return;
    void act(async () => {
      await setInstalledModelSourceOverride(id, path);
      notifyAiModelsChanged();
      await readDetail(id, run);
    });
  }

  async function copy(text: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      addToast(get(t)("ai.toast.copied_id"), "success");
    } catch {
      addToast(get(t)("ai.toast.copy_failed"), "error");
    }
  }
</script>

{#if model && head}
  <!-- The scrolling box is not the column itself: a flex column of a fixed
       height would squeeze its folding sections into one another. -->
  <div class="h-full overflow-y-auto" data-testid="ai-model-panel" data-model-id={model.id}>
    <div class="flex flex-col gap-4 p-4">
      <header class="flex flex-col gap-2">
        <h3 class="m-0 break-words text-body font-medium text-foreground" data-testid="ai-panel-name">
          {model.id}
        </h3>
        <Button
          variant="primary-solid"
          size="sm"
          class="w-full"
          onclick={useInChat}
          disabled={working || !head.chat.enabled}
          data-testid="ai-panel-use-in-chat"
        >
          <MessageSquarePlus size={14} aria-hidden="true" />
          {$t("ai.panel.head.use_in_chat")}
        </Button>
        <Button
          variant="outline"
          size="sm"
          class="w-full"
          onclick={loadOrUnload}
          disabled={working || !head.load.enabled}
          data-testid="ai-panel-load-btn"
          data-kind={head.load.kind}
        >
          {#if head.load.kind === "unload"}
            <Square size={14} aria-hidden="true" />
            {$t("ai.loaded.unload")}
          {:else}
            <Play size={14} aria-hidden="true" />
            {$t("ai.panel.head.load")}
          {/if}
        </Button>
        {#if head.chat.reasonKey}
          <p class="m-0 text-caption text-muted-foreground" data-testid="ai-panel-head-reason">
            {$t(head.chat.reasonKey)}
          </p>
        {/if}
      </header>

      <TabBar
        items={tabs}
        activeTab={tab}
        ontabchange={(key) => (asked = key as PanelTab)}
        testidPrefix="ai-panel"
        variant="underline"
      />

      {#if tab === "info"}
        <ModelInfoTab {model} {detail} oncopy={(text) => void copy(text)} />
      {:else}
        <ModelPresetBar
          {presets}
          {chosen}
          {changed}
          {working}
          onchoose={choose}
          onsaveas={saveAs}
          onsave={save}
          onsetdefault={setDefault}
        />
        {#if tab === "load"}
          <ModelLoadTab
            format={model.format}
            {detail}
            bind:form
            {errors}
            writable={loadSettingsKept}
            relaunch={relaunchWarning(selection?.loaded ?? false, form, baseline)}
          />
        {:else}
          <ModelInferenceTab bind:form {errors} authorFields={detail?.author_fields ?? []} />
        {/if}
      {/if}

      <ModelDevSections {detail} {working} ontype={overrideType} onsource={overrideSource} />
    </div>
  </div>
{/if}
