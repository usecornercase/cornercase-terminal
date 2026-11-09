import { VERTEX } from './shaders';

export interface Target {
  texture: WebGLTexture;
  frame: WebGLFramebuffer;
  width: number;
  height: number;
}

export interface Pair {
  read: Target;
  write: Target;
}

export interface Program {
  id: WebGLProgram;
  uniforms: Map<string, WebGLUniformLocation>;
}

export interface Format {
  internal: number;
  format: number;
}

export interface Formats {
  rgba: Format;
  rg: Format;
  r: Format;
  type: number;
}

function compile(gl: WebGL2RenderingContext, kind: number, text: string): WebGLShader {
  const shader = gl.createShader(kind);
  if (!shader) throw new Error('no shader');
  gl.shaderSource(shader, text);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(shader) ?? 'shader failed');
  }
  return shader;
}

export function program(gl: WebGL2RenderingContext, fragment: string): Program {
  const id = gl.createProgram();
  if (!id) throw new Error('no program');
  gl.attachShader(id, compile(gl, gl.VERTEX_SHADER, VERTEX));
  gl.attachShader(id, compile(gl, gl.FRAGMENT_SHADER, fragment));
  gl.bindAttribLocation(id, 0, 'corner');
  gl.linkProgram(id);
  if (!gl.getProgramParameter(id, gl.LINK_STATUS)) {
    throw new Error(gl.getProgramInfoLog(id) ?? 'program failed');
  }
  const uniforms = new Map<string, WebGLUniformLocation>();
  const count = gl.getProgramParameter(id, gl.ACTIVE_UNIFORMS) as number;
  for (let i = 0; i < count; i += 1) {
    const info = gl.getActiveUniform(id, i);
    const place = info && gl.getUniformLocation(id, info.name);
    if (info && place) uniforms.set(info.name, place);
  }
  return { id, uniforms };
}

export function quad(gl: WebGL2RenderingContext): void {
  const buffer = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
  gl.enableVertexAttribArray(0);
  gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
}

function renderable(gl: WebGL2RenderingContext, wanted: Format, type: number): boolean {
  const texture = gl.createTexture();
  const frame = gl.createFramebuffer();
  gl.bindTexture(gl.TEXTURE_2D, texture);
  gl.texImage2D(gl.TEXTURE_2D, 0, wanted.internal, 4, 4, 0, wanted.format, type, null);
  gl.bindFramebuffer(gl.FRAMEBUFFER, frame);
  gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
  const fine = gl.checkFramebufferStatus(gl.FRAMEBUFFER) === gl.FRAMEBUFFER_COMPLETE;
  gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  gl.deleteFramebuffer(frame);
  gl.deleteTexture(texture);
  return fine;
}

export function formats(gl: WebGL2RenderingContext): Formats | null {
  const float = gl.getExtension('EXT_color_buffer_float');
  const half = gl.getExtension('EXT_color_buffer_half_float');
  if (!float && !half) return null;
  const type = gl.HALF_FLOAT;
  const rgba = { internal: gl.RGBA16F, format: gl.RGBA };
  if (!renderable(gl, rgba, type)) return null;
  const rgWanted = { internal: gl.RG16F, format: gl.RG };
  const rWanted = { internal: gl.R16F, format: gl.RED };
  return {
    rgba,
    rg: renderable(gl, rgWanted, type) ? rgWanted : rgba,
    r: renderable(gl, rWanted, type) ? rWanted : rgba,
    type,
  };
}

export function target(gl: WebGL2RenderingContext, size: [number, number], format: Format, type: number): Target {
  const texture = gl.createTexture();
  const frame = gl.createFramebuffer();
  if (!texture || !frame) throw new Error('no target');
  gl.bindTexture(gl.TEXTURE_2D, texture);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  gl.texImage2D(gl.TEXTURE_2D, 0, format.internal, size[0], size[1], 0, format.format, type, null);
  gl.bindFramebuffer(gl.FRAMEBUFFER, frame);
  gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
  if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE) throw new Error('no room for a target');
  gl.viewport(0, 0, size[0], size[1]);
  gl.clearColor(0, 0, 0, 1);
  gl.clear(gl.COLOR_BUFFER_BIT);
  return { texture, frame, width: size[0], height: size[1] };
}

export function pair(gl: WebGL2RenderingContext, size: [number, number], format: Format, type: number): Pair {
  return { read: target(gl, size, format, type), write: target(gl, size, format, type) };
}

export function swap(both: Pair): void {
  const held = both.read;
  both.read = both.write;
  both.write = held;
}

export function free(gl: WebGL2RenderingContext, ...targets: Target[]): void {
  for (const one of targets) {
    gl.deleteFramebuffer(one.frame);
    gl.deleteTexture(one.texture);
  }
}

export function glyphAtlas(gl: WebGL2RenderingContext, glyphs: string[], tile: [number, number], font: string): WebGLTexture {
  const canvas = document.createElement('canvas');
  canvas.width = tile[0] * glyphs.length;
  canvas.height = tile[1];
  const ink = canvas.getContext('2d');
  const texture = gl.createTexture();
  if (!ink || !texture) throw new Error('no atlas');
  ink.fillStyle = '#000';
  ink.fillRect(0, 0, canvas.width, canvas.height);
  ink.fillStyle = '#fff';
  ink.font = `${Math.round(tile[0] / 0.6)}px ${font}`;
  ink.textAlign = 'center';
  ink.textBaseline = 'middle';
  glyphs.forEach((glyph, i) => ink.fillText(glyph, tile[0] * (i + 0.5), tile[1] * 0.54));
  gl.bindTexture(gl.TEXTURE_2D, texture);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, canvas);
  return texture;
}
