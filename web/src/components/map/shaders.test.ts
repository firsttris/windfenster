import { describe, expect, it } from 'vitest';
import * as shaders from './shaders';

// The shaders' full sources: everything exported that is a whole GLSL program
const programs = Object.entries(shaders).filter(
  (e): e is [string, string] => typeof e[1] === 'string' && e[1].startsWith('#version 300 es'),
);

describe('shader precision', () => {
  it('every shader reads numbers and textures at full precision', () => {
    // Low precision passes on SwiftShader and desktop OpenGL and breaks the particles where a
    // driver honours it (Mesa radeonsi under ANGLE): no test on a software GPU can see it.
    expect(programs.map(([name]) => name).sort()).toEqual(
      [
        'COPY_FRAG',
        'DRAW_FRAG',
        'DRAW_VERT',
        'FADE_FRAG',
        'FIELD_FRAG',
        'FIELD_VERT',
        'QUAD_VERT',
        'UPDATE_FRAG',
      ].sort(),
    );
    for (const [name, source] of programs) {
      expect(source, name).toContain('precision highp float;');
      expect(source, name).toContain('precision highp sampler2D;');
      expect(source, name).not.toMatch(/\b(mediump|lowp)\b/);
    }
  });
});
