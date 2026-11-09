export const VERTEX = `#version 300 es
in vec2 corner;
out vec2 uv;
void main() {
  uv = corner * 0.5 + 0.5;
  gl_Position = vec4(corner, 0.0, 1.0);
}`;

const HEAD = `#version 300 es
precision highp float;
precision highp sampler2D;
in vec2 uv;
out vec4 result;
`;

export const ADVECT = `${HEAD}
uniform sampler2D velocity;
uniform sampler2D source;
uniform vec2 texel;
uniform vec2 drift;
uniform float dt;
uniform float keep;
void main() {
  vec2 back = uv - dt * (texture(velocity, uv).xy * texel + drift);
  result = keep * texture(source, back);
}`;

export const DIVERGENCE = `${HEAD}
uniform sampler2D velocity;
uniform vec2 texel;
void main() {
  float l = texture(velocity, uv - vec2(texel.x, 0.0)).x;
  float r = texture(velocity, uv + vec2(texel.x, 0.0)).x;
  float b = texture(velocity, uv - vec2(0.0, texel.y)).y;
  float t = texture(velocity, uv + vec2(0.0, texel.y)).y;
  result = vec4(0.5 * (r - l + t - b), 0.0, 0.0, 1.0);
}`;

export const CURL = `${HEAD}
uniform sampler2D velocity;
uniform vec2 texel;
void main() {
  float l = texture(velocity, uv - vec2(texel.x, 0.0)).y;
  float r = texture(velocity, uv + vec2(texel.x, 0.0)).y;
  float b = texture(velocity, uv - vec2(0.0, texel.y)).x;
  float t = texture(velocity, uv + vec2(0.0, texel.y)).x;
  result = vec4(0.5 * (r - l - t + b), 0.0, 0.0, 1.0);
}`;

export const VORTICITY = `${HEAD}
uniform sampler2D velocity;
uniform sampler2D curl;
uniform vec2 texel;
uniform float strength;
uniform float dt;
void main() {
  float l = texture(curl, uv - vec2(texel.x, 0.0)).x;
  float r = texture(curl, uv + vec2(texel.x, 0.0)).x;
  float b = texture(curl, uv - vec2(0.0, texel.y)).x;
  float t = texture(curl, uv + vec2(0.0, texel.y)).x;
  float c = texture(curl, uv).x;
  vec2 force = 0.5 * vec2(abs(t) - abs(b), abs(r) - abs(l));
  force /= length(force) + 0.0001;
  force *= strength * c;
  force.y *= -1.0;
  result = vec4(texture(velocity, uv).xy + force * dt, 0.0, 1.0);
}`;

export const PRESSURE = `${HEAD}
uniform sampler2D pressure;
uniform sampler2D divergence;
uniform vec2 texel;
void main() {
  float l = texture(pressure, uv - vec2(texel.x, 0.0)).x;
  float r = texture(pressure, uv + vec2(texel.x, 0.0)).x;
  float b = texture(pressure, uv - vec2(0.0, texel.y)).x;
  float t = texture(pressure, uv + vec2(0.0, texel.y)).x;
  float d = texture(divergence, uv).x;
  result = vec4((l + r + b + t - d) * 0.25, 0.0, 0.0, 1.0);
}`;

export const GRADIENT = `${HEAD}
uniform sampler2D pressure;
uniform sampler2D velocity;
uniform vec2 texel;
void main() {
  float l = texture(pressure, uv - vec2(texel.x, 0.0)).x;
  float r = texture(pressure, uv + vec2(texel.x, 0.0)).x;
  float b = texture(pressure, uv - vec2(0.0, texel.y)).x;
  float t = texture(pressure, uv + vec2(0.0, texel.y)).x;
  result = vec4(texture(velocity, uv).xy - 0.5 * vec2(r - l, t - b), 0.0, 1.0);
}`;

export const SPLAT = `${HEAD}
uniform sampler2D target;
uniform vec2 point;
uniform vec3 amount;
uniform float radius;
uniform float aspect;
void main() {
  vec2 p = uv - point;
  p.x *= aspect;
  vec3 add = exp(-dot(p, p) / radius) * amount;
  result = vec4(texture(target, uv).rgb + add, 1.0);
}`;

const MARK = `
uniform vec2 markAt;
uniform float markSize;
uniform vec2 pixels;

float box(vec2 p, vec2 a, vec2 b) {
  return step(a.x, p.x) * step(p.x, b.x) * step(a.y, p.y) * step(p.y, b.y);
}

vec2 markCell(vec2 at) {
  vec2 p = (at * pixels - markAt) / markSize;
  p.y = -p.y;
  return p * 12.0 + 6.0;
}

float markCorner(vec2 p) {
  return max(box(p, vec2(3.0, 0.0), vec2(12.0, 3.0)), box(p, vec2(0.0, 3.0), vec2(3.0, 12.0)));
}

float markCursor(vec2 p) {
  return box(p, vec2(6.0, 6.0), vec2(8.0, 11.0));
}
`;

export const STAMP = `${HEAD}
uniform sampler2D target;
uniform vec3 cobalt;
${MARK}
void main() {
  vec2 p = markCell(uv);
  vec3 add = markCorner(p) * cobalt * 1.15 + markCursor(p) * vec3(0.95, 0.97, 1.05);
  result = vec4(texture(target, uv).rgb + add, 1.0);
}`;

export const SHOW = `${HEAD}
uniform sampler2D dye;
uniform sampler2D velocity;
uniform sampler2D atlas;
uniform vec2 cell;
uniform float count;
uniform float time;
uniform float gain;
uniform float glow;
uniform float markOn;
uniform float markFray;
uniform vec3 cobalt;
${MARK}

const vec3 ground = vec3(0.0431, 0.0471, 0.0549);

float hash(vec2 p) {
  p = fract(p * vec2(123.34, 456.21));
  p += dot(p, p + 45.32);
  return fract(p.x * p.y);
}

float noise(vec2 p) {
  vec2 i = floor(p);
  vec2 f = fract(p);
  f = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
}

float cloud(vec2 p) {
  return noise(p) * 0.6 + noise(p * 2.1 + 7.3) * 0.28 + noise(p * 4.3 + 3.1) * 0.12;
}

void main() {
  vec2 here = uv * pixels;
  vec2 slot = floor(here / cell);
  vec2 inside = fract(here / cell);
  vec2 centre = (slot + 0.5) * cell / pixels;

  vec3 d = texture(dye, centre).rgb;
  vec2 v = texture(velocity, centre).xy;
  float speed = length(v);

  vec2 drifting = vec2(slot.x * 0.018 * pixels.y / pixels.x * 1.7 + time * 0.012, slot.y * 0.036 - time * 0.02);
  float idle = pow(cloud(drifting), 2.6) * 0.34;
  float twinkle = hash(slot + floor(time * 5.0 + hash(slot) * 5.0));
  idle *= 0.72 + 0.28 * twinkle;

  float raw = max(d.r, max(d.g, d.b));
  float lit = raw / (1.0 + 0.62 * raw);
  float amount = lit + idle * (1.0 - smoothstep(0.0, 0.3, lit));
  vec3 hue = raw > 0.001 ? d / raw : cobalt;
  hue = mix(cobalt, hue, smoothstep(0.02, 0.2, lit));

  float markMask = 0.0;
  vec3 markHue = cobalt;
  if (markOn > 0.001) {
    vec2 off = centre + v * 0.00005;
    vec2 p = markCell(off);
    float corner = markCorner(p);
    float cursor = markCursor(p);
    float seed = hash(slot * 0.77 + 3.1);
    float gate = step(seed, markOn) * (1.0 - step(1.0 - markFray, seed) * (1.0 - step(0.5, twinkle)));
    markMask = max(corner, cursor) * gate;
    markHue = cursor > 0.5 ? vec3(0.93, 0.95, 1.0) : mix(vec3(0.0, 0.42, 1.0), vec3(0.17, 0.48, 1.0), twinkle);
  }

  amount = max(amount * gain, markMask * (0.86 + 0.14 * twinkle));
  hue = mix(hue, markHue, markMask);

  float slots = count - 4.0;
  float pick = clamp(pow(amount, 0.8), 0.0, 0.999) * slots;
  float which = floor(pick);
  float flowing = smoothstep(150.0, 620.0, speed) * 0.34;
  if (hash(slot + 9.7) < flowing && amount > 0.1 && amount < 0.8 && markMask < 0.5) {
    float angle = atan(v.y, v.x);
    float turn = floor(mod(angle + 3.14159265 / 8.0, 3.14159265) / (3.14159265 / 4.0));
    which = slots + mod(turn, 4.0);
  }
  if (markMask > 0.5) which = slots - 1.0 - step(0.78, hash(slot + 4.4));
  vec2 tile = vec2((which + inside.x) / count, 1.0 - inside.y);
  float ink = texture(atlas, tile).r;

  vec3 bright = mix(hue, vec3(1.0), 0.46);
  vec3 tone = hue * (0.34 + 1.5 * smoothstep(0.0, 0.42, amount));
  tone = mix(tone, bright * 1.15, smoothstep(0.36, 0.74, amount));
  tone = mix(tone, vec3(0.93, 0.95, 1.0), smoothstep(0.72, 1.05, amount));
  tone = mix(tone, markHue, markMask);

  vec3 soft = texture(dye, uv).rgb;
  vec3 halo = soft / (1.0 + 0.8 * soft) * glow * gain;
  vec2 from = uv - 0.5;
  from.x *= pixels.x / pixels.y;
  float edge = 1.0 - smoothstep(0.34, 1.28, length(from));

  vec3 colour = ground + halo * edge + ink * tone * mix(0.62, 1.0, edge);
  result = vec4(colour, 1.0);
}`;
