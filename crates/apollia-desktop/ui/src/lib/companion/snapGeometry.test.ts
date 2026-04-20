import { describe, expect, test } from "vitest";
import {
  COMPANION_MIN_SIZE,
  SNAP_GUIDE_DISTANCE_PX,
  SNAP_TRIGGER_DISTANCE_PX,
  clampPosition,
  clampSize,
  edgeDistances,
  edgeGuides,
  maxCompanionSize,
  snapToEdges,
  validateGeometry,
} from "./snapGeometry";

const VIEWPORT = { width: 1280, height: 800 };
const SIZE = { width: 380, height: 520 };

describe("clampSize", () => {
  test("enforces minimum bounds", () => {
    // GIVEN a size below the minimum
    // WHEN clamped
    const out = clampSize({ width: 100, height: 100 }, VIEWPORT);
    // THEN bumped to min size
    expect(out).toEqual(COMPANION_MIN_SIZE);
  });

  test("enforces maximum (80vw x 80vh)", () => {
    // GIVEN an oversized request
    // WHEN clamped
    const out = clampSize({ width: 5000, height: 5000 }, VIEWPORT);
    // THEN capped at the viewport-based max
    expect(out).toEqual(maxCompanionSize(VIEWPORT));
  });
});

describe("clampPosition", () => {
  test("keeps panel inside the viewport", () => {
    // GIVEN a position pushing the panel partially off-screen
    const out = clampPosition({ x: 5000, y: -200 }, SIZE, VIEWPORT);
    // THEN both axes are clamped to in-bounds
    expect(out.x).toBe(VIEWPORT.width - SIZE.width);
    expect(out.y).toBe(0);
  });
});

describe("edgeDistances", () => {
  test("computes per-edge distances", () => {
    // GIVEN a panel near the bottom-right
    const pos = { x: 880, y: 240 };
    // WHEN distances are computed
    const d = edgeDistances(pos, SIZE, VIEWPORT);
    // THEN they match the manual calculation
    expect(d).toEqual({
      left: 880,
      right: VIEWPORT.width - (880 + SIZE.width),
      top: 240,
      bottom: VIEWPORT.height - (240 + SIZE.height),
    });
  });
});

describe("edgeGuides", () => {
  test("flags edges within the guide threshold", () => {
    // GIVEN distances close to left + bottom edges
    const guides = edgeGuides({ left: 10, right: 100, top: 200, bottom: 5 });
    // THEN only those edges are flagged
    expect(guides).toEqual({
      left: true,
      right: false,
      top: false,
      bottom: true,
    });
  });

  test("ignores edges exactly at the threshold", () => {
    // GIVEN distance equal to the guide threshold
    const guides = edgeGuides({
      left: SNAP_GUIDE_DISTANCE_PX,
      right: 100,
      top: 100,
      bottom: 100,
    });
    // THEN it is NOT flagged (strict <)
    expect(guides.left).toBe(false);
  });
});

describe("snapToEdges", () => {
  test("snaps to left edge when within trigger distance", () => {
    // GIVEN a position within the snap trigger distance
    const out = snapToEdges(
      { x: SNAP_TRIGGER_DISTANCE_PX - 5, y: 100 },
      SIZE,
      VIEWPORT,
    );
    // THEN the panel snaps to x=0
    expect(out.snapped).toBe(true);
    expect(out.position.x).toBe(0);
  });

  test("snaps to bottom edge when within trigger distance", () => {
    // GIVEN bottom distance within trigger
    const y = VIEWPORT.height - SIZE.height - (SNAP_TRIGGER_DISTANCE_PX - 5);
    const out = snapToEdges({ x: 200, y }, SIZE, VIEWPORT);
    // THEN snaps to bottom edge
    expect(out.snapped).toBe(true);
    expect(out.position.y).toBe(VIEWPORT.height - SIZE.height);
  });

  test("does not snap when far from any edge", () => {
    // GIVEN a centered position
    const out = snapToEdges({ x: 500, y: 200 }, SIZE, VIEWPORT);
    // THEN no snap occurs
    expect(out.snapped).toBe(false);
  });
});

describe("validateGeometry", () => {
  test("repairs an out-of-viewport persisted geometry", () => {
    // GIVEN persisted geometry with stale viewport coords
    const out = validateGeometry(
      { position: { x: 10_000, y: 10_000 }, size: { width: 100, height: 100 } },
      VIEWPORT,
    );
    // THEN size is clamped up to the minimum and position back into view
    expect(out.size).toEqual(COMPANION_MIN_SIZE);
    expect(out.position.x).toBeLessThanOrEqual(
      VIEWPORT.width - COMPANION_MIN_SIZE.width,
    );
    expect(out.position.y).toBeLessThanOrEqual(
      VIEWPORT.height - COMPANION_MIN_SIZE.height,
    );
  });
});
