/** Dragging a sheet up and down by its top: follows the finger, then raises or lowers it a step
 * or closes it (lib/swipe.ts). Pointer events, so mouse and touch alike; the dragged area does not scroll the
 * page (touch-action: none). */
import { type PointerEvent, useRef, useState } from 'react';
import { swipeAction } from '@/lib/swipe';

export function useSheetDrag(step: number, top: number, onStep: (s: number) => void, onClose: () => void) {
  const start = useRef<{ y: number; t: number; id: number } | null>(null);
  const [dy, setDy] = useState(0);
  const moved = useRef(false);

  const handlers = {
    onPointerDown: (e: PointerEvent<HTMLElement>) => {
      start.current = { y: e.clientY, t: e.timeStamp, id: e.pointerId };
      moved.current = false;
      // The grip holds the pointer at once (a mouse leaves it with the first move up); other
      // areas only once dragging, so their buttons still get their clicks.
      if (e.currentTarget.dataset.grip) e.currentTarget.setPointerCapture(e.pointerId);
    },
    onPointerMove: (e: PointerEvent<HTMLElement>) => {
      const s = start.current;
      if (!s || s.id !== e.pointerId) return;
      const d = e.clientY - s.y;
      if (Math.abs(d) > 4 && !moved.current) {
        moved.current = true;
        e.currentTarget.setPointerCapture(e.pointerId);
      }
      // Down follows the finger, up only a little: the sheet grows instead
      if (moved.current) setDy(d > 0 ? d : Math.max(-24, d / 3));
    },
    onPointerUp: (e: PointerEvent<HTMLElement>) => {
      const s = start.current;
      start.current = null;
      setDy(0);
      if (!s || !moved.current) return;
      const d = e.clientY - s.y;
      const action = swipeAction(d, d / Math.max(1, e.timeStamp - s.t), step, top);
      if (action === 'up') onStep(step + 1);
      if (action === 'down') onStep(step - 1);
      if (action === 'close') onClose();
    },
    onPointerCancel: () => {
      start.current = null;
      setDy(0);
    },
  };
  /** Whether the last press was a drag, so a tap handler can ignore it */
  const dragged = () => moved.current;
  return { dy, handlers, dragged };
}
