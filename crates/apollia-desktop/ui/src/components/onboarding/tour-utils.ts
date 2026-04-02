/** Placement direction of the step card relative to the target element. */
export type CardPlacement = 'bottom' | 'right' | 'top' | 'left';

/** Computed fixed position for the step card. */
export interface CardPosition {
  top: number;
  left: number;
  placement: CardPlacement;
}

/** Screen viewport dimensions in pixels. */
export interface ViewportSize {
  width: number;
  height: number;
}

/** Step card dimensions in pixels. */
export interface CardSize {
  width: number;
  height: number;
}

/**
 * Computes the optimal fixed position for a step card next to a target element.
 *
 * Placement priority: bottom → right → top → left.
 * The result is clamped so the card never overflows the viewport edges.
 *
 * @param targetRect - Bounding rect of the highlighted element.
 * @param viewport   - Current viewport dimensions.
 * @param cardSize   - Dimensions of the step card.
 * @param gap        - Space in pixels between the target and the card.
 */
export function calculateCardPosition(
  targetRect: Pick<DOMRect, 'top' | 'left' | 'width' | 'height' | 'bottom' | 'right'>,
  viewport: ViewportSize,
  cardSize: CardSize,
  gap = 8,
): CardPosition {
  const edge = 16;

  const spaceBelow = viewport.height - targetRect.bottom;
  const spaceRight = viewport.width - targetRect.right;
  const spaceAbove = targetRect.top;

  if (spaceBelow >= cardSize.height + gap) {
    return {
      top: targetRect.bottom + gap,
      left: clamp(
        targetRect.left + targetRect.width / 2 - cardSize.width / 2,
        edge,
        viewport.width - cardSize.width - edge,
      ),
      placement: 'bottom',
    };
  }

  if (spaceRight >= cardSize.width + gap) {
    return {
      top: clamp(
        targetRect.top + targetRect.height / 2 - cardSize.height / 2,
        edge,
        viewport.height - cardSize.height - edge,
      ),
      left: targetRect.right + gap,
      placement: 'right',
    };
  }

  if (spaceAbove >= cardSize.height + gap) {
    return {
      top: targetRect.top - cardSize.height - gap,
      left: clamp(
        targetRect.left + targetRect.width / 2 - cardSize.width / 2,
        edge,
        viewport.width - cardSize.width - edge,
      ),
      placement: 'top',
    };
  }

  return {
    top: clamp(
      targetRect.top + targetRect.height / 2 - cardSize.height / 2,
      edge,
      viewport.height - cardSize.height - edge,
    ),
    left: Math.max(edge, targetRect.left - cardSize.width - gap),
    placement: 'left',
  };
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}
