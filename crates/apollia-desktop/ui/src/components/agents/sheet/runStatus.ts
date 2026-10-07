// Status vocabulary of a run listed on an agent's sheet.
//
// Maps the runtime `TaskSummary["status"]` to display concerns (rail colour,
// Badge variant, status Lucide icon, i18n key). Colours are token references,
// never raw palette.

import { Activity, CheckCircle, Clock, XCircle, AlertTriangle, Ban } from "lucide-svelte";
import type { Icon } from "lucide-svelte";
import type { TaskSummary } from "$lib/types";

type Status = TaskSummary["status"];
type BadgeVariant = "success" | "destructive" | "warning" | "info" | "secondary";

interface StatusMeta {
  /** Sidebar rail colour (token reference). */
  rail: string;
  /** Badge tone. */
  variant: BadgeVariant;
  /** Header status icon. */
  icon: typeof Icon;
  /** i18n key under the `run_status.*` family. */
  i18nKey: string;
}

const META: Record<Status, StatusMeta> = {
  working: {
    rail: "hsl(var(--primary))",
    variant: "info",
    icon: Activity,
    i18nKey: "run_status.working",
  },
  submitted: {
    rail: "hsl(var(--info))",
    variant: "secondary",
    icon: Clock,
    i18nKey: "run_status.submitted",
  },
  input_required: {
    rail: "hsl(var(--warning))",
    variant: "warning",
    icon: AlertTriangle,
    i18nKey: "run_status.approval",
  },
  completed: {
    rail: "hsl(var(--success))",
    variant: "success",
    icon: CheckCircle,
    i18nKey: "run_status.completed",
  },
  failed: {
    rail: "hsl(var(--destructive))",
    variant: "destructive",
    icon: XCircle,
    i18nKey: "run_status.failed",
  },
  canceled: {
    rail: "hsl(var(--muted-foreground))",
    variant: "secondary",
    icon: Ban,
    i18nKey: "run_status.canceled",
  },
};

const FALLBACK = META.submitted;

export function statusMeta(status: Status): StatusMeta {
  return META[status] ?? FALLBACK;
}
