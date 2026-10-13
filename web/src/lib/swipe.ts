/** What a vertical swipe on a sheet does: far or fast enough up raises it a step, down lowers
 * it a step, and down on the lowest step closes it. `dy` in pixels (down positive), `velocity`
 * in px/ms; `step` is the sheet's step now, from 0 (lowest) to `top`. */
export type SwipeAction = 'up' | 'down' | 'close' | 'stay';

const DISTANCE = 40;
const SPEED = 0.5;

export function swipeAction(dy: number, velocity: number, step: number, top: number): SwipeAction {
  const up = dy < -DISTANCE || velocity < -SPEED;
  const down = dy > DISTANCE || velocity > SPEED;
  if (up) return step < top ? 'up' : 'stay';
  if (down) return step > 0 ? 'down' : 'close';
  return 'stay';
}
