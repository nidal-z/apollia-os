/**
 * The state behind the sheet of one workflow: the workflow, its runs, the
 * run that is open with its steps, and the launch and stop gestures.
 *
 * A run still going is read again every second while the sheet is mounted:
 * the steps move without any gesture, and the history follows.
 */
import { getAgentPackageDetail } from "$lib/ipc/library";
import {
  cancelWorkflowRun,
  getWorkflow,
  getWorkflowRun,
  launchWorkflow,
  listWorkflowRuns,
  type WorkflowRun,
  type WorkflowRunDetail,
  type WorkflowView,
} from "$lib/ipc/workflows";
import { reportError } from "$lib/errors/reportError";
import type { PackageRequiredConnection } from "$lib/types";
import {
  isRunning,
  runningRuns,
  splitWorkflowId,
  workflowAccess,
  type WorkflowAccess,
} from "./workflowRuns";

/** Milliseconds between two readings of a run still going. */
const REFRESH_MS = 1000;

export interface WorkflowSheetState {
  readonly workflow: WorkflowView | null;
  readonly runs: WorkflowRun[];
  /** The run shown above the steps, with its steps. */
  readonly open: WorkflowRunDetail | null;
  readonly access: WorkflowAccess | null;
  /** The connections the package of the workflow requires. */
  readonly connections: PackageRequiredConnection[];
  readonly loading: boolean;
  /** Why the workflow could not be read. */
  readonly loadError: string | null;
  readonly stopping: boolean;
  /** The clock a run still going is measured against. */
  readonly now: number;
  /** Launches the workflow; rejects with the engine's message when refused. */
  launch(inputs: Record<string, unknown>): Promise<void>;
  /** Shows the run `runId` above the steps. */
  openRun(runId: string): Promise<void>;
  /** Goes back to the steps as the workflow declares them. */
  closeRun(): void;
  /** Stops the run that is open. */
  stop(): Promise<void>;
}

export function createWorkflowSheet(workflowId: () => string): WorkflowSheetState {
  let workflow = $state<WorkflowView | null>(null);
  let runs = $state<WorkflowRun[]>([]);
  let open = $state<WorkflowRunDetail | null>(null);
  let access = $state<WorkflowAccess | null>(null);
  let connections = $state<PackageRequiredConnection[]>([]);
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let stopping = $state(false);
  let now = $state(Date.now());

  async function readRuns(id: string): Promise<void> {
    const read = await listWorkflowRuns(id);
    if (id === workflowId()) runs = read;
  }

  async function readOpen(runId: string): Promise<void> {
    const detail = await getWorkflowRun(runId);
    if (open === null || open.run.runId === runId) open = detail;
  }

  async function readPackage(id: string): Promise<void> {
    const { package: pkg, name } = splitWorkflowId(id);
    try {
      const detail = await getAgentPackageDetail(pkg);
      if (id !== workflowId()) return;
      access = workflowAccess(detail.manifest, name);
      connections = detail.content.connections_required;
    } catch {
      // The sheet reads without its package: the accesses then say that
      // nothing is known, which the section states.
      access = null;
      connections = [];
    }
  }

  // The workflow named by the sheet: read at mount and whenever it changes.
  $effect(() => {
    const id = workflowId();
    let live = true;
    loading = true;
    loadError = null;
    workflow = null;
    runs = [];
    open = null;
    access = null;
    connections = [];
    void (async () => {
      try {
        const [view, history] = await Promise.all([getWorkflow(id), listWorkflowRuns(id)]);
        if (!live) return;
        workflow = view;
        runs = history;
        void readPackage(id);
        const going = runningRuns(history)[0];
        if (going) open = await getWorkflowRun(going.runId);
      } catch (err) {
        if (live) loadError = reportError(err, { surface: "inline" }).friendly_message;
      } finally {
        if (live) loading = false;
      }
    })();
    return () => {
      live = false;
    };
  });

  // While a run is going, the open run and the history are read again.
  $effect(() => {
    const going = (open !== null && isRunning(open.run)) || runningRuns(runs).length > 0;
    if (!going) return;
    const id = workflowId();
    const timer = setInterval(() => {
      now = Date.now();
      const shown = open?.run.runId;
      void readRuns(id).catch(() => undefined);
      if (shown) void readOpen(shown).catch(() => undefined);
    }, REFRESH_MS);
    return () => clearInterval(timer);
  });

  return {
    get workflow() {
      return workflow;
    },
    get runs() {
      return runs;
    },
    get open() {
      return open;
    },
    get access() {
      return access;
    },
    get connections() {
      return connections;
    },
    get loading() {
      return loading;
    },
    get loadError() {
      return loadError;
    },
    get stopping() {
      return stopping;
    },
    get now() {
      return now;
    },
    async launch(inputs) {
      const id = workflowId();
      const run = await launchWorkflow(id, inputs);
      now = Date.now();
      runs = [run, ...runs.filter((known) => known.runId !== run.runId)];
      open = { run, steps: [] };
      await readOpen(run.runId).catch(() => undefined);
    },
    async openRun(runId) {
      try {
        now = Date.now();
        open = await getWorkflowRun(runId);
      } catch (err) {
        reportError(err, { surface: "toast" });
      }
    },
    closeRun() {
      open = null;
    },
    async stop() {
      if (open === null || stopping) return;
      const runId = open.run.runId;
      stopping = true;
      try {
        await cancelWorkflowRun(runId);
      } catch (err) {
        reportError(err, { surface: "toast" });
      } finally {
        stopping = false;
      }
      await Promise.all([
        readOpen(runId).catch(() => undefined),
        readRuns(workflowId()).catch(() => undefined),
      ]);
    },
  };
}
