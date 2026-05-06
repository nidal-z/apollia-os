<script lang="ts">
  /**
   * Onboarding step 1 — Welcome screen.
   *
   * Static intro card that introduces Apollia and triggers the step machine.
   * Designed to fit inside the shared {@link OnboardingModal} overlay (no
   * own backdrop or fixed positioning).
   */
  import { Sparkles, ShieldCheck, Cpu } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    onnext: () => void;
  }

  const { onnext }: Props = $props();
</script>

<div class="flex flex-col items-center gap-6 px-2 py-4 text-center" data-testid="onboarding-welcome">
  <div class="flex h-16 w-16 items-center justify-center rounded-2xl bg-gradient-to-br from-primary to-secondary shadow-lg">
    <Sparkles size={28} strokeWidth={1.75} class="text-primary-foreground" />
  </div>

  <div class="space-y-2">
    <h2 class="text-xl font-semibold tracking-tight text-foreground">
      Bienvenue sur Apollia
    </h2>
    <p class="mx-auto max-w-sm text-sm leading-relaxed text-muted-foreground">
      Le runtime souverain pour exécuter tes agents IA en local.
      On va calibrer rapidement ton installation — moins de 3 minutes.
    </p>
  </div>

  <ul class="grid w-full max-w-md grid-cols-1 gap-2 text-left sm:grid-cols-3">
    <li class="flex items-start gap-2 rounded-lg border border-border/60 bg-muted/40 p-3">
      <ShieldCheck size={16} strokeWidth={1.75} class="mt-0.5 flex-shrink-0 text-primary" />
      <div>
        <p class="text-xs font-medium text-foreground">Local-first</p>
        <p class="text-[11px] leading-snug text-muted-foreground">Tes données ne quittent pas ta machine.</p>
      </div>
    </li>
    <li class="flex items-start gap-2 rounded-lg border border-border/60 bg-muted/40 p-3">
      <Cpu size={16} strokeWidth={1.75} class="mt-0.5 flex-shrink-0 text-primary" />
      <div>
        <p class="text-xs font-medium text-foreground">LLM au choix</p>
        <p class="text-[11px] leading-snug text-muted-foreground">Local (llama.cpp) ou cloud (Anthropic, OpenAI…).</p>
      </div>
    </li>
    <li class="flex items-start gap-2 rounded-lg border border-border/60 bg-muted/40 p-3">
      <Sparkles size={16} strokeWidth={1.75} class="mt-0.5 flex-shrink-0 text-primary" />
      <div>
        <p class="text-xs font-medium text-foreground">Agents autonomes</p>
        <p class="text-[11px] leading-snug text-muted-foreground">ReAct natif, pas un pipeline déterministe.</p>
      </div>
    </li>
  </ul>

  <Button
    variant="primary-gradient"
    size="lg"
    onclick={onnext}
    data-testid="onboarding-welcome-cta"
  >
    Commencer la configuration
  </Button>
</div>
