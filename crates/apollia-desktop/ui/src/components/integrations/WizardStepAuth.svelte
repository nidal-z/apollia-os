<script lang="ts">
  import { t } from "svelte-i18n";
  import { Eye, EyeOff, ExternalLink, Lock } from "lucide-svelte";
  import { Input } from "$lib/components/ui/input";
  import { Textarea } from "$lib/components/ui/textarea";
  import { FormField } from "$lib/components/ui/form-field";
  import type {
    ConnectorEnrichmentView,
    RegistryEnvVarView,
    RegistryPackageArgView,
  } from "$lib/types";

  interface PackageArgEntry {
    index: number;
    arg: RegistryPackageArgView;
  }

  interface Props {
    envVars: RegistryEnvVarView[];
    enrichment: ConnectorEnrichmentView | null;
    values: Record<string, string>;
    onchange: (key: string, value: string) => void;
    /** Positional/named args the user must supply (registry value is null). */
    packageArgs?: PackageArgEntry[];
    /** Values typed by the user, keyed by the entry's `index`. */
    argValues?: Record<number, string>;
    onArgChange?: (index: number, value: string) => void;
  }

  let {
    envVars,
    enrichment,
    values,
    onchange,
    packageArgs = [],
    argValues = {},
    onArgChange = () => {},
  }: Props = $props();

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
  {#if envVars.length === 0 && packageArgs.length === 0}
    <p class="text-sm text-muted-foreground" data-testid="auth-no-vars">
      {$t("integrations.wizard.no_auth_required")}
    </p>
  {:else}
    {#if enrichment?.auth_help_text || enrichment?.auth_help_url}
      <div class="rounded-md border border-border bg-muted/40 px-4 py-3">
        {#if enrichment?.auth_help_text}
          <p class="text-sm text-muted-foreground">{enrichment.auth_help_text}</p>
        {:else}
          <p class="text-sm text-muted-foreground">
            {$t("integrations.wizard.auth_help_default")}
          </p>
        {/if}
        {#if enrichment?.auth_help_url}
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
        {/if}
      </div>
    {/if}

    {#if packageArgs.length > 0}
      <div class="space-y-3" data-testid="wizard-step-args">
        {#each packageArgs as entry (entry.index)}
          {@const arg = entry.arg}
          {@const inputId = `arg-${entry.index}`}
          {@const label = arg.valueHint ?? `arg ${entry.index + 1}`}
          <FormField
            id={inputId}
            {label}
            labelClass="text-sm text-foreground"
            class="space-y-1.5"
            required={arg.isRequired}
            optional={!arg.isRequired}
            optionalLabel={`(${$t("integrations.wizard.optional")})`}
            hint={arg.description ?? undefined}
          >
            {#if arg.isRepeatable}
              <Textarea
                id={inputId}
                rows={2}
                class="w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm font-mono"
                placeholder={arg.valueHint ?? ""}
                value={argValues[entry.index] ?? ""}
                oninput={(e) =>
                  onArgChange(entry.index, (e.currentTarget as HTMLTextAreaElement).value)}
                data-testid={`arg-input-${entry.index}`}
              ></Textarea>
            {:else}
              <Input
                id={inputId}
                type="text"
                value={argValues[entry.index] ?? ""}
                oninput={(e) =>
                  onArgChange(entry.index, (e.currentTarget as HTMLInputElement).value)}
                placeholder={arg.valueHint ?? ""}
                autocomplete="off"
                data-testid={`arg-input-${entry.index}`}
              />
            {/if}
          </FormField>
        {/each}
      </div>
    {/if}

    {#if envVars.length === 0}
      <!-- args-only path: no env vars to render -->
    {:else}

    <div class="space-y-3">
      {#each envVars as envVar (envVar.name)}
        <FormField
          id={`env-${envVar.name}`}
          label={envVar.name}
          labelClass="text-sm text-foreground"
          class="space-y-1.5"
          required={envVar.is_required}
          optional={!envVar.is_required}
          optionalLabel={`(${$t("integrations.wizard.optional")})`}
          hint={envVar.description ?? undefined}
        >
          {#if envVar.is_secret}
            <p class="flex items-center gap-1 text-[11px] text-muted-foreground" data-testid={`env-encrypted-note-${envVar.name}`}>
              <Lock size={10} aria-hidden="true" />
              {$t("integrations.wizard.encrypted_locally")}
            </p>
          {/if}

          {#if envVar.is_secret}
            <Input
              id={`env-${envVar.name}`}
              type={inputType(envVar)}
              value={values[envVar.name] ?? ""}
              oninput={(e) => onchange(envVar.name, (e.currentTarget as HTMLInputElement).value)}
              placeholder="••••••••"
              autocomplete="off"
              data-testid={`env-input-${envVar.name}`}
            >
              {#snippet trailing()}
                <button
                  type="button"
                  class="text-muted-foreground transition-colors hover:text-foreground"
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
              {/snippet}
            </Input>
          {:else}
            <Input
              id={`env-${envVar.name}`}
              type="text"
              value={values[envVar.name] ?? ""}
              oninput={(e) => onchange(envVar.name, (e.currentTarget as HTMLInputElement).value)}
              placeholder={envVar.name}
              autocomplete="off"
              data-testid={`env-input-${envVar.name}`}
            />
          {/if}
        </FormField>
      {/each}
    </div>
    {/if}
  {/if}
</div>
