<script module lang="ts">
  import { sha256Hex } from "$lib/utils/hash";

  /// The four checklist items shown in the interactive disclaimer.
  /// The version hash is derived from this exact ordered list — any change
  /// to wording or order will force existing users to re-accept.
  export const DISCLAIMER_ITEMS = [
    "i18n:integrations.disclaimer.items.code_on_machine",
    "i18n:integrations.disclaimer.items.external_data",
    "i18n:integrations.disclaimer.items.revocable",
    "i18n:integrations.disclaimer.items.read_capabilities",
  ] as const;

  /// localStorage key: last accepted disclaimer version (SHA-256 of items).
  export const DISCLAIMER_VERSION_KEY = "apollia-mcp-disclaimer-version";

  /// Compute the disclaimer version hash for the current item set.
  export async function computeDisclaimerVersion(): Promise<string> {
    return sha256Hex(DISCLAIMER_ITEMS.join("|"));
  }

  /// Returns true if the user accepted a disclaimer with the current version hash.
  export async function isDisclaimerVersionAccepted(): Promise<boolean> {
    const current = await computeDisclaimerVersion();
    return localStorage.getItem(DISCLAIMER_VERSION_KEY) === current;
  }

  /// Record acceptance for the current version.
  export async function recordDisclaimerAcceptance(): Promise<void> {
    const current = await computeDisclaimerVersion();
    localStorage.setItem(DISCLAIMER_VERSION_KEY, current);
  }
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import {
    Shield,
    Globe,
    Undo2,
    BookOpen,
    ExternalLink,
  } from "lucide-svelte";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { navigateTo } from "$lib/stores/navigation";
  import type { ComponentType } from "svelte";

  interface Props {
    checks: Record<string, boolean>;
    onchange: (key: string, value: boolean) => void;
  }

  let { checks, onchange }: Props = $props();

  interface Item {
    key: string;
    icon: ComponentType;
    titleKey: string;
    descKey: string;
    critical: boolean;
  }

  const ITEMS: Item[] = [
    {
      key: "code_on_machine",
      icon: Shield,
      titleKey: "integrations.disclaimer.items.code_on_machine",
      descKey: "integrations.disclaimer.items.code_on_machine_desc",
      critical: true,
    },
    {
      key: "external_data",
      icon: Globe,
      titleKey: "integrations.disclaimer.items.external_data",
      descKey: "integrations.disclaimer.items.external_data_desc",
      critical: true,
    },
    {
      key: "revocable",
      icon: Undo2,
      titleKey: "integrations.disclaimer.items.revocable",
      descKey: "integrations.disclaimer.items.revocable_desc",
      critical: false,
    },
    {
      key: "read_capabilities",
      icon: BookOpen,
      titleKey: "integrations.disclaimer.items.read_capabilities",
      descKey: "integrations.disclaimer.items.read_capabilities_desc",
      critical: false,
    },
  ];

  function openSecurityDocs(): void {
    navigateTo("settings");
    // In a full build this would open the dedicated /docs/security route.
  }
</script>

<div class="space-y-4" data-testid="wizard-step-disclaimer">
  <div>
    <h3 class="text-sm font-semibold text-foreground">
      {$t("integrations.disclaimer.step_title")}
    </h3>
    <p class="mt-1 text-sm text-muted-foreground">
      {$t("integrations.disclaimer.step_subtitle")}
    </p>
  </div>

  <ul class="space-y-3">
    {#each ITEMS as item (item.key)}
      {@const Icon = item.icon}
      <li
        class="flex items-start gap-3 rounded-md border border-border bg-muted/30 p-3"
        data-testid={`disclaimer-item-${item.key}`}
      >
        <Checkbox
          id={`disclaimer-${item.key}`}
          checked={checks[item.key] ?? false}
          onchange={(v: boolean) => onchange(item.key, v)}
          data-testid={`disclaimer-check-${item.key}`}
        />
        <Icon
          size={16}
          class={item.critical
            ? "mt-0.5 shrink-0 text-destructive"
            : "mt-0.5 shrink-0 text-muted-foreground"}
          aria-hidden="true"
        />
        <label for={`disclaimer-${item.key}`} class="flex-1 cursor-pointer">
          <p
            class={item.critical
              ? "text-sm font-medium text-destructive"
              : "text-sm font-medium text-foreground"}
          >
            {$t(item.titleKey)}
          </p>
          <p class="mt-0.5 text-xs text-muted-foreground">
            {$t(item.descKey)}
          </p>
        </label>
      </li>
    {/each}
  </ul>

  <button
    type="button"
    onclick={openSecurityDocs}
    class="inline-flex items-center gap-1 text-xs text-primary underline-offset-4 hover:underline"
    data-testid="disclaimer-learn-more"
  >
    <ExternalLink size={12} aria-hidden="true" />
    {$t("integrations.disclaimer.learn_more")}
  </button>
</div>
