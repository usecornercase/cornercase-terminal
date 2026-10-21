export type Span = [number, number];

export interface FindState {
  query: string;
  focused: boolean;
  at: Span;
}

export interface Hit {
  line: number;
  a: number;
  b: number;
}

export function occurrences(text: string, query: string): Span[] {
  if (!query) return [];
  const fold = !/\p{Lu}/u.test(query);
  const hay = [...(fold ? text.toLowerCase() : text)];
  const needle = [...(fold ? query.toLowerCase() : query)];
  const out: Span[] = [];
  let i = 0;
  while (i + needle.length <= hay.length) {
    if (needle.every((c, k) => hay[i + k] === c)) {
      out.push([i, i + needle.length]);
      i += needle.length;
    } else i += 1;
  }
  return out;
}

export function findAll(lines: string[], query: string): Hit[] {
  const wanted = query.trim();
  return lines.flatMap((text, i) => occurrences(text, wanted).map(([a, b]): Hit => ({ line: i + 1, a, b })));
}

const before = (h: Hit, at: Span): boolean => h.line < at[0] || (h.line === at[0] && h.a < at[1]);

function after(hits: Hit[], at: Span): number {
  const i = hits.findIndex((h) => !before(h, at));
  return i < 0 ? hits.length : i;
}

export function firstFrom(hits: Hit[], at: Span): number | null {
  if (!hits.length) return null;
  const i = after(hits, at);
  return i === hits.length ? 0 : i;
}

export function nearest(hits: Hit[], at: Span): number | null {
  if (!hits.length) return null;
  const i = after(hits, at);
  const below = i < hits.length ? i : null;
  const above = i > 0 ? i - 1 : null;
  if (below === null) return above;
  if (above === null) return below;
  return Math.abs(hits[above].line - at[0]) < Math.abs(hits[below].line - at[0]) ? above : below;
}

export const start = (h: Hit): Span => [h.line, h.a];
