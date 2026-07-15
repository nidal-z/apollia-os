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
  import { t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import ListRow from "./ListRow.svelte";

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
    /** Primary CTA - "Autoriser" / "Répondre" / "Résoudre" depending on type. */
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
    labelKey: string;
    tone: "warning" | "danger" | "info" | "success" | "secondary" | "primary";
    iconCmp: typeof Shield;
  };

  const CFG: Record<InboxType, Cfg> = {
    approval: { labelKey: "inbox.row.type_approval", tone: "warning", iconCmp: Shield },
    question: { labelKey: "inbox.row.type_question", tone: "info", iconCmp: HelpCircle },
    error: { labelKey: "inbox.row.type_error", tone: "danger", iconCmp: X },
    deliverable: { labelKey: "inbox.row.type_deliverable", tone: "info", iconCmp: File },
    trigger: { labelKey: "inbox.row.type_trigger", tone: "success", iconCmp: Zap },
    memory: { labelKey: "inbox.row.type_memory", tone: "secondary", iconCmp: Brain },
    cost: { labelKey: "inbox.row.type_cost", tone: "warning", iconCmp: Activity },
  };

  const cfg = $derived(CFG[type]);
  const IconCmp = $derived(cfg.iconCmp);
</script>

<ListRow align="stretch" state={unread ? "unread" : "default"}>
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
      <Badge size="sm" variant={cfg.tone}>{$t(cfg.labelKey)}</Badge>
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
        aria-label={secondaryLabel ?? $t("inbox.row.open_conversation")}
        title={secondaryLabel ?? $t("inbox.row.open_conversation")}
        data-testid="inbox-row-secondary"
      >
        <MessageSquare size={14} strokeWidth={1.75} />
      </Button>
    {/if}
    {#if type === "approval"}
      <Button variant="primary-solid" size="sm" onclick={onAction} data-testid="inbox-row-primary">{$t("inbox.row.action_allow")}</Button>
    {:else if type === "question"}
      <Button variant="outline" size="sm" onclick={onAction} data-testid="inbox-row-primary">{$t("inbox.row.action_reply")}</Button>
    {:else if type === "error"}
      <Button variant="outline" size="sm" onclick={onAction} data-testid="inbox-row-primary">{$t("inbox.row.action_resolve")}</Button>
    {/if}
  </div>
</ListRow>
