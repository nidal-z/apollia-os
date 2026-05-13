<script lang="ts">
  import {
    Shield,
    X,
    File,
    Zap,
    Brain,
    Activity,
    Sparkles,
    HelpCircle,
    MessageSquare,
  } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";

  export type InboxType =
    | "approval"
    | "question"
    | "error"
    | "deliverable"
    | "trigger"
    | "memory"
    | "cost";

  interface Props {
    type: InboxType;
    title: string;
    /** Source agent / system name. */
    agent: string;
    /** Display timestamp (e.g. "il y a 2 min"). */
    timestamp: string;
    unread?: boolean;
    /** Primary CTA — "Autoriser" / "Répondre" / "Résoudre" depending on type. */
    onAction?: (e: MouseEvent) => void;
    /** Optional second action shown as an icon-only button before the
     *  primary CTA. Used for ask_user items to expose "Open the source
     *  conversation" without expanding the row first. */
    onSecondaryAction?: (e: MouseEvent) => void;
    /** Accessible label for the secondary action (tooltip + aria-label). */
    secondaryLabel?: string;
  }

  let {
    type,
    title,
    agent,
    timestamp,
    unread = false,
    onAction,
    onSecondaryAction,
    secondaryLabel,
  }: Props = $props();

  type Cfg = {
    label: string;
    tone: "warning" | "danger" | "info" | "success" | "secondary" | "primary";
    iconCmp: typeof Shield;
  };

  const CFG: Record<InboxType, Cfg> = {
    approval: { label: "approbation", tone: "warning", iconCmp: Shield },
    question: { label: "question", tone: "info", iconCmp: HelpCircle },
    error: { label: "erreur", tone: "danger", iconCmp: X },
    deliverable: { label: "livrable", tone: "info", iconCmp: File },
    trigger: { label: "trigger", tone: "success", iconCmp: Zap },
    memory: { label: "mémoire", tone: "secondary", iconCmp: Brain },
    cost: { label: "coût", tone: "warning", iconCmp: Activity },
  };

  const cfg = $derived(CFG[type]);
  const IconCmp = $derived(cfg.iconCmp);
</script>

<div
  class="flex gap-2.5 px-4 py-3 border-b border-border/60 transition-colors {unread
    ? 'bg-primary/5'
    : 'bg-transparent hover:bg-muted/40'}"
>
  {#if unread}
    <div
      class="w-1 self-stretch rounded-sm bg-primary -ml-2.5 mr-1.5"
    ></div>
  {/if}
  <div
    class="w-7 h-7 rounded-lg shrink-0 inline-flex items-center justify-center"
    style="background: hsl(var(--{type === 'approval' || type === 'cost' ? 'warning' : type === 'error' ? 'destructive' : type === 'deliverable' || type === 'question' ? 'info' : type === 'trigger' ? 'success' : 'secondary'}) / 0.10); color: hsl(var(--{type === 'approval' || type === 'cost' ? 'warning' : type === 'error' ? 'destructive' : type === 'deliverable' || type === 'question' ? 'info' : type === 'trigger' ? 'success' : 'secondary'}));"
  >
    <IconCmp size={12} />
  </div>
  <div class="flex-1 min-w-0">
    <div class="flex items-center gap-1.5">
      <Badge size="sm" variant={cfg.tone}>{cfg.label}</Badge>
      <span
        class="text-[12.5px] text-foreground truncate"
        style:font-weight={unread ? 600 : 500}
      >
        {title}
      </span>
    </div>
    <div
      class="text-[10.5px] text-muted-foreground mt-1 inline-flex items-center gap-1.5"
    >
      <Sparkles size={9} />
      {agent} · {timestamp}
    </div>
  </div>
  <div class="flex items-center gap-1.5 shrink-0">
    {#if onSecondaryAction}
      <Button variant="ghost" size="sm"
        type="button"
        class="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/60 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
        onclick={(e) => {
          e.stopPropagation();
          onSecondaryAction?.(e);
        }}
        aria-label={secondaryLabel ?? "Open conversation"}
        title={secondaryLabel ?? "Open conversation"}
        data-testid="inbox-row-secondary"
      >
        <MessageSquare size={14} strokeWidth={1.75} />
      </Button>
    {/if}
    {#if type === "approval"}
      <Button variant="primary-solid" size="sm" onclick={onAction}>Autoriser</Button>
    {:else if type === "question"}
      <Button variant="outline" size="sm" onclick={onAction}>Répondre</Button>
    {:else if type === "error"}
      <Button variant="outline" size="sm" onclick={onAction}>Résoudre</Button>
    {/if}
  </div>
</div>
