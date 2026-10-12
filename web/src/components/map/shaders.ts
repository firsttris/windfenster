/**
 * GLSL of the wind layers (WebGL2). The texture format and the particle state encoding follow
 * mapbox/webgl-wind (ISC, src/shaders): red U and green V scaled to the file's range, particle
 * positions in two bytes per axis.
 */

/** Full precision for numbers and for what textures give back, in every shader. WebGL reads
 * textures at low precision unless told otherwise; ANGLE on OpenGL ES drivers that honour that
 * (Mesa radeonsi under Chrome and Brave) then decodes them in 16 bits, too coarse for particle
 * positions kept in two bytes per axis. Seen on an AMD card: no particles in Chrome and Brave,
 * while Firefox (desktop OpenGL, which ignores precision) and the tests' SwiftShader drew them. */
export const PRECISION = `precision highp float;
precision highp int;
precision highp sampler2D;`;

/** The uniforms of one layer: two hours (`0`, `1`), each with its texture, texel edges (west,
 * south, east, north) and value ranges (uMin, uMax, vMin, vMax), and the gust at blue 255 per
 * hour (negative when the hour has none). */
const layerUniforms = (name: string) => `
uniform sampler2D u_${name}0;
uniform sampler2D u_${name}1;
uniform vec4 u_${name}_bounds0;
uniform vec4 u_${name}_bounds1;
uniform vec4 u_${name}_range0;
uniform vec4 u_${name}_range1;
uniform vec2 u_${name}_gust;`;

/** The layer's wind at `ll`, its two hours blended; zero where it is not shown */
const layerWind = (name: string, k: string) =>
  `u_has.${k} * mixWind(windOf(u_${name}0, u_${name}_bounds0, u_${name}_range0, u_${name}_gust.x, ll), ` +
  `windOf(u_${name}1, u_${name}_bounds1, u_${name}_range1, u_${name}_gust.y, ll), u_mix.${k})`;

/** The wind at a Mercator position: (u, v) and the gust in kn, and how much of the spot lies in
 * a model's domain. Up to three models, each two hours blended, finest on top: the base (ECMWF,
 * the whole world); over it a finer global model (ICON global) in the piece the app has around
 * the view, fading out over the piece's outer 4 %; and over both the detail (ICON-D2, its
 * domain), which fades into what lies under it over the last 30–60 km to its edge (the edge read
 * from a coarse mip level). A texture 360° wide (the global base) repeats east and west, so
 * MapLibre's world copies show it and the seam at 180° is sampled across; a regional one is
 * placed on the copy its longitudes fall on. The colour field samples with mipmaps; the
 * particles read the full texture (`SAMPLE`). */
export const WIND = `
${layerUniforms('base')}
${layerUniforms('mid')}
${layerUniforms('detail')}
uniform vec3 u_mix;   // how far between the two hours: base, mid, detail
uniform vec3 u_has;   // 1 where a model is shown: base, mid, detail

const float PI = 3.141592653589793;
const float EDGE_LOD = 4.0;

/** Longitude (beyond ±180° on world copies) and latitude */
vec2 lonLat(vec2 merc) {
  return vec2(merc.x * 360.0 - 180.0, degrees(atan(sinh(PI * (1.0 - 2.0 * merc.y)))));
}

/** 1 for a texture that goes once around the world */
float wrapsOf(vec4 b) {
  return step(359.0, b.z - b.x);
}

/** Texture coordinates; around the world unwrapped (the texture repeats, and the mipmaps see no
 * jump at 180°), else on the copy the place falls on. */
vec2 uvOf(vec4 b, vec2 ll) {
  float lon = wrapsOf(b) > 0.5 ? ll.x : ll.x - 360.0 * floor((ll.x + 180.0) / 360.0);
  return vec2((lon - b.x) / (b.z - b.x), (b.w - ll.y) / (b.w - b.y));
}

float insideOf(vec4 b, vec2 uv) {
  float x = max(wrapsOf(b), step(0.0, uv.x) * step(uv.x, 1.0));
  return x * step(0.0, uv.y) * step(uv.y, 1.0);
}

vec2 clampOf(vec4 b, vec2 uv) {
  return vec2(wrapsOf(b) > 0.5 ? uv.x : clamp(uv.x, 0.0, 1.0), clamp(uv.y, 0.0, 1.0));
}

vec4 windOf(sampler2D tex, vec4 b, vec4 r, float g, vec2 ll) {
  vec2 uv = uvOf(b, ll);
  // Sampled before any branch, so the field's mipmaps see smooth derivatives
  vec4 c = SAMPLE(tex, clampOf(b, uv));
  // Texels outside the domain are (0, 0, 0, 0): dividing by alpha leaves the valid ones.
  vec3 n = c.rgb / max(c.a, 0.004);
  // No gust outside the texture: the clamped edge texel's would otherwise stand in for a hour
  // below that has none (the first hour of an ECMWF run), hundreds of km from that edge
  float inside = insideOf(b, uv);
  float gust = g < 0.0 || inside < 0.5 ? -1.0 : n.z * g;
  return vec4(mix(r.x, r.y, n.x), mix(r.z, r.w, n.y), gust, c.a * inside);
}

/** a and b mixed; a gust missing on one side (negative) is taken from the other. */
vec4 mixWind(vec4 a, vec4 b, float t) {
  vec4 m = mix(a, b, t);
  m.z = mix(a.z < 0.0 ? b.z : a.z, b.z < 0.0 ? a.z : b.z, t);
  return m;
}

/** How far inside a piece: 0 at its edges, 1 from 4 % of its size inwards (along the latitude
 * only for a texture around the world). */
float innerOf(vec4 b, vec2 uv) {
  vec2 d = min(uv, 1.0 - uv);
  return smoothstep(0.0, 0.04, wrapsOf(b) > 0.5 ? d.y : min(d.x, d.y)) * insideOf(b, uv);
}

/** One layer over another by weight w (where the upper has wind); how much of the spot lies in
 * either */
vec4 layered(vec4 under, vec4 over, float w) {
  vec4 m = mixWind(under, over, step(0.5, over.w) * (under.w < 0.5 ? 1.0 : w));
  m.w = max(under.w, over.w);
  return m;
}

vec4 wind(vec2 merc) {
  vec2 ll = lonLat(merc);
  vec4 base = ${layerWind('base', 'x')};
  vec4 mid = ${layerWind('mid', 'y')};
  vec4 detail = ${layerWind('detail', 'z')};
  vec4 under = layered(base, mid, innerOf(u_mid_bounds0, uvOf(u_mid_bounds0, ll)));
  vec2 duv = uvOf(u_detail_bounds0, ll);
  float edge = textureLod(u_detail0, clampOf(u_detail_bounds0, duv), EDGE_LOD).a * insideOf(u_detail_bounds0, duv);
  vec4 m = layered(under, detail, smoothstep(0.6, 0.98, edge));
  // Never a gust below the wind: both are interpolated apart, and hours without gusts show wind.
  m.z = max(m.z, length(m.xy));
  return m;
}
`;

/** The colour field: a quad over the texture in Mercator, coloured by speed. */
export const FIELD_VERT = `#version 300 es
${PRECISION}
uniform mat4 u_matrix;
in vec2 a_merc;
out vec2 v_merc;
void main() {
  v_merc = a_merc;
  gl_Position = u_matrix * vec4(a_merc, 0.0, 1.0);
}`;

export const FIELD_FRAG = `#version 300 es
${PRECISION}
// One mip level softer than the screen asks for: 2 km detail reads as noise from afar.
#define SAMPLE(t, uv) texture(t, uv, 1.0)
${WIND}
uniform sampler2D u_ramp;
uniform float u_ramp_max;
uniform float u_opacity;
uniform float u_gusts;    // 1: colour by the gusts instead of the wind
in vec2 v_merc;
out vec4 color;
void main() {
  vec4 w = wind(v_merc);
  float a = u_opacity * smoothstep(0.3, 0.7, w.w);
  if (a <= 0.0) discard;
  float kn = mix(length(w.xy), w.z, u_gusts);
  vec3 c = texture(u_ramp, vec2(clamp(kn / u_ramp_max, 0.0, 1.0), 0.5)).rgb;
  // MapLibre blends premultiplied colours.
  color = vec4(c * a, a);
}`;

/** A quad over the whole target, for the particle update and the trails. */
export const QUAD_VERT = `#version 300 es
${PRECISION}
in vec2 a_pos;
out vec2 v_pos;
void main() {
  v_pos = a_pos;
  gl_Position = vec4(2.0 * a_pos - 1.0, 0.0, 1.0);
}`;

/** How fast particles run, in knots' worth of the screen speed: as the wind from about 5 kn
 * up, faster below, so light air still flows in long lines instead of crawling as specks (the
 * colour and the readout keep the true speed); below 1 kn, calm, they slow to a stop, so the
 * texture's rounding never sets still air drifting. */
const PACE = `
float pace(float kn) {
  return max(kn, 2.2 * sqrt(kn)) * smoothstep(0.3, 1.0, kn);
}
`;

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

/** Moves every particle along the wind, `u_px_per_kn` screen pixels per knot of `pace` and frame;
 * particles outside the domain or the view, and a few at random, start anew elsewhere. When the
 * map moved (`u_shift`), the particles move with it and the view stays evenly filled: after a
 * zoom out the old view covers only part of the new one, so as many of its particles as it now
 * holds too many start anew, and these and the ones a pan pushed out go where the old view was
 * not (the strip a pan uncovers, the border a zoom out adds), sampled there directly. */
export const UPDATE_FRAG = `#version 300 es
${PRECISION}
#define SAMPLE(t, uv) textureLod(t, uv, 0.0)
${WIND}
${STATE}
${PACE}
uniform sampler2D u_particles;
uniform vec4 u_view;      // visible Mercator rectangle: x, y, width, height
uniform vec4 u_shift;     // the view moved: new = old * xy + zw
uniform vec2 u_screen;    // view size in pixels
uniform float u_px_per_kn;
uniform uint u_frame;
uniform float u_drop;
in vec2 v_pos;
out vec4 color;

// An integer hash (PCG, Jarzynski and Olano 2020): every draw is independent of the others. The
// sine hash of webgl-wind gave x and y from one number, so new particles gathered on a few
// points and their first strokes formed stars there.
uint pcg(uint v) {
  uint s = v * 747796405u + 2891336453u;
  uint w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
  return (w >> 22u) ^ w;
}
float unit(uint h) {
  return float(h >> 8u) / 16777216.0;
}

/** A random place in what the last frame's view did not cover, chosen by area among the up to
 * four rectangles around it (left and right strips whole, bottom and top between them); sets
 * area to their sum (0 when the old view covers everything, as after a zoom in). Sampled
 * directly: a thin strip after a small pan is found every time, not by luck. */
vec2 uncovered(uint h, out float area) {
  vec2 lo = clamp(u_shift.zw, 0.0, 1.0);
  vec2 hi = clamp(u_shift.zw + u_shift.xy, 0.0, 1.0);
  float width = max(hi.x - lo.x, 0.0);
  vec4 a = vec4(lo.x, 1.0 - hi.x, width * lo.y, width * (1.0 - hi.y));
  area = a.x + a.y + a.z + a.w;
  float r = unit(h) * area;
  vec2 q = vec2(unit(pcg(h ^ 0x68e31da4u)), unit(pcg(h ^ 0xb5297a4du)));
  if (r < a.x) return vec2(q.x * lo.x, q.y);
  if (r < a.x + a.y) return vec2(hi.x + q.x * (1.0 - hi.x), q.y);
  if (r < a.x + a.y + a.z) return vec2(lo.x + q.x * width, q.y * lo.y);
  return vec2(lo.x + q.x * width, hi.y + q.y * (1.0 - hi.y));
}

void main() {
  vec2 p = decode(texelFetch(u_particles, ivec2(gl_FragCoord.xy), 0)) * u_shift.xy + u_shift.zw;
  uint h = pcg(uint(gl_FragCoord.y) * 65536u + uint(gl_FragCoord.x) ^ pcg(u_frame));
  vec4 w = wind(u_view.xy + p * u_view.zw);
  float speed = length(w.xy);
  vec2 dir = speed > 0.0 ? w.xy / speed : vec2(0.0);
  vec2 next = p + vec2(dir.x, -dir.y) * pace(speed) * u_px_per_kn / u_screen;
  bool lost = w.w < 0.5 || any(lessThan(next, vec2(0.0))) || any(greaterThan(next, vec2(1.0)));
  // Calm particles would only pile up; strong ones are renewed a bit more often.
  float drop = u_drop + u_drop * 3.0 * smoothstep(0.0, 30.0, speed);
  // The map moved it out of the view (a pan), or zoomed out the old view's particles crowd
  // into its smaller area (keep as many as it holds): these fill what the old view did not
  // cover, exactly as many as it lost
  bool shiftedOut = any(lessThan(p, vec2(0.0))) || any(greaterThan(p, vec2(1.0)));
  bool crowded = unit(pcg(h ^ 0x9e3779b9u)) < 1.0 - u_shift.x * u_shift.y;
  if (lost || crowded || unit(h) < drop) {
    uint hx = pcg(h);
    float area;
    vec2 fill = uncovered(hx, area);
    // Renewed by chance, or where nothing is uncovered (a zoom in): anywhere
    next = (shiftedOut || crowded) && area > 1e-6 ? fill : vec2(unit(hx), unit(pcg(hx)));
  }
  color = encode(clamp(next, 0.0, 1.0));
}`;

/** One quad per particle from where it was to where it is, `u_width` pixels wide plus a pixel of
 * soft edge on either side, so slanted lines are smooth instead of stair-stepped. */
export const DRAW_VERT = `#version 300 es
${PRECISION}
#define SAMPLE(t, uv) textureLod(t, uv, 0.0)
${WIND}
${STATE}
${PACE}
uniform sampler2D u_before;
uniform sampler2D u_after;
uniform vec4 u_shift;
uniform vec4 u_view;
uniform vec2 u_screen;
uniform float u_res;
uniform float u_width;
uniform float u_move;     // device pixels per knot this frame
uniform float u_gusts;    // 1: the colour follows the gusts
in vec2 a_corner;         // x: 0 at the tail, 1 at the head; y: -1 or 1 across
out float v_alpha;
out float v_kn;
out float v_across;       // pixels from the line's middle

void main() {
  int i = gl_InstanceID;
  ivec2 at = ivec2(i % int(u_res), i / int(u_res));
  vec2 b = decode(texelFetch(u_after, at, 0));
  vec2 a = decode(texelFetch(u_before, at, 0)) * u_shift.xy + u_shift.zw;
  vec2 pb = b * u_screen;
  vec2 pa = a * u_screen;
  vec4 w = wind(u_view.xy + b * u_view.zw);
  // A particle that started anew has no tail: it went further than the wind could take it.
  // (A fixed limit drew strokes up to that long to every new particle in a calm.)
  float speed = length(w.xy);
  float reach = (pace(speed) * 1.5 + 2.0) * u_move + 1.0;
  // Nor does it draw a dot: it would stay on the trails as a speck.
  bool shown = distance(pa, pb) <= reach && w.w >= 0.5;
  if (!shown) pa = pb;
  vec2 d = pb - pa;
  float len = length(d);
  vec2 dir = len > 0.001 ? d / len : vec2(1.0, 0.0);
  vec2 across = vec2(-dir.y, dir.x);
  // Segments of one trail meet end to end: overlapping ends would draw beads along it. Only a
  // particle that barely moved is stretched to a pixel.
  float ext = max(0.0, (u_width - len) * 0.5);
  float side = u_width * 0.5 + 1.0;
  vec2 p = mix(pa - dir * ext, pb + dir * ext, a_corner.x) + across * side * a_corner.y;
  v_across = side * a_corner.y;
  // Fainter in light wind, gone in calm, where particles would only stand as dots
  v_alpha = shown ? mix(0.3, 0.85, smoothstep(2.0, 25.0, speed)) * smoothstep(0.3, 1.0, speed) : 0.0;
  v_kn = mix(speed, w.z, u_gusts);
  gl_Position = vec4(p / u_screen * 2.0 - 1.0, 0.0, 1.0);
  gl_Position.y = -gl_Position.y;
}`;

/** White over the colour field; from town zoom on, where the field gives way to the map, in
 * the field's colours, a little stronger, so the wind's strength stays readable. */
export const DRAW_FRAG = `#version 300 es
${PRECISION}
uniform vec4 u_ink;       // the theme's ink and how far to lean towards it
uniform sampler2D u_ramp;
uniform float u_ramp_max;
uniform float u_town;
uniform float u_width;
in float v_alpha;
in float v_kn;
in float v_across;
out vec4 color;
void main() {
  // How much of this pixel the line covers across it
  float cover = clamp(u_width * 0.5 + 0.5 - abs(v_across), 0.0, 1.0);
  if (cover <= 0.0) discard;
  vec3 ramp = texture(u_ramp, vec2(clamp(v_kn / u_ramp_max, 0.0, 1.0), 0.5)).rgb;
  vec3 c = mix(vec3(1.0), mix(ramp, u_ink.rgb, u_ink.a), u_town);
  float a = min(1.0, v_alpha * (1.0 + 0.4 * u_town)) * cover;
  color = vec4(c * a, a);
}`;

/** The trails of the last frame, a little fainter: what is left fades out over time. When the
 * map moved they move with it (`u_shift` as in the update), so a pan or zoom keeps them where
 * the wind drew them instead of wiping them. */
export const FADE_FRAG = `#version 300 es
${PRECISION}
uniform sampler2D u_screen_tex;
uniform float u_fade;
uniform vec4 u_shift;     // the view moved: new = old * xy + zw, from the top left
in vec2 v_pos;
out vec4 color;
void main() {
  // The texture's rows go from the bottom, the view's places from the top.
  vec2 old = (vec2(v_pos.x, 1.0 - v_pos.y) - u_shift.zw) / u_shift.xy;
  if (any(lessThan(old, vec2(0.0))) || any(greaterThan(old, vec2(1.0)))) {
    color = vec4(0.0);
    return;
  }
  vec4 c = texture(u_screen_tex, vec2(old.x, 1.0 - old.y));
  // Rounding down makes sure faint trails reach zero.
  color = floor(255.0 * c * u_fade) / 255.0;
}`;

/** Copies a texture onto the canvas. */
export const COPY_FRAG = `#version 300 es
${PRECISION}
uniform sampler2D u_screen_tex;
in vec2 v_pos;
out vec4 color;
void main() {
  color = texture(u_screen_tex, v_pos);
}`;
