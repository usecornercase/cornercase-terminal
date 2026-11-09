import { free, formats, glyphAtlas, pair, program, quad, swap, target } from './gl';
import type { Formats, Pair, Program, Target } from './gl';
import { ADVECT, CURL, DIVERGENCE, GRADIENT, PRESSURE, SHOW, SPLAT, STAMP, VORTICITY } from './shaders';

export type Colour = [number, number, number];

export const COBALT: Colour = [0.0, 0.3725, 1.0];

const DENSITY = [' ', ' ', '.', '·', ':', ';', '-', '=', '+', '*', 'x', '%', '#', '@', '▓', '█'];
const FLOW = ['─', '╱', '│', '╲'];
const GLYPHS = [...DENSITY, ...FLOW];
const FONT = "'JetBrains Mono Variable', ui-monospace, Menlo, monospace";
const SIM_HEIGHT = 90;
const CELL: [number, number] = [9, 18];
const NARROW_CELL: [number, number] = [7, 14];

interface Splat {
  x: number;
  y: number;
  dx: number;
  dy: number;
  colour: Colour;
  radius: number;
}

interface Programs {
  advect: Program;
  divergence: Program;
  curl: Program;
  vorticity: Program;
  pressure: Program;
  gradient: Program;
  splat: Program;
  stamp: Program;
  show: Program;
}

interface Buffers {
  velocity: Pair;
  dye: Pair;
  pressure: Pair;
  divergence: Target;
  curl: Target;
}

export class Field {
  gain = 1;
  glow = 0.6;
  markOn = 0;
  markFray = 0.08;
  markAt: [number, number] = [0.5, 0.5];
  markSize = 300;
  driftY = 0;
  swirl = 16;
  iterations = 14;
  cobalt: Colour = [...COBALT];

  private readonly gl: WebGL2RenderingContext;
  private readonly canvas: HTMLCanvasElement;
  private kinds: Formats;
  private programs: Programs;
  private buffers: Buffers | null = null;
  private atlas: WebGLTexture | null = null;
  private cell: [number, number] = [18, 36];
  private queue: Splat[] = [];
  private stamps = 0;
  private cssHeight = 1;

  get lvh(): number {
    return this.cssHeight;
  }

  private bound: Program | null = null;

  static start(canvas: HTMLCanvasElement): Field | null {
    const gl = canvas.getContext('webgl2', { alpha: false, antialias: false, depth: false, stencil: false, powerPreference: 'high-performance' });
    const kinds = gl && formats(gl);
    if (!gl || !kinds) return null;
    try {
      return new Field(canvas, gl, kinds);
    } catch {
      return null;
    }
  }

  private constructor(canvas: HTMLCanvasElement, gl: WebGL2RenderingContext, kinds: Formats) {
    this.canvas = canvas;
    this.gl = gl;
    this.kinds = kinds;
    this.programs = Field.compileAll(gl);
    Field.prepare(gl);
  }

  private static compileAll(gl: WebGL2RenderingContext): Programs {
    return {
      advect: program(gl, ADVECT),
      divergence: program(gl, DIVERGENCE),
      curl: program(gl, CURL),
      vorticity: program(gl, VORTICITY),
      pressure: program(gl, PRESSURE),
      gradient: program(gl, GRADIENT),
      splat: program(gl, SPLAT),
      stamp: program(gl, STAMP),
      show: program(gl, SHOW),
    };
  }

  private static prepare(gl: WebGL2RenderingContext): void {
    quad(gl);
    gl.disable(gl.BLEND);
    gl.disable(gl.DEPTH_TEST);
  }

  restore(): void {
    const kinds = formats(this.gl);
    if (!kinds) throw new Error('no float targets');
    this.kinds = kinds;
    this.programs = Field.compileAll(this.gl);
    Field.prepare(this.gl);
    this.buffers = null;
    this.atlas = null;
    this.bound = null;
    this.queue.length = 0;
    this.stamps = 0;
  }

  resize(width: number, height: number, dpr: number): void {
    const { gl, kinds } = this;
    const scale = Math.max(1, Math.round(dpr * 4) / 4);
    const pixels: [number, number] = [Math.max(1, Math.round(width * scale)), Math.max(1, Math.round(height * scale))];
    this.canvas.width = pixels[0];
    this.canvas.height = pixels[1];
    this.cssHeight = height;
    const base = width < 700 ? NARROW_CELL : CELL;
    this.cell = [Math.round(base[0] * scale), Math.round(base[1] * scale)];
    const grid: [number, number] = [Math.ceil(pixels[0] / this.cell[0]), Math.ceil(pixels[1] / this.cell[1])];
    const simHeight = width < 700 ? Math.round(SIM_HEIGHT * 1.4) : SIM_HEIGHT;
    const sim: [number, number] = [Math.max(16, Math.round((simHeight * width) / height)), simHeight];
    this.release();
    this.buffers = {
      velocity: pair(gl, sim, kinds.rg, kinds.type),
      dye: pair(gl, grid, kinds.rgba, kinds.type),
      pressure: pair(gl, sim, kinds.r, kinds.type),
      divergence: target(gl, sim, kinds.r, kinds.type),
      curl: target(gl, sim, kinds.r, kinds.type),
    };
    if (this.atlas) gl.deleteTexture(this.atlas);
    this.atlas = glyphAtlas(gl, GLYPHS, this.cell, FONT);
  }

  refreshGlyphs(): void {
    if (this.atlas) this.gl.deleteTexture(this.atlas);
    this.atlas = glyphAtlas(this.gl, GLYPHS, this.cell, FONT);
  }

  splat(x: number, y: number, dx: number, dy: number, colour: Colour, radius = 0.002): void {
    this.queue.push({ x, y, dx, dy, colour, radius });
  }

  stamp(): void {
    this.stamps += 1;
  }

  burst(x: number, y: number, power: number, colour: Colour): void {
    for (let i = 0; i < 18; i += 1) {
      const turn = (i / 18) * Math.PI * 2;
      const reach = 0.012 + (i % 3) * 0.004;
      this.splat(x + Math.cos(turn) * reach, y + Math.sin(turn) * reach, Math.cos(turn + 0.5) * power, Math.sin(turn + 0.5) * power, colour, 0.0016);
    }
  }

  frame(dt: number, time: number): void {
    const buffers = this.buffers;
    if (!buffers) return;
    const step = Math.min(dt, 1 / 30);
    this.gl.disable(this.gl.BLEND);
    this.applySplats(buffers);
    this.applyStamps(buffers);
    this.confine(buffers, step);
    this.project(buffers);
    this.advect(buffers, step);
    this.show(buffers, time);
  }

  private release(): void {
    const b = this.buffers;
    if (!b) return;
    free(this.gl, b.velocity.read, b.velocity.write, b.dye.read, b.dye.write, b.pressure.read, b.pressure.write, b.divergence, b.curl);
    this.buffers = null;
  }

  private use(which: Program): void {
    if (this.bound !== which) this.gl.useProgram(which.id);
    this.bound = which;
  }

  private texture(which: Program, name: string, unit: number, texture: WebGLTexture): void {
    const { gl } = this;
    gl.activeTexture(gl.TEXTURE0 + unit);
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.uniform1i(which.uniforms.get(name) ?? null, unit);
  }

  private draw(into: Target | null): void {
    const { gl } = this;
    if (into) {
      gl.bindFramebuffer(gl.FRAMEBUFFER, into.frame);
      gl.viewport(0, 0, into.width, into.height);
    } else {
      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
      gl.viewport(0, 0, this.canvas.width, this.canvas.height);
    }
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  }

  private uniform(which: Program, name: string): WebGLUniformLocation | null {
    return which.uniforms.get(name) ?? null;
  }

  private applySplats(b: Buffers): void {
    if (this.queue.length === 0) return;
    const { gl, programs } = this;
    const p = programs.splat;
    const aspect = this.canvas.width / this.canvas.height;
    this.use(p);
    gl.uniform1f(this.uniform(p, 'aspect'), aspect);
    for (const one of this.queue) {
      gl.uniform2f(this.uniform(p, 'point'), one.x, one.y);
      gl.uniform1f(this.uniform(p, 'radius'), one.radius);
      if (one.dx !== 0 || one.dy !== 0) {
        gl.uniform3f(this.uniform(p, 'amount'), one.dx, one.dy, 0);
        this.texture(p, 'target', 0, b.velocity.read.texture);
        this.draw(b.velocity.write);
        swap(b.velocity);
      }
      if (one.colour[0] + one.colour[1] + one.colour[2] > 0) {
        gl.uniform3f(this.uniform(p, 'amount'), one.colour[0], one.colour[1], one.colour[2]);
        this.texture(p, 'target', 0, b.dye.read.texture);
        this.draw(b.dye.write);
        swap(b.dye);
      }
    }
    this.queue.length = 0;
  }

  private markUniforms(p: Program): void {
    const { gl, canvas } = this;
    const scale = canvas.height / this.cssHeight;
    gl.uniform2f(this.uniform(p, 'pixels'), canvas.width, canvas.height);
    gl.uniform2f(this.uniform(p, 'markAt'), this.markAt[0] * canvas.width, this.markAt[1] * canvas.height);
    gl.uniform1f(this.uniform(p, 'markSize'), this.markSize * scale);
  }

  private applyStamps(b: Buffers): void {
    if (this.stamps === 0) return;
    const { gl, programs } = this;
    const p = programs.stamp;
    this.use(p);
    this.markUniforms(p);
    gl.uniform3f(this.uniform(p, 'cobalt'), this.cobalt[0], this.cobalt[1], this.cobalt[2]);
    this.texture(p, 'target', 0, b.dye.read.texture);
    this.draw(b.dye.write);
    swap(b.dye);
    this.stamps = 0;
  }

  private confine(b: Buffers, dt: number): void {
    const { gl, programs } = this;
    const texelX = 1 / b.curl.width;
    const texelY = 1 / b.curl.height;
    this.use(programs.curl);
    gl.uniform2f(this.uniform(programs.curl, 'texel'), texelX, texelY);
    this.texture(programs.curl, 'velocity', 0, b.velocity.read.texture);
    this.draw(b.curl);
    this.use(programs.vorticity);
    gl.uniform2f(this.uniform(programs.vorticity, 'texel'), texelX, texelY);
    gl.uniform1f(this.uniform(programs.vorticity, 'strength'), this.swirl);
    gl.uniform1f(this.uniform(programs.vorticity, 'dt'), dt);
    this.texture(programs.vorticity, 'velocity', 0, b.velocity.read.texture);
    this.texture(programs.vorticity, 'curl', 1, b.curl.texture);
    this.draw(b.velocity.write);
    swap(b.velocity);
  }

  private project(b: Buffers): void {
    const { gl, programs } = this;
    const texelX = 1 / b.curl.width;
    const texelY = 1 / b.curl.height;
    this.use(programs.divergence);
    gl.uniform2f(this.uniform(programs.divergence, 'texel'), texelX, texelY);
    this.texture(programs.divergence, 'velocity', 0, b.velocity.read.texture);
    this.draw(b.divergence);
    this.use(programs.pressure);
    gl.uniform2f(this.uniform(programs.pressure, 'texel'), texelX, texelY);
    this.texture(programs.pressure, 'divergence', 1, b.divergence.texture);
    for (let i = 0; i < this.iterations; i += 1) {
      this.texture(programs.pressure, 'pressure', 0, b.pressure.read.texture);
      this.draw(b.pressure.write);
      swap(b.pressure);
    }
    this.use(programs.gradient);
    gl.uniform2f(this.uniform(programs.gradient, 'texel'), texelX, texelY);
    this.texture(programs.gradient, 'pressure', 0, b.pressure.read.texture);
    this.texture(programs.gradient, 'velocity', 1, b.velocity.read.texture);
    this.draw(b.velocity.write);
    swap(b.velocity);
  }

  private advect(b: Buffers, dt: number): void {
    const { gl, programs } = this;
    const p = programs.advect;
    this.use(p);
    gl.uniform2f(this.uniform(p, 'texel'), 1 / b.curl.width, 1 / b.curl.height);
    gl.uniform2f(this.uniform(p, 'drift'), 0, this.driftY);
    gl.uniform1f(this.uniform(p, 'dt'), dt);
    gl.uniform1f(this.uniform(p, 'keep'), 1 / (1 + 0.55 * dt));
    this.texture(p, 'velocity', 0, b.velocity.read.texture);
    this.texture(p, 'source', 1, b.velocity.read.texture);
    this.draw(b.velocity.write);
    swap(b.velocity);
    gl.uniform1f(this.uniform(p, 'keep'), 1 / (1 + 0.62 * dt));
    this.texture(p, 'velocity', 0, b.velocity.read.texture);
    this.texture(p, 'source', 1, b.dye.read.texture);
    this.draw(b.dye.write);
    swap(b.dye);
  }

  private show(b: Buffers, time: number): void {
    const { gl, programs } = this;
    const p = programs.show;
    if (!this.atlas) return;
    this.use(p);
    this.markUniforms(p);
    gl.uniform2f(this.uniform(p, 'cell'), this.cell[0], this.cell[1]);
    gl.uniform1f(this.uniform(p, 'count'), GLYPHS.length);
    gl.uniform1f(this.uniform(p, 'time'), time);
    gl.uniform1f(this.uniform(p, 'gain'), this.gain);
    gl.uniform1f(this.uniform(p, 'glow'), this.glow);
    gl.uniform1f(this.uniform(p, 'markOn'), this.markOn);
    gl.uniform1f(this.uniform(p, 'markFray'), this.markFray);
    gl.uniform3f(this.uniform(p, 'cobalt'), this.cobalt[0], this.cobalt[1], this.cobalt[2]);
    this.texture(p, 'dye', 0, b.dye.read.texture);
    this.texture(p, 'velocity', 1, b.velocity.read.texture);
    this.texture(p, 'atlas', 2, this.atlas);
    this.draw(null);
  }
}
