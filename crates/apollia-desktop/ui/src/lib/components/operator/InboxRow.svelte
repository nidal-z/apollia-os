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
  } from "lucide-svelte";
  import Chip from "./Chip.svelte";
  import BtnPrimary from "./BtnPrimary.svelte";
  import BtnSecondary from "./BtnSecondary.svelte";

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
    onclick?: (e: MouseEvent) => void;
    onAction?: (e: MouseEvent) => void;
  }

  let {
    type,
    title,
    agent,
    timestamp,
    unread = false,
    onclick,
    onAction,
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
  role={onclick ? "button" : undefined}
  tabindex={onclick ? 0 : undefined}
  onclick={onclick}
  onkeydown={onclick
    ? (e) => {
        if (e.key === "Enter") onclick(e as unknown as MouseEvent);
      }
    : undefined}
  class="flex gap-2.5 px-4 py-3 cursor-pointer border-b border-border/60 transition-colors {unread
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
      <Chip size="sm" tone={cfg.tone}>{cfg.label}</Chip>
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
  {#if type === "approval"}
    <BtnPrimary onclick={onAction}>Autoriser</BtnPrimary>
  {:else if type === "question"}
    <BtnSecondary onclick={onAction}>Répondre</BtnSecondary>
  {:else if type === "error"}
    <BtnSecondary onclick={onAction}>Résoudre</BtnSecondary>
  {/if}
</div>
