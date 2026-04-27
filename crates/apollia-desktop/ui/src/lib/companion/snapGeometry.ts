/**
 * Companion geometry helpers — snap-to-edge, clamping, viewport validation.
 *
 * Pure functions only: no DOM reads, all inputs passed explicitly so the
 * module is trivially unit-testable.
 */

export interface Position {
  x: number;
  y: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface Viewport {
  width: number;
  height: number;
}

export interface CompanionGeometry {
  position: Position;
  size: Size;
}

export const SNAP_GUIDE_DISTANCE_PX = 40;
export const SNAP_TRIGGER_DISTANCE_PX = 20;

export const COMPANION_MIN_SIZE: Size = { width: 320, height: 400 };

/** Maximum size expressed as a fraction of the viewport. */
export function maxCompanionSize(viewport: Viewport): Size {
  return {
    width: Math.floor(viewport.width * 0.8),
    height: Math.floor(viewport.height * 0.8),
  };
}

/** Clamps a size to the Companion [min, max] bounds. */
export function clampSize(size: Size, viewport: Viewport): Size {
  const max = maxCompanionSize(viewport);
  return {
    width: Math.max(COMPANION_MIN_SIZE.width, Math.min(max.width, size.width)),
    height: Math.max(
      COMPANION_MIN_SIZE.height,
      Math.min(max.height, size.height),
    ),
  };
}

/** Clamps a position so the panel stays within the visible viewport. */
export function clampPosition(
  pos: Position,
  size: Size,
  viewport: Viewport,
): Position {
  return {
    x: Math.max(0, Math.min(pos.x, viewport.width - size.width)),
    y: Math.max(0, Math.min(pos.y, viewport.height - size.height)),
  };
}

export interface EdgeDistances {
  left: number;
  right: number;
  top: number;
  bottom: number;
}

/** Distances from each viewport edge to the nearest panel edge. */
export function edgeDistances(
  pos: Position,
  size: Size,
  viewport: Viewport,
): EdgeDistances {
  return {
    left: pos.x,
    right: viewport.width - (pos.x + size.width),
    top: pos.y,
    bottom: viewport.height - (pos.y + size.height),
  };
}

/** Which edges are close enough to warrant showing a guide line (<40 px). */
export interface EdgeGuides {
  left: boolean;
  right: boolean;
  top: boolean;
  bottom: boolean;
}

export function edgeGuides(distances: EdgeDistances): EdgeGuides {
  return {
    left: distances.left < SNAP_GUIDE_DISTANCE_PX && distances.left >= 0,
    right: distances.right < SNAP_GUIDE_DISTANCE_PX && distances.right >= 0,
    top: distances.top < SNAP_GUIDE_DISTANCE_PX && distances.top >= 0,
    bottom: distances.bottom < SNAP_GUIDE_DISTANCE_PX && distances.bottom >= 0,
  };
}

/**
 * Snaps a position to the nearest viewport edge whenever the current
 * distance is below {@link SNAP_TRIGGER_DISTANCE_PX}. Returns the resulting
 * position along with a flag indicating whether snapping occurred.
 */
export function snapToEdges(
  pos: Position,
  size: Size,
  viewport: Viewport,
): { position: Position; snapped: boolean } {
  const d = edgeDistances(pos, size, viewport);
  let { x, y } = pos;
  let snapped = false;

  if (d.left >= 0 && d.left < SNAP_TRIGGER_DISTANCE_PX) {
    x = 0;
    snapped = true;
  } else if (d.right >= 0 && d.right < SNAP_TRIGGER_DISTANCE_PX) {
    x = viewport.width - size.width;
    snapped = true;
  }

  if (d.top >= 0 && d.top < SNAP_TRIGGER_DISTANCE_PX) {
    y = 0;
    snapped = true;
  } else if (d.bottom >= 0 && d.bottom < SNAP_TRIGGER_DISTANCE_PX) {
    y = viewport.height - size.height;
    snapped = true;
  }

  return { position: { x, y }, snapped };
}

/** Validates and repairs geometry loaded from persistent storage. */
export function validateGeometry(
  geometry: CompanionGeometry,
  viewport: Viewport,
): CompanionGeometry {
  const size = clampSize(geometry.size, viewport);
  const position = clampPosition(geometry.position, size, viewport);
  return { position, size };
}
