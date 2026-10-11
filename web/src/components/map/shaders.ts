/**
 * GLSL of the wind layers (WebGL2). The texture format and the particle state encoding follow
 * mapbox/webgl-wind (ISC, src/shaders): red U and green V scaled to the file's range, particle
 * positions in two bytes per axis.
 */

/** The wind at a Mercator position: (u, v) in kn and how much of the spot lies in a model's
 * domain. Two models, each two hours blended: the base (ECMWF, the whole region) and the detail
 * (ICON-D2, its domain), which wins inside its domain and fades into the base over the last
 * 30–60 km to its edge (the edge read from a coarse mip level). Textures hold texel edges in
 * `bounds` (west, south, east, north) and value ranges in `range` (uMin, uMax, vMin, vMax).
 * The colour field samples with mipmaps; the particles read the full texture (`SAMPLE`). */
const WIND = `
uniform sampler2D u_base0;
uniform sampler2D u_base1;
uniform sampler2D u_detail0;
uniform sampler2D u_detail1;
uniform vec4 u_base_bounds0;
uniform vec4 u_base_bounds1;
uniform vec4 u_base_range0;
uniform vec4 u_base_range1;
uniform vec4 u_detail_bounds0;
uniform vec4 u_detail_bounds1;
uniform vec4 u_detail_range0;
uniform vec4 u_detail_range1;
uniform vec2 u_mix;   // how far between the two hours: base, detail
uniform vec2 u_has;   // 1 where a model is shown: base, detail

const float PI = 3.141592653589793;
const float EDGE_LOD = 4.0;

vec2 lonLat(vec2 merc) {
  return vec2(merc.x * 360.0 - 180.0, degrees(atan(sinh(PI * (1.0 - 2.0 * merc.y)))));
}

vec2 uvOf(vec4 b, vec2 ll) {
  return vec2((ll.x - b.x) / (b.z - b.x), (b.w - ll.y) / (b.w - b.y));
}

float insideOf(vec2 uv) {
  return step(0.0, uv.x) * step(uv.x, 1.0) * step(0.0, uv.y) * step(uv.y, 1.0);
}

vec3 windOf(sampler2D tex, vec4 b, vec4 r, vec2 ll) {
  vec2 uv = uvOf(b, ll);
  // Sampled before any branch, so the field's mipmaps see smooth derivatives
  vec4 c = SAMPLE(tex, clamp(uv, 0.0, 1.0));
  // Texels outside the domain are (0, 0, 0, 0): dividing by alpha leaves the valid ones.
  vec2 n = c.rg / max(c.a, 0.004);
  return vec3(mix(r.x, r.y, n.x), mix(r.z, r.w, n.y), c.a * insideOf(uv));
}

vec3 wind(vec2 merc) {
  vec2 ll = lonLat(merc);
  vec3 base = u_has.x * mix(
    windOf(u_base0, u_base_bounds0, u_base_range0, ll),
    windOf(u_base1, u_base_bounds1, u_base_range1, ll), u_mix.x);
  vec3 detail = u_has.y * mix(
    windOf(u_detail0, u_detail_bounds0, u_detail_range0, ll),
    windOf(u_detail1, u_detail_bounds1, u_detail_range1, ll), u_mix.y);
  vec2 duv = uvOf(u_detail_bounds0, ll);
  float edge = textureLod(u_detail0, clamp(duv, 0.0, 1.0), EDGE_LOD).a * insideOf(duv);
  float w = step(0.5, detail.z) * (base.z < 0.5 ? 1.0 : smoothstep(0.6, 0.98, edge));
  return vec3(mix(base.xy, detail.xy, w), max(base.z, detail.z));
}
`;

/** The colour field: a quad over the texture in Mercator, coloured by speed. */
export const FIELD_VERT = `#version 300 es
uniform mat4 u_matrix;
in vec2 a_merc;
out vec2 v_merc;
void main() {
  v_merc = a_merc;
  gl_Position = u_matrix * vec4(a_merc, 0.0, 1.0);
}`;

export const FIELD_FRAG = `#version 300 es
precision highp float;
// One mip level softer than the screen asks for: 2 km detail reads as noise from afar.
#define SAMPLE(t, uv) texture(t, uv, 1.0)
${WIND}
uniform sampler2D u_ramp;
uniform float u_ramp_max;
uniform float u_opacity;
in vec2 v_merc;
out vec4 color;
void main() {
  vec3 w = wind(v_merc);
  float a = u_opacity * smoothstep(0.3, 0.7, w.z);
  if (a <= 0.0) discard;
  vec3 c = texture(u_ramp, vec2(clamp(length(w.xy) / u_ramp_max, 0.0, 1.0), 0.5)).rgb;
  // MapLibre blends premultiplied colours.
  color = vec4(c * a, a);
}`;

/** A quad over the whole target, for the particle update and the trails. */
export const QUAD_VERT = `#version 300 es
in vec2 a_pos;
out vec2 v_pos;
void main() {
  v_pos = a_pos;
  gl_Position = vec4(2.0 * a_pos - 1.0, 0.0, 1.0);
}`;

/** Particle positions are kept relative to the visible map (0…1 from the top left), two
 * bytes per axis: red and blue for x, green and alpha for y. */
const STATE = `
vec2 decode(vec4 c) {
  return vec2(c.r / 255.0 + c.b, c.g / 255.0 + c.a);
}
vec4 encode(vec2 p) {
  return vec4(fract(p * 255.0), floor(p * 255.0) / 255.0);
}
`;

/** Moves every particle along the wind, `u_px_per_kn` screen pixels per knot and frame;
 * particles outside the domain or the view, and a few at random, start anew elsewhere. */
export const UPDATE_FRAG = `#version 300 es
precision highp float;
#define SAMPLE(t, uv) textureLod(t, uv, 0.0)
${WIND}
${STATE}
uniform sampler2D u_particles;
uniform vec4 u_view;      // visible Mercator rectangle: x, y, width, height
uniform vec4 u_shift;     // the view moved: new = old * xy + zw
uniform vec2 u_screen;    // view size in pixels
uniform float u_px_per_kn;
uniform float u_seed;
uniform float u_drop;
in vec2 v_pos;
out vec4 color;

float rand(vec2 co) {
  float t = dot(vec2(12.9898, 78.233), co);
  return fract(sin(t) * (4375.85453 + t));
}

void main() {
  vec2 p = decode(texelFetch(u_particles, ivec2(gl_FragCoord.xy), 0)) * u_shift.xy + u_shift.zw;
  vec2 seed = (p + v_pos) * u_seed;
  vec3 w = wind(u_view.xy + p * u_view.zw);
  float speed = length(w.xy);
  vec2 next = p + vec2(w.x, -w.y) * u_px_per_kn / u_screen;
  bool lost = w.z < 0.5 || any(lessThan(next, vec2(0.0))) || any(greaterThan(next, vec2(1.0)));
  // Calm particles would only pile up; strong ones are renewed a bit more often.
  float drop = u_drop + u_drop * 3.0 * smoothstep(0.0, 30.0, speed);
  if (lost || rand(seed) > 1.0 - drop) {
    next = vec2(rand(seed + 1.3), rand(seed + 2.1));
  }
  color = encode(clamp(next, 0.0, 1.0));
}`;

/** One quad per particle from where it was to where it is, `u_width` pixels wide. */
export const DRAW_VERT = `#version 300 es
precision highp float;
#define SAMPLE(t, uv) textureLod(t, uv, 0.0)
${WIND}
${STATE}
uniform sampler2D u_before;
uniform sampler2D u_after;
uniform vec4 u_shift;
uniform vec4 u_view;
uniform vec2 u_screen;
uniform float u_res;
uniform float u_width;
in vec2 a_corner;         // x: 0 at the tail, 1 at the head; y: -1 or 1 across
out float v_alpha;

void main() {
  int i = gl_InstanceID;
  ivec2 at = ivec2(i % int(u_res), i / int(u_res));
  vec2 b = decode(texelFetch(u_after, at, 0));
  vec2 a = decode(texelFetch(u_before, at, 0)) * u_shift.xy + u_shift.zw;
  vec2 pb = b * u_screen;
  vec2 pa = a * u_screen;
  vec3 w = wind(u_view.xy + b * u_view.zw);
  // A particle that started anew has no tail.
  if (distance(pa, pb) > 40.0 || w.z < 0.5) pa = pb;
  vec2 d = pb - pa;
  float len = length(d);
  vec2 dir = len > 0.001 ? d / len : vec2(1.0, 0.0);
  vec2 across = vec2(-dir.y, dir.x);
  float half_w = u_width * 0.5;
  vec2 p = mix(pa - dir * half_w, pb + dir * half_w, a_corner.x) + across * half_w * a_corner.y;
  v_alpha = w.z < 0.5 ? 0.0 : mix(0.3, 0.85, smoothstep(2.0, 25.0, length(w.xy)));
  gl_Position = vec4(p / u_screen * 2.0 - 1.0, 0.0, 1.0);
  gl_Position.y = -gl_Position.y;
}`;

export const DRAW_FRAG = `#version 300 es
precision mediump float;
uniform vec3 u_color;
in float v_alpha;
out vec4 color;
void main() {
  color = vec4(u_color * v_alpha, v_alpha);
}`;

/** The trails of the last frame, a little fainter: what is left fades out over time. */
export const FADE_FRAG = `#version 300 es
precision mediump float;
uniform sampler2D u_screen_tex;
uniform float u_fade;
in vec2 v_pos;
out vec4 color;
void main() {
  vec4 c = texture(u_screen_tex, v_pos);
  // Rounding down makes sure faint trails reach zero.
  color = floor(255.0 * c * u_fade) / 255.0;
}`;

/** Copies a texture onto the canvas. */
export const COPY_FRAG = `#version 300 es
precision mediump float;
uniform sampler2D u_screen_tex;
in vec2 v_pos;
out vec4 color;
void main() {
  color = texture(u_screen_tex, v_pos);
}`;
