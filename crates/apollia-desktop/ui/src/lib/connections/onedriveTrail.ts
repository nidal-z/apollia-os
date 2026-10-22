/**
 * The path of folders opened while browsing a OneDrive to choose what Apollia
 * may read: empty at the root of the drive, one step per folder opened.
 */

/** One folder opened while browsing. */
export interface TrailStep {
  id: string;
  name: string;
}

/** The trail after opening `folder` from the end of `trail`. */
export function openFolder(trail: TrailStep[], folder: TrailStep): TrailStep[] {
  return [...trail, { id: folder.id, name: folder.name }];
}

/** The trail one folder up; the root stays the root. */
export function goUp(trail: TrailStep[]): TrailStep[] {
  return trail.slice(0, -1);
}

/** The folder being shown: the last one opened, or the root (null). */
export function currentFolder(trail: TrailStep[]): string | null {
  return trail.length > 0 ? trail[trail.length - 1].id : null;
}
