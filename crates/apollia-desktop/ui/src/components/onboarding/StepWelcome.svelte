<script lang="ts">
  import { t } from "svelte-i18n";
  import { Button } from "$lib/components/ui/button";
  import { Code, Sparkles } from "lucide-svelte";

  interface Props {
    onChoose: (profile: "operator" | "builder") => void;
  }

  let { onChoose }: Props = $props();
</script>

<div class="space-y-8 text-center" data-testid="step-welcome">
  <div>
    <h1 class="text-3xl font-bold text-primary">{$t('onboarding.welcome_title')}</h1>
    <p class="mt-2 text-base text-muted-foreground">
      {$t('onboarding.welcome_subtitle')}
    </p>
    <p class="mt-1 text-sm text-muted-foreground">
      {$t('onboarding.welcome_pitch')}
    </p>
  </div>

  <div class="mx-auto grid max-w-lg grid-cols-2 gap-4">
    <button
      class="group flex flex-col items-center gap-3 rounded-xl border-2 border-muted bg-card p-6 transition-all hover:border-primary hover:shadow-md"
      onclick={() => onChoose("builder")}
      data-testid="profile-builder"
    >
      <span class="flex h-12 w-12 items-center justify-center rounded-full bg-primary/10 text-primary transition-colors group-hover:bg-primary group-hover:text-primary-foreground">
        <Code size={24} />
      </span>
      <span class="text-sm font-semibold">{$t('onboarding.profile_builder')}</span>
      <span class="text-xs text-muted-foreground">{$t('onboarding.profile_builder_desc')}</span>
    </button>

    <button
      class="group flex flex-col items-center gap-3 rounded-xl border-2 border-muted bg-card p-6 transition-all hover:border-primary hover:shadow-md"
      onclick={() => onChoose("operator")}
      data-testid="profile-operator"
    >
      <span class="flex h-12 w-12 items-center justify-center rounded-full bg-primary/10 text-primary transition-colors group-hover:bg-primary group-hover:text-primary-foreground">
        <Sparkles size={24} />
      </span>
      <span class="text-sm font-semibold">{$t('onboarding.profile_operator')}</span>
      <span class="text-xs text-muted-foreground">{$t('onboarding.profile_operator_desc')}</span>
    </button>
  </div>
</div>
