/**
 * One tool call of the turn being streamed.
 *
 * `reasoningCursor` records how many closed reasoning fragments had streamed
 * when the tool started, so the live timeline interleaves reasoning captions and
 * tool rows in true arrival order instead of grouping them by kind.
 */
export interface LiveToolCall {
  name: string;
  status: "running" | "done" | "refused";
  startedAt: number;
  durationMs?: number;
  reasoningCursor: number;
}
