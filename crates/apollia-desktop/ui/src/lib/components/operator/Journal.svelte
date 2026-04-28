<script lang="ts">
  import {
    User,
    MessageCircle,
    Brain,
    Cpu,
    Zap,
    Workflow,
    Shield,
    X,
  } from "lucide-svelte";

  export type JournalEventType =
    | "msg"
    | "msg_out"
    | "think"
    | "ctx"
    | "llm"
    | "tool"
    | "plan"
    | "wait"
    | "hitl"
    | "err"
    | "mem"
    | "trigger";

  export type JournalEvent = {
    type: JournalEventType;
    /** Heading (event title or technical name in builder mode). */
    heading: string;
    /** Body description. */
    body: string;
    /** Timestamp label. */
    time: string;
    live?: boolean;
  };

  export type JournalMode = "operator" | "builder";

  interface Props {
    events: JournalEvent[];
    mode?: JournalMode;
  }

  let { events, mode = "operator" }: Props = $props();

  type EvCfg = {
    color: string;
    iconBg: string;
    iconCmp: typeof User;
  };

  const CFG: Record<JournalEventType, EvCfg> = {
    msg: {
      color: "hsl(var(--warm-accent))",
      iconBg: "hsl(var(--warm-accent) / 0.13)",
      iconCmp: User,
    },
    msg_out: {
      color: "hsl(var(--warm-accent))",
      iconBg: "hsl(var(--warm-accent) / 0.13)",
      iconCmp: MessageCircle,
    },
    think: {
      color: "hsl(var(--secondary))",
      iconBg: "hsl(var(--secondary) / 0.13)",
      iconCmp: Brain,
    },
    ctx: {
      color: "hsl(var(--secondary))",
      iconBg: "hsl(var(--secondary) / 0.13)",
      iconCmp: Brain,
    },
    llm: {
      color: "hsl(var(--primary))",
      iconBg: "hsl(var(--primary) / 0.13)",
      iconCmp: Cpu,
    },
    tool: {
      color: "hsl(var(--primary))",
      iconBg: "hsl(var(--primary) / 0.13)",
      iconCmp: Zap,
    },
    plan: {
      color: "hsl(var(--primary))",
      iconBg: "hsl(var(--primary) / 0.13)",
      iconCmp: Workflow,
    },
    wait: {
      color: "hsl(var(--warning))",
      iconBg: "hsl(var(--warning) / 0.13)",
      iconCmp: Shield,
    },
    hitl: {
      color: "hsl(var(--warning))",
      iconBg: "hsl(var(--warning) / 0.13)",
      iconCmp: Shield,
    },
    err: {
      color: "hsl(var(--destructive))",
      iconBg: "hsl(var(--destructive) / 0.13)",
      iconCmp: X,
    },
    mem: {
      color: "hsl(var(--info))",
      iconBg: "hsl(var(--info) / 0.13)",
      iconCmp: Brain,
    },
    trigger: {
      color: "hsl(var(--warning))",
      iconBg: "hsl(var(--warning) / 0.13)",
      iconCmp: Zap,
    },
  };

  const isBuilder = $derived(mode === "builder");
</script>

<div class="rounded-[10px] px-4 py-3.5 relative bg-card border border-border">
  <div class="absolute top-[18px] bottom-[18px] left-[22px] w-px bg-border"></div>
  {#each events as e}
    {@const cfg = CFG[e.type]}
    {@const IconCmp = cfg.iconCmp}
    <div class="flex gap-2.5 mb-2.5 relative">
      <div
        class="w-3 h-3 rounded-full shrink-0 z-10 mt-0.5 inline-flex items-center justify-center {e.live
          ? 'animate-glow-pulse'
          : ''}"
        style="background: {cfg.iconBg}; color: {cfg.color}; border: 1.5px solid {e.live
          ? cfg.color
          : 'hsl(var(--card))'}; box-shadow: {e.live ? `0 0 0 3px ${cfg.iconBg}` : 'none'};"
      >
        <IconCmp size={7} />
      </div>
      <div class="flex-1 min-w-0">
        <div class="flex items-baseline gap-1.5">
          <span
            class="text-[11px] font-semibold text-foreground {isBuilder ? 'font-mono' : ''}"
          >
            {e.heading}
          </span>
          <span class="ml-auto text-[9.5px] font-mono text-muted-foreground">
            {e.time}
          </span>
        </div>
        <div
          class="text-[10.5px] mt-0.5 {isBuilder ? 'font-mono whitespace-pre-wrap break-words' : ''}"
          style:color={e.live ? cfg.color : "hsl(var(--muted-foreground))"}
        >
          {e.body}
        </div>
      </div>
    </div>
  {/each}
</div>
