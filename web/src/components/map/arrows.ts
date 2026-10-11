/** For "reduce motion": a calm grid of arrows instead of the moving particles, read from the
 * same two hours on the CPU and drawn again when the view or the hour changes. */
import { latOf, type MercatorView, speedAndDirection } from '@/lib/wind-map';
import type { Particles } from './particles';
import { windAt } from './wind-at';
import { hasWind, NO_WIND, type WindLayers } from './wind-gl';

/** CSS pixels between arrows */
const SPACING = 44;

export function arrowField(canvas: HTMLCanvasElement): Particles | null {
  const ctx = canvas.getContext('2d');
  if (!ctx) return null;
  let shown: WindLayers = NO_WIND;
  let view: MercatorView = { x: 0, y: 0, w: 1, h: 1 };
  let size = { w: 1, h: 1, ratio: 1 };
  let frame = 0;

  function draw() {
    frame = 0;
    if (!ctx) return;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    if (!hasWind(shown)) return;
    ctx.setTransform(size.ratio, 0, 0, size.ratio, 0, 0);
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    for (let y = SPACING / 2; y < size.h; y += SPACING) {
      const lat = latOf(view.y + (y / size.h) * view.h);
      for (let x = SPACING / 2; x < size.w; x += SPACING) {
        const lon = (view.x + (x / size.w) * view.w) * 360 - 180;
        const at = windAt(shown, lon, lat);
        if (!at) continue;
        const { kn, from } = speedAndDirection(at.wind);
        if (kn < 1) continue;
        // Pointing where the wind blows, longer the stronger it is
        const len = Math.min(18, 7 + kn * 0.45);
        const rad = ((from + 180) * Math.PI) / 180;
        const dx = Math.sin(rad) * len;
        const dy = -Math.cos(rad) * len;
        for (const [style, width] of [
          ['rgba(9,9,11,0.55)', 3.2],
          ['rgba(255,255,255,0.92)', 1.4],
        ] as const) {
          ctx.strokeStyle = style;
          ctx.lineWidth = width;
          ctx.beginPath();
          ctx.moveTo(x - dx / 2, y - dy / 2);
          ctx.lineTo(x + dx / 2, y + dy / 2);
          // The head: two short strokes back from the tip
          for (const side of [-1, 1]) {
            const back = rad + Math.PI + side * 0.5;
            ctx.moveTo(x + dx / 2, y + dy / 2);
            ctx.lineTo(x + dx / 2 + Math.sin(back) * 5, y + dy / 2 - Math.cos(back) * 5);
          }
          ctx.stroke();
        }
      }
    }
  }

  const later = () => {
    if (!frame) frame = requestAnimationFrame(draw);
  };

  return {
    show(layers) {
      shown = layers;
      later();
    },
    setView(next, cssWidth, cssHeight, pixelRatio) {
      const width = Math.round(cssWidth * pixelRatio);
      const height = Math.round(cssHeight * pixelRatio);
      if (width !== canvas.width || height !== canvas.height) {
        canvas.width = width;
        canvas.height = height;
      }
      view = next;
      size = { w: cssWidth, h: cssHeight, ratio: pixelRatio };
      later();
    },
    setTheme() {},
    setGusts() {},
    destroy() {
      cancelAnimationFrame(frame);
    },
  };
}
