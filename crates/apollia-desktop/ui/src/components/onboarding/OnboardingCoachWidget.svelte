<script lang="ts">
  /**
   * `OnboardingCoachWidget` — floating, draggable, dismissable coach widget
   * surfaced during onboarding. Placeholder implementation: real LLM calls
   * land with US-SP42-057.
   *
   * Story: US-SP42-055.
   */
  import { t } from 'svelte-i18n';
  import { MessageCircle, X, Send } from 'lucide-svelte';

  interface Props {
    /** Onboarding stage identifier (welcome, profile_choice, ai_setup, …). */
    stage: string;
    /** Current sub-step inside the stage (free-form). */
    currentStep: string;
    /** Called when the user closes the widget. */
    ondismiss?: () => void;
  }

  let { stage, currentStep, ondismiss }: Props = $props();

  interface CoachMessage {
    role: 'user' | 'assistant';
    content: string;
  }

  let messages = $state<CoachMessage[]>([]);
  let draft = $state('');
  let position = $state({ x: 24, y: 24 });
  let dragging = $state(false);
  let dragOffset = { x: 0, y: 0 };

  function handlePointerDown(e: PointerEvent): void {
    dragging = true;
    dragOffset = { x: e.clientX - position.x, y: e.clientY - position.y };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function handlePointerMove(e: PointerEvent): void {
    if (!dragging) return;
    position = {
      x: Math.max(8, Math.min(window.innerWidth - 340, e.clientX - dragOffset.x)),
      y: Math.max(8, Math.min(window.innerHeight - 120, e.clientY - dragOffset.y)),
    };
  }

  function handlePointerUp(e: PointerEvent): void {
    dragging = false;
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
  }

  function handleSend(): void {
    const text = draft.trim();
    if (text === '') return;
    // TODO(US-SP42-057): replace stub with real Coach meta-chat call.
    messages = [
      ...messages,
      { role: 'user', content: text },
      {
        role: 'assistant',
        content: `Placeholder — US-SP42-057 Coach integration pending (stage=${stage}, step=${currentStep}).`,
      },
    ];
    draft = '';
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }
</script>

<aside
  class="coach-widget"
  role="complementary"
  aria-label={$t('onboarding_v2.coach_widget.aria_label')}
  data-testid="onboarding-coach-widget"
  style:right="auto"
  style:bottom="auto"
  style:left="{position.x}px"
  style:top="{position.y}px"
>
  <header
    class="coach-header"
    onpointerdown={handlePointerDown}
    onpointermove={handlePointerMove}
    onpointerup={handlePointerUp}
  >
    <span class="coach-title">
      <MessageCircle size={16} strokeWidth={1.75} />
      {$t('onboarding_v2.coach_widget.title')}
    </span>
    <button
      class="coach-close"
      aria-label={$t('onboarding_v2.coach_widget.close')}
      onclick={() => ondismiss?.()}
      data-testid="coach-widget-close"
    >
      <X size={14} strokeWidth={2} />
    </button>
  </header>

  <div class="coach-body" data-testid="coach-widget-messages">
    {#if messages.length === 0}
      <p class="coach-placeholder">{$t('onboarding_v2.coach_widget.placeholder')}</p>
    {:else}
      {#each messages as msg, i (i)}
        <div class="coach-msg coach-msg--{msg.role}">{msg.content}</div>
      {/each}
    {/if}
  </div>

  <div class="coach-composer">
    <input
      type="text"
      class="coach-input"
      bind:value={draft}
      onkeydown={handleKeydown}
      placeholder={$t('onboarding_v2.coach_widget.input_placeholder')}
      aria-label={$t('onboarding_v2.coach_widget.input_placeholder')}
      data-testid="coach-widget-input"
    />
    <button
      class="coach-send"
      onclick={handleSend}
      aria-label={$t('onboarding_v2.coach_widget.send')}
      data-testid="coach-widget-send"
    >
      <Send size={14} strokeWidth={2} />
    </button>
  </div>
</aside>

<style>
  .coach-widget {
    position: fixed;
    width: 20rem;
    max-height: 24rem;
    display: flex;
    flex-direction: column;
    background: hsl(var(--card));
    border: 1px solid hsl(var(--primary) / 0.12);
    border-radius: 0.875rem;
    box-shadow: var(--shadow-elev-4);
    z-index: 70;
    overflow: hidden;
  }

  .coach-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.5rem 0.75rem;
    background: linear-gradient(90deg, hsl(var(--primary) / 0.08), hsl(var(--secondary) / 0.08));
    border-bottom: 1px solid hsl(var(--primary) / 0.08);
    cursor: grab;
    user-select: none;
    touch-action: none;
  }

  .coach-header:active {
    cursor: grabbing;
  }

  .coach-title {
    display: inline-flex;
    align-items: center;
    gap: 0.375rem;
    font-size: 0.8125rem;
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .coach-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.375rem;
    height: 1.375rem;
    border: none;
    background: transparent;
    border-radius: 0.3125rem;
    color: hsl(var(--muted-foreground));
    cursor: pointer;
  }

  .coach-close:hover {
    background: hsl(var(--muted-foreground) / 0.1);
  }

  .coach-body {
    flex: 1;
    min-height: 5rem;
    max-height: 16rem;
    overflow-y: auto;
    padding: 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .coach-placeholder {
    margin: 0;
    font-size: 0.8125rem;
    color: hsl(var(--muted-foreground));
    line-height: 1.5;
  }

  .coach-msg {
    font-size: 0.8125rem;
    line-height: 1.45;
    padding: 0.5rem 0.625rem;
    border-radius: 0.5rem;
    max-width: 85%;
  }

  .coach-msg--user {
    align-self: flex-end;
    background: hsl(var(--primary) / 0.12);
    color: hsl(var(--foreground));
  }

  .coach-msg--assistant {
    align-self: flex-start;
    background: hsl(var(--muted-foreground) / 0.08);
    color: hsl(var(--foreground));
  }

  .coach-composer {
    display: flex;
    gap: 0.375rem;
    padding: 0.5rem;
    border-top: 1px solid hsl(var(--primary) / 0.08);
    background: hsl(var(--background));
  }

  .coach-input {
    flex: 1;
    padding: 0.4375rem 0.625rem;
    border-radius: 0.5rem;
    border: 1px solid hsl(var(--primary) / 0.15);
    background: hsl(var(--card));
    color: hsl(var(--foreground));
    font-size: 0.8125rem;
    outline: none;
  }

  .coach-input:focus {
    border-color: hsl(var(--primary) / 0.45);
  }

  .coach-send {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2rem;
    height: 2rem;
    border: none;
    border-radius: 0.5rem;
    background: linear-gradient(135deg, hsl(var(--primary-gradient-from)), hsl(var(--primary-gradient-to)));
    color: hsl(var(--primary-foreground));
    cursor: pointer;
    box-shadow: var(--shadow-primary-sm);
  }

  .coach-send:hover {
    box-shadow: var(--shadow-primary-md);
  }
</style>
