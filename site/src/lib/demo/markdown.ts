import { BOLD, ITALIC, type Style } from '../term/grid';
import { highlight, language } from './highlight';
import { type Line, type Seg, seg } from './text';

const DIM: Style = { fg: 8 };
const CODE: Style = { fg: 3 };
const REFERENCE: Style = { fg: 2 };
const MENTION: Style = { fg: 6 };

function inline(text: string, base: Style = {}): Seg[] {
  const out: Seg[] = [];
  const pattern = /(\*\*[^*]+\*\*|\*[^*]+\*|`[^`]+`|@[A-Za-z0-9_-]+|#\d+|[^*`@#]+|.)/g;
  for (const [token] of text.matchAll(pattern)) {
    if (token.startsWith('**') && token.length > 4) out.push(seg(token.slice(2, -2), { ...base, add: (base.add ?? 0) | BOLD }));
    else if (token.startsWith('*') && token.length > 2) out.push(seg(token.slice(1, -1), { ...base, add: (base.add ?? 0) | ITALIC }));
    else if (token.startsWith('`') && token.length > 2) out.push(seg(token.slice(1, -1), CODE));
    else if (/^@[A-Za-z0-9_-]+$/.test(token)) out.push(seg(token, MENTION));
    else if (/^#\d+$/.test(token)) out.push(seg(token, REFERENCE));
    else out.push(seg(token, base));
  }
  return out;
}

function words(segs: Seg[]): Seg[] {
  const out: Seg[] = [];
  for (const s of segs) for (const part of s.t.split(/(\s+)/)) if (part) out.push(seg(part, s.s));
  return out;
}

function wrapped(segs: Seg[], width: number, first: Seg[], rest: Seg[]): Line[] {
  const lines: Line[] = [];
  let line: Line = [...first];
  let used = first.reduce((n, s) => n + [...s.t].length, 0);
  const indent = rest.reduce((n, s) => n + [...s.t].length, 0);
  for (const w of words(segs)) {
    const size = [...w.t].length;
    const blank = !w.t.trim();
    if (used + size > width && !blank && used > indent) {
      lines.push(line);
      line = [...rest];
      used = indent;
    }
    if (blank && used === indent) continue;
    line.push(w);
    used += size;
  }
  lines.push(line);
  return lines;
}

export function plain(source: string): string {
  const out: string[] = [];
  let fenced = false;
  for (const raw of source.split('\n')) {
    if (raw.startsWith('```')) {
      fenced = !fenced;
      continue;
    }
    if (fenced) {
      out.push(raw);
      continue;
    }
    const rest = raw.replace(/^\s*#{1,6}\s+/, '').replace(/^\s*([-*]|\d+\.)\s+(\[( |x)\]\s+)?/, '');
    out.push(inline(rest).map((s) => s.t).join(''));
  }
  return out.join('\n');
}

export function render(source: string, width: number): Line[] {
  const out: Line[] = [];
  const gap = () => {
    if (out.length && out[out.length - 1].some((s) => s.t.trim())) out.push([]);
  };
  const lines = source.split('\n');
  let i = 0;
  while (i < lines.length) {
    const raw = lines[i];
    const fence = raw.match(/^```(\w*)/);
    if (fence) {
      gap();
      const lang = fence[1];
      if (lang) out.push([seg(lang, DIM)]);
      i += 1;
      while (i < lines.length && !lines[i].startsWith('```')) {
        out.push([seg('│ ', DIM), ...highlight(lines[i], language(`x.${lang === 'rust' ? 'rs' : lang}`))]);
        i += 1;
      }
      i += 1;
      continue;
    }
    if (!raw.trim()) {
      i += 1;
      continue;
    }
    const heading = raw.match(/^#{1,6}\s+(.*)$/);
    if (heading) {
      gap();
      out.push(...wrapped(inline(heading[1], { add: BOLD }), width, [], []));
      gap();
      i += 1;
      continue;
    }
    const item = raw.match(/^(\s*)([-*]|\d+\.)\s+(.*)$/);
    if (item) {
      const previous = out[out.length - 1];
      if (previous && previous.some((s) => s.t.trim()) && !/^\s*(•|\d+\.)/.test(previous[0]?.t ?? '')) gap();
      const depth = Math.floor(item[1].length / 2);
      const indent = '  '.repeat(depth);
      const bullet = /\d/.test(item[2]) ? `${item[2]} ` : '• ';
      let text = item[3];
      const prefix: Seg[] = [seg(indent + bullet)];
      const task = text.match(/^\[( |x)\]\s+(.*)$/);
      if (task) {
        prefix.push(task[1] === 'x' ? seg('[x] ', REFERENCE) : seg('[ ] '));
        text = task[2];
      }
      out.push(...wrapped(inline(text), width, prefix, [seg(' '.repeat(indent.length + bullet.length))]));
      i += 1;
      if (i < lines.length && !lines[i].trim().match(/^([-*]|\d+\.)\s/) && !lines[i].trim()) gap();
      continue;
    }
    const paragraph: string[] = [];
    while (i < lines.length && lines[i].trim() && !lines[i].startsWith('```') && !/^#{1,6}\s/.test(lines[i]) && !/^\s*([-*]|\d+\.)\s/.test(lines[i])) {
      paragraph.push(lines[i].trim());
      i += 1;
    }
    gap();
    out.push(...wrapped(inline(paragraph.join(' ')), width, [], []));
    gap();
  }
  while (out.length && !out[out.length - 1].some((s) => s.t.trim())) out.pop();
  return out;
}
