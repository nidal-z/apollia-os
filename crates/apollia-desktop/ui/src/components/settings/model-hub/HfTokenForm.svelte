<script lang="ts">
  /**
   * HfTokenForm - the optional HuggingFace read-token entry.
   *
   * Shown when a gated model is downloaded without a token. The token lives in
   * memory only (never persisted here) and is never logged; the field is a
   * password input.
   */
  import { t } from "svelte-i18n";
  import { Key, ExternalLink } from "lucide-svelte";
  import { Input } from "$lib/components/ui/input";
  import { Button } from "$lib/components/ui/button";
  import SettingsSection from "../SettingsSection.svelte";

  interface Props {
    token: string;
    onDone: () => void;
  }

  let { token = $bindable(), onDone }: Props = $props();
</script>

<SettingsSection
  title={$t("settings.model_hub.token.label")}
  data-testid="hf-token-form"
>
  {#snippet icon()}<Key size={15} strokeWidth={1.75} class="text-warning-a11y" />{/snippet}

  <p class="text-caption text-muted-foreground">
    {$t("settings.model_hub.token.description")}
    <a
      href="https://huggingface.co/settings/tokens"
      target="_blank"
      rel="noopener noreferrer"
      class="inline-flex items-center gap-1 text-primary hover:underline"
    >
      huggingface.co/settings/tokens <ExternalLink size={12} />
    </a>
  </p>
  <div class="flex items-center gap-2">
    <Input
      type="password"
      size="sm"
      bind:value={token}
      placeholder="hf_••••••••••••••••"
      class="flex-1"
      data-testid="hf-token-input"
    />
    <Button variant="default" size="sm" onclick={onDone} data-testid="hf-token-save">
      {$t("settings.model_hub.token.done")}
    </Button>
  </div>
</SettingsSection>
