<script lang="ts">
  /**
   * ModelsBlock - the "Models" block of the General settings.
   *
   * The models folder in force with "Change", which opens a folder picker and
   * writes `models.dir` to `apollia.toml` (no model is moved), then the Hugging
   * Face token: a masked field with Show, Save and Clear, kept in the secret
   * store of this machine and read by the downloader for a gated repository.
   * Each field saves through its own button. Logic lives in `modelsBlock.ts`.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { Boxes, Lock, ExternalLink } from "lucide-svelte";
  import { open as openFolderPicker } from "@tauri-apps/plugin-dialog";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import { openExternalUrl } from "$lib/utils/externalLink";
  import { getResolvedSetting, setSetting, type SettingSource } from "$lib/ipc/settings";
  import { clearHfToken, getHfToken, getModelsDir, setHfToken } from "$lib/ipc/modelsFolder";
  import SettingsSection from "../SettingsSection.svelte";
  import SettingsFieldRow from "../SettingsFieldRow.svelte";
  import {
    HF_TOKENS_URL,
    MODELS_DIR_KEY,
    changeModelsDir,
    folderLocked,
    pickedFolder,
    tokenFieldState,
  } from "./modelsBlock";

  let folder = $state<string | null>(null);
  let folderSource = $state<SettingSource>("default");
  let changing = $state(false);

  let typedToken = $state("");
  let storedToken = $state("");
  let revealed = $state(false);
  let tokenBusy = $state(false);

  const locked = $derived(folderLocked(folderSource));
  const tokenState = $derived(tokenFieldState(typedToken, storedToken));

  async function loadFolder(): Promise<void> {
    try {
      folder = await getModelsDir();
      folderSource = (await getResolvedSetting(MODELS_DIR_KEY)).source;
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }

  async function loadToken(): Promise<void> {
    try {
      storedToken = (await getHfToken()) ?? "";
      typedToken = storedToken;
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }

  async function chooseFolder(): Promise<void> {
    if (locked || changing) return;
    changing = true;
    try {
      const chosen = pickedFolder(
        await openFolderPicker({ directory: true, defaultPath: folder ?? undefined }),
      );
      if (chosen === null) return;
      folder = await changeModelsDir(chosen, { writeSetting: setSetting, readFolder: getModelsDir });
      folderSource = "user";
      addToast($t("settings.general.models_dir_changed_toast"), "success");
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      changing = false;
    }
  }

  async function saveToken(): Promise<void> {
    if (!tokenState.canSave || tokenBusy) return;
    tokenBusy = true;
    try {
      const token = typedToken.trim();
      await setHfToken(token);
      storedToken = token;
      typedToken = token;
      addToast($t("settings.general.hf_token_saved_toast"), "success");
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      tokenBusy = false;
    }
  }

  async function clearToken(): Promise<void> {
    if (!tokenState.canClear || tokenBusy) return;
    tokenBusy = true;
    try {
      await clearHfToken();
      storedToken = "";
      typedToken = "";
      revealed = false;
      addToast($t("settings.general.hf_token_cleared_toast"), "success");
    } catch (err) {
      reportError(err, { surface: "toast" });
    } finally {
      tokenBusy = false;
    }
  }

  async function openTokensPage(): Promise<void> {
    try {
      await openExternalUrl(HF_TOKENS_URL);
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }

  onMount(() => {
    void loadFolder();
    void loadToken();
  });
</script>

<SettingsSection
  title={$t("settings.general.models_title")}
  bodyLayout="flush"
  data-testid="general-models"
>
  {#snippet icon()}<Boxes size={15} strokeWidth={1.75} />{/snippet}
  <SettingsFieldRow
    label={$t("settings.general.models_dir_label")}
    hint={$t("settings.general.models_dir_hint")}
    data-testid="general-models-dir"
  >
    {#snippet control()}
      <div class="flex flex-col items-end gap-1">
        <div class="flex min-w-0 items-center gap-2">
          <code
            class="min-w-0 max-w-64 truncate rounded-sm bg-muted px-2 py-1 font-mono text-caption text-muted-foreground"
            title={folder ?? ""}
            data-testid="general-models-dir-path"
          >
            {folder ?? $t("settings.general.models_dir_unknown")}
          </code>
          <Button
            variant="outline"
            size="sm"
            disabled={locked || changing}
            onclick={chooseFolder}
            data-testid="general-models-dir-change"
          >
            {$t("settings.general.models_dir_change")}
          </Button>
        </div>
        {#if locked}
          <p
            class="inline-flex items-center gap-1.5 text-xs text-muted-foreground"
            title={$t("settings.policy_locked_hint")}
            data-testid="general-models-dir-policy"
          >
            <Lock class="h-3 w-3" aria-hidden="true" />
            {$t("settings.policy_locked")}
          </p>
        {/if}
      </div>
    {/snippet}
  </SettingsFieldRow>
  <SettingsFieldRow
    label={$t("settings.general.hf_token_label")}
    hint={$t("settings.general.hf_token_hint")}
    for="general-hf-token"
    layout="stack"
    border={false}
    data-testid="general-hf-token"
  >
    {#snippet control()}
      <div class="flex flex-col gap-2">
        <button
          type="button"
          class="inline-flex w-fit items-center gap-1 text-caption text-primary hover:underline"
          onclick={openTokensPage}
          data-testid="general-hf-token-link"
        >
          {$t("settings.general.hf_token_link")}
          <ExternalLink size={12} aria-hidden="true" />
        </button>
        <div class="flex flex-wrap items-center gap-2">
          <!-- i18n-ignore: Hugging Face token prefix, not copy -->
          <Input
            id="general-hf-token"
            type={revealed ? "text" : "password"}
            size="sm"
            bind:value={typedToken}
            placeholder="hf_••••••••••••••••"
            autocomplete="off"
            spellcheck={false}
            class="w-64 font-mono"
            data-testid="general-hf-token-input"
          />
          <Button
            variant="ghost"
            size="sm"
            onclick={() => (revealed = !revealed)}
            aria-pressed={revealed}
            data-testid="general-hf-token-reveal"
          >
            {revealed ? $t("settings.general.hf_token_hide") : $t("settings.general.hf_token_show")}
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={!tokenState.canSave || tokenBusy}
            onclick={saveToken}
            data-testid="general-hf-token-save"
          >
            {$t("settings.general.hf_token_save")}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={!tokenState.canClear || tokenBusy}
            onclick={clearToken}
            data-testid="general-hf-token-clear"
          >
            {$t("settings.general.hf_token_clear")}
          </Button>
        </div>
        {#if tokenState.dirty}
          <p
            class="inline-flex items-center gap-1.5 text-caption text-warning-a11y"
            data-testid="general-hf-token-unsaved"
          >
            <span class="h-1.5 w-1.5 rounded-full bg-warning" aria-hidden="true"></span>
            {$t("settings.general.hf_token_unsaved")}
          </p>
        {/if}
      </div>
    {/snippet}
  </SettingsFieldRow>
</SettingsSection>
