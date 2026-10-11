/** The colour field: a MapLibre custom layer that paints the model's wind speed under the
 * coastlines and labels. It draws only when the map or the hour changes. */
import type { CustomLayerInterface, Map as MapLibreMap } from 'maplibre-gl';
import { mercatorX, mercatorY, RAMP_MAX_KN, rampPixels } from '@/lib/wind-map';
import { activate, bindAttribute, bindTexture, createBuffer, createProgram, createTexture, type Program } from './gl';
import { FIELD_FRAG, FIELD_VERT } from './shaders';
import { bindWind, framesOf, hasWind, NO_WIND, type WindLayers, type WindTextures, windTextures } from './wind-gl';

export interface WindField {
  layer: CustomLayerInterface;
  /** Shows the layers' hours blended, or nothing. */
  show(layers: WindLayers): void;
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
  let shown: WindLayers = NO_WIND;
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
      textures = windTextures(gl);
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
      if (!hasWind(shown) || !program || !ramp || !textures) return;
      const g = context;
      // Two triangles over all the textures' extent, in Mercator
      const metas = framesOf(shown).map((f) => f.meta);
      const west = Math.min(...metas.map((m) => m.west));
      const east = Math.max(...metas.map((m) => m.east));
      const south = Math.min(...metas.map((m) => m.south));
      const north = Math.max(...metas.map((m) => m.north));
      const key = `${west},${south},${east},${north}`;
      if (quadFor !== key) {
        const [x0, x1] = [mercatorX(west), mercatorX(east)];
        const [y0, y1] = [mercatorY(north), mercatorY(south)];
        g.deleteBuffer(quad);
        quad = createBuffer(g, new Float32Array([x0, y0, x1, y0, x0, y1, x0, y1, x1, y0, x1, y1]));
        quadFor = key;
      }
      textures.keep(shown);
      activate(g, program);
      g.bindVertexArray(vao);
      bindAttribute(g, quad as WebGLBuffer, program.a.a_merc, 2);
      g.uniformMatrix4fv(program.u.u_matrix, false, Float32Array.from(options.defaultProjectionData.mainMatrix));
      bindWind(g, program, textures, shown, 0);
      bindTexture(g, ramp, 4);
      g.uniform1i(program.u.u_ramp, 4);
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
    show(layers) {
      shown = layers;
      map?.triggerRepaint();
    },
    setOpacity(o) {
      alpha = o;
      map?.triggerRepaint();
    },
  };
}
