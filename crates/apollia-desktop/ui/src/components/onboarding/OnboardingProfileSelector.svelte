<script lang="ts">
  /**
   * Onboarding step 2 - Profile selector.
   *
   * Two-card chooser (operator vs builder). The selection is persisted via
   * `set_onboarding_profile`, mirrored into the local {@link uiMode} store
   * (so the rest of the UI adapts immediately), then the modal advances.
   */
  import { t } from "svelte-i18n";
  import { Sparkles, Code2, AlertCircle } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { onboardingStore } from "$lib/stores/onboarding";
  import { uiMode } from "$lib/stores/mode";
  import { Button } from "$lib/components/ui/button";

  type Profile = "operator" | "builder";

  interface Props {
    onnext: () => void;
    onback: () => void;
  }

  const { onnext, onback }: Props = $props();

  let selecting = $state<Profile | null>(null);
  let error = $state<string | null>(null);

  async function handleSelect(profile: Profile): Promise<void> {
    if (selecting !== null) return;
    selecting = profile;
    error = null;
    try {
      await onboardingStore.setProfile(profile);
      uiMode.set(profile);
      onnext();
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
      selecting = null;
    }
  }

  // Keyboard quick-select: 1 = operator, 2 = builder. Ignored while a choice
  // is already resolving or when the focus sits in a text field.
  function onKeydown(event: KeyboardEvent): void {
    if (selecting !== null) return;
    const target = event.target as HTMLElement | null;
    if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) {
      return;
    }
    if (event.key === "1") {
      event.preventDefault();
      void handleSelect("operator");
    } else if (event.key === "2") {
      event.preventDefault();
      void handleSelect("builder");
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="flex flex-col gap-5" data-testid="onboarding-profile-selector">
  <header class="space-y-1.5 text-center">
    <h2 class="text-xl font-semibold tracking-tight text-foreground">
      {$t("onboarding.profile_selector.title")}
    </h2>
    <p class="text-sm text-muted-foreground">
      {$t("onboarding.profile_selector.subtitle")}
    </p>
  </header>

  {#if error}
    <p
      class="flex items-center gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive"
      role="alert"
    >
      <AlertCircle size={14} strokeWidth={2} aria-hidden="true" />
      {error}
    </p>
  {/if}

  <div class="grid gap-3 sm:grid-cols-2">
    <button
      type="button"
      class="profile-card"
      class:is-selecting={selecting === "operator"}
      onclick={() => handleSelect("operator")}
      disabled={selecting !== null}
      data-testid="profile-card-operator"
    >
      <div class="card-icon card-icon-operator">
        {#if selecting === "operator"}
          <Spinner size={22} />
        {:else}
          <Sparkles size={22} strokeWidth={1.75} />
        {/if}
      </div>
      <h3 class="card-title">{$t("onboarding.profile_selector.operator_title")}</h3>
      <p class="card-desc">
        {$t("onboarding.profile_selector.operator_desc")}
      </p>
      <ul class="card-bullets">
        <li>{$t("onboarding.profile_selector.operator_bullet_1")}</li>
        <li>{$t("onboarding.profile_selector.operator_bullet_2")}</li>
        <li>{$t("onboarding.profile_selector.operator_bullet_3")}</li>
      </ul>
      <p class="card-examples">{$t("onboarding.profile_selector.operator_examples")}</p>
    </button>

    <button
      type="button"
      class="profile-card"
      class:is-selecting={selecting === "builder"}
      onclick={() => handleSelect("builder")}
      disabled={selecting !== null}
      data-testid="profile-card-builder"
    >
      <div class="card-icon card-icon-builder">
        {#if selecting === "builder"}
          <Spinner size={22} />
        {:else}
          <Code2 size={22} strokeWidth={1.75} />
        {/if}
      </div>
      <h3 class="card-title">Builder</h3>
      <p class="card-desc">
        {$t("onboarding.profile_selector.builder_desc")}
      </p>
      <ul class="card-bullets">
        <li>{$t("onboarding.profile_selector.builder_bullet_1")}</li>
        <li>{$t("onboarding.profile_selector.builder_bullet_2")}</li>
        <li>{$t("onboarding.profile_selector.builder_bullet_3")}</li>
      </ul>
      <p class="card-examples">{$t("onboarding.profile_selector.builder_examples")}</p>
    </button>
  </div>

  <footer class="flex items-center justify-between pt-1">
    <Button variant="ghost" size="sm"
      type="button"
      class="text-xs text-muted-foreground underline-offset-4 hover:text-foreground hover:underline disabled:opacity-50"
      onclick={onback}
      disabled={selecting !== null}
      data-testid="profile-back"
    >
      {$t("onboarding.profile_selector.back")}
    </Button>
    <Button variant="ghost" size="sm"
      type="button"
      class="text-xs text-muted-foreground underline-offset-4 hover:text-foreground hover:underline disabled:opacity-50"
      onclick={() => handleSelect("builder")}
      disabled={selecting !== null}
      data-testid="profile-both"
    >
      {$t("onboarding.profile_selector.both")}
    </Button>
  </footer>
</div>

<style>
  .profile-card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.625rem;
    padding: 1.25rem;
    border-radius: 0.875rem;
    border: 1.5px solid hsl(var(--border));
    background: hsl(var(--card));
    text-align: left;
    cursor: pointer;
    transition:
      transform 150ms ease,
      border-color 150ms ease,
      box-shadow 150ms ease,
      background 150ms ease;
  }

  .profile-card:hover:not(:disabled) {
    transform: translateY(-2px);
    border-color: hsl(var(--primary) / 0.45);
    box-shadow: 0 8px 24px -10px hsl(var(--primary) / 0.25);
    background: hsl(var(--card));
  }

  .profile-card:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .profile-card.is-selecting {
    border-color: hsl(var(--primary));
    opacity: 1;
    box-shadow: 0 0 0 3px hsl(var(--primary) / 0.15);
  }

  .profile-card:focus-visible {
    outline: 2px solid hsl(var(--primary));
    outline-offset: 2px;
  }

  .card-icon {
    width: 2.5rem;
    height: 2.5rem;
    border-radius: 0.625rem;
    display: flex;
    align-items: center;
    justify-content: center;
    color: hsl(var(--primary-foreground));
  }

  .card-icon-operator {
    background: linear-gradient(135deg, hsl(var(--primary)), hsl(var(--secondary)));
  }

  .card-icon-builder {
    background: linear-gradient(135deg, hsl(var(--secondary)), hsl(var(--primary)));
  }

  .card-title {
    font-size: 1rem;
    font-weight: 600;
    color: hsl(var(--foreground));
    margin: 0;
    letter-spacing: -0.01em;
  }

  .card-desc {
    font-size: 0.8125rem;
    color: hsl(var(--foreground) / 0.78);
    margin: 0;
    line-height: 1.5;
  }

  .card-bullets {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.75rem;
    color: hsl(var(--muted-foreground));
    line-height: 1.45;
  }

  .card-bullets li {
    position: relative;
    padding-left: 0.875rem;
  }

  .card-bullets li::before {
    content: "";
    position: absolute;
    left: 0;
    top: 0.5em;
    width: 0.3rem;
    height: 0.3rem;
    border-radius: 50%;
    background: hsl(var(--primary) / 0.55);
  }

  .card-examples {
    font-size: 0.6875rem;
    color: hsl(var(--muted-foreground) / 0.85);
    margin: 0.25rem 0 0;
    font-style: italic;
  }
</style>
