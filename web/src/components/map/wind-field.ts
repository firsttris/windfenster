/** The colour field: a MapLibre custom layer that paints the model's wind speed under the
 * coastlines and labels. It draws only when the map or the hour changes. */
import type { CustomLayerInterface, Map as MapLibreMap } from 'maplibre-gl';
import { mercatorX, mercatorY, RAMP_MAX_KN, rampPixels } from '@/lib/wind-map';
import { activate, bindAttribute, bindTexture, createBuffer, createProgram, createTexture, type Program } from './gl';
import { FIELD_FRAG, FIELD_VERT } from './shaders';
import { bindWind, type WindPair, type WindTextures, windTextures } from './wind-gl';

export interface WindField {
  layer: CustomLayerInterface;
  /** Shows two hours blended, or nothing. */
  show(pair: WindPair | null): void;
  setOpacity(opacity: number): void;
}

export function windField(id: string, opacity: number): WindField {
  let map: MapLibreMap | null = null;
  let gl: WebGL2RenderingContext | null = null;
  let program: Program | null = null;
  let ramp: WebGLTexture | null = null;
  let quad: WebGLBuffer | null = null;
  let quadFor = '';
  // Our own vertex array, so MapLibre's stay untouched
  let vao: WebGLVertexArrayObject | null = null;
  let textures: WindTextures | null = null;
  let pair: WindPair | null = null;
  let alpha = opacity;

  const layer: CustomLayerInterface = {
    id,
    type: 'custom',
    renderingMode: '2d',
    onAdd(m, context) {
      map = m;
      gl = context;
      program = createProgram(gl, FIELD_VERT, FIELD_FRAG);
      ramp = createTexture(gl, gl.LINEAR, rampPixels(), 256, 1);
      textures = windTextures(gl, true);
      vao = gl.createVertexArray();
    },
    onRemove() {
      textures?.clear();
      if (gl) {
        gl.deleteProgram(program?.program ?? null);
        gl.deleteTexture(ramp);
        gl.deleteBuffer(quad);
        gl.deleteVertexArray(vao);
      }
      map = gl = program = ramp = quad = textures = vao = null;
      quadFor = '';
    },
    render(context, options) {
      if (!pair || !program || !ramp || !textures) return;
      const g = context;
      const meta = pair.a.meta;
      // Two triangles over the texture's extent, in Mercator
      const key = `${meta.west},${meta.south},${meta.east},${meta.north}`;
      if (quadFor !== key) {
        const [x0, x1] = [mercatorX(meta.west), mercatorX(meta.east)];
        const [y0, y1] = [mercatorY(meta.north), mercatorY(meta.south)];
        g.deleteBuffer(quad);
        quad = createBuffer(g, new Float32Array([x0, y0, x1, y0, x0, y1, x0, y1, x1, y0, x1, y1]));
        quadFor = key;
      }
      textures.keep(pair);
      activate(g, program);
      g.bindVertexArray(vao);
      bindAttribute(g, quad as WebGLBuffer, program.a.a_merc, 2);
      g.uniformMatrix4fv(program.u.u_matrix, false, Float32Array.from(options.defaultProjectionData.mainMatrix));
      bindWind(g, program, textures, pair, 0);
      bindTexture(g, ramp, 2);
      g.uniform1i(program.u.u_ramp, 2);
      g.uniform1f(program.u.u_ramp_max, RAMP_MAX_KN);
      g.uniform1f(program.u.u_opacity, alpha);
      g.enable(g.BLEND);
      g.blendFunc(g.ONE, g.ONE_MINUS_SRC_ALPHA);
      g.drawArrays(g.TRIANGLES, 0, 6);
      g.bindVertexArray(null);
    },
  };

  return {
    layer,
    show(p) {
      pair = p;
      map?.triggerRepaint();
    },
    setOpacity(o) {
      alpha = o;
      map?.triggerRepaint();
    },
  };
}
