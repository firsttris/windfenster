/** What a vertical swipe on a sheet does: far or fast enough up opens it, down folds it, and
 * down on a folded one closes it. `dy` in pixels (down positive), `velocity` in px/ms. */
export type SwipeAction = 'expand' | 'collapse' | 'close' | 'stay';

const DISTANCE = 40;
const SPEED = 0.5;

export function swipeAction(dy: number, velocity: number, expanded: boolean): SwipeAction {
  const up = dy < -DISTANCE || velocity < -SPEED;
  const down = dy > DISTANCE || velocity > SPEED;
  if (up) return expanded ? 'stay' : 'expand';
  if (down) return expanded ? 'collapse' : 'close';
  return 'stay';
}
