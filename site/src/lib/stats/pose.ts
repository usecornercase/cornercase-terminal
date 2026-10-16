export type Matrix = number[];

export interface Pose { rx: number; ry: number; rz: number; lift: number; scale: number }
export interface Light { x: number; y: number }

export const STAGE = { w: 1200, h: 675 };
export const TICKET = { w: 1000, h: 520 };
export const DEPTH = 1800;
export const REST: Pose = { rx: 9, ry: 15, rz: -4, lift: 0, scale: 0.9 };
export const LIT: Light = { x: 0.3, y: 0.22 };
export const CENTER = { x: 610, y: 322 };
const TILT = { x: 13, y: 18 };

const IDENTITY: Matrix = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
const radians = (degrees: number) => degrees * Math.PI / 180;

export function multiply(a: Matrix, b: Matrix): Matrix {
  const out = new Array<number>(16).fill(0);
  for (let row = 0; row < 4; row++) for (let col = 0; col < 4; col++) {
    let sum = 0;
    for (let k = 0; k < 4; k++) sum += a[row * 4 + k] * b[k * 4 + col];
    out[row * 4 + col] = sum;
  }
  return out;
}

const chain = (...matrices: Matrix[]): Matrix => matrices.reduce(multiply, IDENTITY);
export const translate = (x: number, y: number, z = 0): Matrix => [1, 0, 0, x, 0, 1, 0, y, 0, 0, 1, z, 0, 0, 0, 1];
export const scale = (s: number): Matrix => [s, 0, 0, 0, 0, s, 0, 0, 0, 0, s, 0, 0, 0, 0, 1];
export const perspective = (d: number): Matrix => [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, -1 / d, 1];
export function rotateX(degrees: number): Matrix { const c = Math.cos(radians(degrees)), s = Math.sin(radians(degrees)); return [1, 0, 0, 0, 0, c, -s, 0, 0, s, c, 0, 0, 0, 0, 1]; }
export function rotateY(degrees: number): Matrix { const c = Math.cos(radians(degrees)), s = Math.sin(radians(degrees)); return [c, 0, s, 0, 0, 1, 0, 0, -s, 0, c, 0, 0, 0, 0, 1]; }
export function rotateZ(degrees: number): Matrix { const c = Math.cos(radians(degrees)), s = Math.sin(radians(degrees)); return [c, -s, 0, 0, s, c, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]; }

export function ticketMatrix(pose: Pose): Matrix {
  return chain(translate(CENTER.x, CENTER.y), perspective(DEPTH), rotateX(pose.rx), rotateY(pose.ry), rotateZ(pose.rz), translate(0, 0, pose.lift), scale(pose.scale), translate(-TICKET.w / 2, -TICKET.h / 2));
}

const behind = (pose: Pose, drop: number, depth: number, size: number): Matrix => chain(translate(CENTER.x, CENTER.y + drop), perspective(DEPTH), rotateX(pose.rx), rotateY(pose.ry), rotateZ(pose.rz), translate(0, 0, depth), scale(pose.scale * size), translate(-TICKET.w / 2, -TICKET.h / 2));
export const shadowMatrix = (pose: Pose): Matrix => behind(pose, 46 + pose.lift * 0.6, -70, 0.94);
export const haloMatrix = (pose: Pose): Matrix => behind(pose, pose.lift * 0.2, -40, 1.03);

export function project(m: Matrix, x: number, y: number): [number, number] {
  const w = m[12] * x + m[13] * y + m[15];
  return [(m[0] * x + m[1] * y + m[3]) / w, (m[4] * x + m[5] * y + m[7]) / w];
}

export function css(m: Matrix): string {
  const columns = [0, 1, 2, 3].flatMap((col) => [0, 1, 2, 3].map((row) => m[row * 4 + col]));
  return `matrix3d(${columns.map((n) => Number(n.toFixed(9))).join(',')})`;
}

export function aimed(pointer: { x: number; y: number }): { pose: Pose; light: Light } {
  const x = Math.max(-1, Math.min(1, pointer.x)), y = Math.max(-1, Math.min(1, pointer.y));
  return { pose: { rx: -y * TILT.x, ry: x * TILT.y, rz: 0, lift: 40, scale: 0.92 }, light: { x: (x + 1) / 2, y: (y + 1) / 2 } };
}

export function drift(seconds: number): { pose: Pose; light: Light } {
  const t = seconds;
  return {
    pose: { rx: REST.rx + 1.6 * Math.sin(t * 0.6), ry: REST.ry + 3.2 * Math.sin(t * 0.43 + 1.1), rz: REST.rz + 0.7 * Math.sin(t * 0.31 + 2), lift: 12 * Math.sin(t * 0.8), scale: REST.scale },
    light: { x: LIT.x + 0.1 * Math.sin(t * 0.43 + 1.1), y: LIT.y + 0.06 * Math.cos(t * 0.6) },
  };
}

export function approach<T extends object>(from: T, to: T, rate: number, seconds: number): T {
  const k = 1 - Math.exp(-rate * seconds);
  const out = { ...from } as Record<string, number>;
  for (const key of Object.keys(to)) out[key] += ((to as Record<string, number>)[key] - out[key]) * k;
  return out as T;
}
