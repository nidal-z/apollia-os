<script lang="ts">
  import { t } from "svelte-i18n";
  import { Eye, EyeOff, ExternalLink } from "lucide-svelte";
  import { Input } from "$lib/components/ui/input";
  import type { ConnectorEnrichmentView, RegistryEnvVarView } from "$lib/types";

  interface Props {
    envVars: RegistryEnvVarView[];
    enrichment: ConnectorEnrichmentView | null;
    values: Record<string, string>;
    onchange: (key: string, value: string) => void;
  }

  let { envVars, enrichment, values, onchange }: Props = $props();

  let revealed = $state<Record<string, boolean>>({});

  function toggleReveal(name: string): void {
    revealed = { ...revealed, [name]: !revealed[name] };
  }

  function inputType(envVar: RegistryEnvVarView): string {
    if (!envVar.is_secret) return "text";
    return revealed[envVar.name] ? "text" : "password";
  }
</script>

<div class="space-y-4" data-testid="wizard-step-auth">
  {#if envVars.length === 0}
    <p class="text-sm text-muted-foreground" data-testid="auth-no-vars">
      {$t("integrations.wizard.no_auth_required")}
    </p>
  {:else}
    {#if enrichment?.auth_help_url}
      <div class="rounded-md border border-border bg-muted/40 px-4 py-3">
        <p class="text-sm text-muted-foreground">
          {enrichment.auth_help_text ?? $t("integrations.wizard.auth_help_default")}
        </p>
        <a
          href={enrichment.auth_help_url}
          target="_blank"
          rel="noopener noreferrer"
          class="mt-1.5 inline-flex items-center gap-1 text-xs text-primary underline-offset-4 hover:underline"
          data-testid="auth-help-link"
        >
          <ExternalLink size={12} />
          {$t("integrations.wizard.auth_help_link")}
        </a>
      </div>
    {/if}

    <div class="space-y-3">
      {#each envVars as envVar (envVar.name)}
        <div class="space-y-1.5">
          <label class="block text-sm font-medium text-foreground" for={`env-${envVar.name}`}>
            {envVar.name}
            {#if envVar.is_required}
              <span class="ml-1 text-xs text-destructive">*</span>
            {:else}
              <span class="ml-1 text-xs text-muted-foreground">({$t("integrations.wizard.optional")})</span>
            {/if}
          </label>

          {#if envVar.description}
            <p class="text-xs text-muted-foreground">{envVar.description}</p>
          {/if}

          <div class="relative">
            <Input
              id={`env-${envVar.name}`}
              type={inputType(envVar)}
              value={values[envVar.name] ?? ""}
              oninput={(e) => onchange(envVar.name, (e.currentTarget as HTMLInputElement).value)}
              placeholder={envVar.is_secret ? "••••••••" : envVar.name}
              autocomplete="off"
              class={envVar.is_secret ? "pr-10" : ""}
              data-testid={`env-input-${envVar.name}`}
            />
            {#if envVar.is_secret}
              <button
                type="button"
                class="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground transition-colors hover:text-foreground"
                onclick={() => toggleReveal(envVar.name)}
                aria-label={revealed[envVar.name]
                  ? $t("integrations.wizard.hide_value")
                  : $t("integrations.wizard.show_value")}
                data-testid={`env-toggle-${envVar.name}`}
              >
                {#if revealed[envVar.name]}
                  <EyeOff size={14} />
                {:else}
                  <Eye size={14} />
                {/if}
              </button>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
