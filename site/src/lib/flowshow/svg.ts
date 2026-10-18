import {
  CAST,
  EDGES,
  END_TAG,
  END_TITLE,
  ISSUE_KEY,
  LAYOUTS,
  NODES,
  arrowHead,
  centre,
  pathD,
  roundedRect,
  source,
  type Box,
  type EdgeId,
  type Layout,
  type NodeId,
} from './graph';
import { FINAL, PACKETS } from './timeline';

const esc = (s: string): string => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
const num = (n: number): string => String(Math.round(n * 100) / 100);

const GLYPHS: [string, string][] = [
  ['idle', '○'],
  ['working', '◐'],
  ['done', '✓'],
  ['finding', '!'],
  ['pending', '●'],
];

function badge(w: number): string {
  const glyphs = GLYPHS.map(([k, g]) => `<text class="g-${k}" text-anchor="middle" dominant-baseline="central">${g}</text>`).join('');
  return `<g class="fs-badge" transform="translate(${w} 0)"><circle r="11"/>${glyphs}</g>`;
}

function node(id: Exclude<NodeId, 'end'>, b: Box, l: Layout, id0: string): string {
  const { fonts } = l;
  const orch = id === 'orchestrator';
  const roleY = orch ? 30 : Math.round(b.h * 0.43 + 4);
  const agentY = roleY + (orch ? 18 : 19);
  const path = roundedRect(b.w, b.h, 12);
  let extra = '';
  if (orch) {
    extra = `<rect class="fs-term" x="12" y="56" width="${b.w - 24}" height="26" rx="6"/><text class="fs-tick" x="22" y="73" font-size="${fonts.ticker}"><tspan class="fs-prompt">›</tspan> <tspan data-tick>${esc(FINAL.ticker)}</tspan><tspan class="fs-caret">▍</tspan></text>`;
  }
  if (id === 'coder') {
    const pips = [1, 2, 3, 4]
      .map((n) => `<circle class="fs-pip" data-pip="${n}"${FINAL.commits >= n ? ' data-on=""' : ''} cx="${b.w - 74 + n * 14}" cy="${Math.round(b.h * 0.43 + 4) - 6}" r="3.5"/>`)
      .join('');
    extra = `<g class="fs-commits">${pips}</g>`;
  }
  const ripple = orch ? `<rect class="fs-ripple" data-ripple x="0" y="0" width="${b.w}" height="${b.h}" rx="12" opacity="0"/>` : '';
  return `<g class="fs-node r-${id}" data-node="${id}" data-state="${FINAL.nodes[id]}" transform="translate(${b.x} ${b.y})">
${ripple}<g class="fs-in"><rect class="fs-halo" x="-4" y="-4" width="${b.w + 8}" height="${b.h + 8}" rx="16" filter="url(#${id0}-blur)"/><g class="fs-body"><path class="fs-box" d="${path}"/><path class="fs-beam" d="${path}" pathLength="100"/><text class="fs-role" x="16" y="${roleY}" font-size="${fonts.role}">${id}</text><text class="fs-agent" x="16" y="${agentY}" font-size="${fonts.agent}">${esc(CAST[id].agent)}</text>${extra}</g>${badge(b.w)}</g></g>`;
}

function end(b: Box, l: Layout): string {
  const { fonts } = l;
  const [cx, cy] = [b.w / 2, b.h / 2];
  const streaks = Array.from({ length: 10 }, (_, i) => {
    const a = (i / 10) * Math.PI * 2;
    const p = (r: number): string => `${num(cx + Math.cos(a) * r)} ${num(cy + Math.sin(a) * r)}`;
    return `<path d="M ${p(Math.min(cx, cy) + 4)} L ${p(Math.min(cx, cy) + 44)}" pathLength="100"/>`;
  }).join('');
  return `<g class="fs-end" data-node="end" data-state="${FINAL.nodes.end}" data-checks="${FINAL.checks}" transform="translate(${b.x} ${b.y})"><g class="fs-in"><path class="fs-box" d="${roundedRect(b.w, b.h, 12)}"/><text class="fs-diamond" x="18" y="${b.h / 2 - 6}" font-size="${fonts.endTitle + 3}" dominant-baseline="central">◆</text><text class="fs-role" x="42" y="${b.h / 2 - 6}" font-size="${fonts.endTitle}" dominant-baseline="central">${END_TITLE}</text><g class="fs-tag"><text x="42" y="${b.h / 2 + 16}" font-size="${fonts.tag}" dominant-baseline="central">${END_TAG}<tspan class="fs-run"> •</tspan><tspan class="fs-pass"> ✓</tspan></text></g><g class="fs-burst">${streaks}</g></g></g>`;
}

export function graphSvg(name: 'wide' | 'tall'): string {
  const l = LAYOUTS[name];
  const p = `fs-${name}`;
  const edges = EDGES.map((e: EdgeId) => {
    const c = l.edges[e];
    const lit = FINAL.lit.includes(e) ? ' data-lit=""' : '';
    const loop = e === 'reviewer>coder' ? ' fs-loop' : '';
    return `<path class="fs-edge r-${source(e) === 'issue' ? 'issue' : source(e)}${loop}" data-edge="${e}"${lit} d="${pathD(c)}" pathLength="100"/><path class="fs-head r-${source(e) === 'issue' ? 'issue' : source(e)}${loop}" data-head="${e}"${lit} d="${arrowHead(c)}"/>`;
  }).join('');
  const lab = l.loopLabel;
  const label = `<g class="fs-label" data-loop transform="translate(${lab.x} ${lab.y})"><rect width="${lab.w}" height="${lab.h}" rx="${lab.h / 2}"/><text data-loop-text x="${lab.w / 2}" y="${lab.h / 2}" font-size="${l.fonts.loop}" text-anchor="middle" dominant-baseline="central">${l.loopText(FINAL.rounds)}</text></g>`;
  const slots = (cls: string, inner: string): string =>
    `<g class="${cls}">${Array.from({ length: PACKETS }, (_, i) => inner.replace(/%i/g, String(i))).join('')}</g>`;
  const trails = slots('fs-trails', '<path class="fs-trail" data-trail="%i" pathLength="100" stroke-dasharray="18 100" visibility="hidden" d="M 0 0"/>');
  const packets = slots('fs-packets', '<g class="fs-packet" data-packet="%i" visibility="hidden" transform="translate(0 0)"><circle class="fs-glow" r="12"/><circle class="fs-core" r="5"/></g>');
  const i = l.issue;
  const issue = `<g class="fs-issue" transform="translate(${i.x} ${i.y})"><path class="fs-pill" d="${roundedRect(i.w, i.h, i.h / 2)}"/><text x="16" y="${i.h / 2}" font-size="${l.fonts.issue}" dominant-baseline="central"><tspan class="fs-key">${ISSUE_KEY}</tspan><tspan class="fs-title">${esc(l.issueText)}</tspan></text></g>`;
  const nodes = NODES.map((id) => (id === 'end' ? end(l.nodes.end, l) : node(id, l.nodes[id], l, p))).join('\n');
  const [sx, sy] = centre(l.nodes.end);
  return `<svg class="fs-graph fs-${name}" data-layout="${name}" viewBox="0 0 ${l.width} ${l.height}" aria-hidden="true" focusable="false">
<defs><filter id="${p}-blur" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="10"/></filter><radialGradient id="${p}-spot"><stop offset="0" stop-color="#875fff" stop-opacity="0.28"/><stop offset="1" stop-color="#875fff" stop-opacity="0"/></radialGradient></defs>
<circle class="fs-spot" data-spot r="${l.spot}" cx="${num(sx)}" cy="${num(sy)}" fill="url(#${p}-spot)"/>
<g class="fs-edges">${edges}</g>
${label}
${trails}
<g class="fs-nodes">${issue}
${nodes}</g>
${packets}
</svg>`;
}
