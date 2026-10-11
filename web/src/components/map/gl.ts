/** Small WebGL2 helpers for the wind layers: programs, textures, framebuffers. */

export interface Program {
  program: WebGLProgram;
  /** Uniform locations by name; null for one the compiler dropped as unused */
  u: Record<string, WebGLUniformLocation | null>;
  /** Attribute locations by name */
  a: Record<string, number>;
}

function shader(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader {
  const s = gl.createShader(type);
  if (!s) throw new Error('createShader failed');
  gl.shaderSource(s, source);
  gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? 'shader');
  return s;
}

export function createProgram(gl: WebGL2RenderingContext, vertex: string, fragment: string): Program {
  const program = gl.createProgram();
  gl.attachShader(program, shader(gl, gl.VERTEX_SHADER, vertex));
  gl.attachShader(program, shader(gl, gl.FRAGMENT_SHADER, fragment));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program) ?? 'link');
  const found: Record<string, WebGLUniformLocation> = {};
  for (let i = 0; i < gl.getProgramParameter(program, gl.ACTIVE_UNIFORMS); i++) {
    const info = gl.getActiveUniform(program, i);
    const loc = info && gl.getUniformLocation(program, info.name);
    if (info && loc) found[info.name] = loc;
  }
  const u = new Proxy(found, { get: (t, k) => (typeof k === 'string' ? (t[k] ?? null) : null) });
  const a: Record<string, number> = {};
  for (let i = 0; i < gl.getProgramParameter(program, gl.ACTIVE_ATTRIBUTES); i++) {
    const info = gl.getActiveAttrib(program, i);
    if (info) a[info.name] = gl.getAttribLocation(program, info.name);
  }
  return { program, u, a };
}

/** Makes `p` the program to draw with; the one place for `gl.useProgram`, which the linter
 * takes for a React hook by its name. */
export function activate(gl: WebGL2RenderingContext, p: Program) {
  // biome-ignore lint/correctness/useHookAtTopLevel: WebGL, not a React hook
  gl.useProgram(p.program);
}

/** An RGBA8 texture from pixels or an image, clamped at the edges. */
export function createTexture(
  gl: WebGL2RenderingContext,
  filter: number,
  data: Uint8Array | TexImageSource | null,
  width = 0,
  height = 0,
): WebGLTexture {
  const t = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, t);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, filter);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, filter);
  // Wind is data, not a picture: no premultiplying, no colour management, rows as stored.
  gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
  gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
  gl.pixelStorei(gl.UNPACK_COLORSPACE_CONVERSION_WEBGL, gl.NONE);
  gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
  if (data === null || data instanceof Uint8Array) {
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, data);
  } else {
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, data);
  }
  return t;
}

export function bindTexture(gl: WebGL2RenderingContext, t: WebGLTexture, unit: number) {
  gl.activeTexture(gl.TEXTURE0 + unit);
  gl.bindTexture(gl.TEXTURE_2D, t);
}

/** Draws into `texture` from now on (null: the canvas). */
export function bindFramebuffer(
  gl: WebGL2RenderingContext,
  fb: WebGLFramebuffer | null,
  texture: WebGLTexture | null = null,
) {
  gl.bindFramebuffer(gl.FRAMEBUFFER, fb);
  if (fb && texture) gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
}

export function createBuffer(gl: WebGL2RenderingContext, data: Float32Array): WebGLBuffer {
  const b = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, b);
  gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
  return b;
}

export function bindAttribute(gl: WebGL2RenderingContext, buffer: WebGLBuffer, location: number, size: number) {
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  gl.enableVertexAttribArray(location);
  gl.vertexAttribPointer(location, size, gl.FLOAT, false, 0, 0);
}
