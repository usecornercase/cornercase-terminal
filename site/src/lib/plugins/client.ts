import { SORTS, type Sort, matches, timeAgo } from './market';

document.documentElement.classList.add('js');

const grid = document.querySelector<HTMLElement>('[data-grid]');
const search = document.querySelector<HTMLInputElement>('[data-search]');
const select = document.querySelector<HTMLSelectElement>('[data-sort]');
const none = document.querySelector<HTMLElement>('[data-none]');
const more = document.querySelector<HTMLButtonElement>('[data-more]');
const count = document.querySelector<HTMLElement>('[data-line-count]');
const size = Number(grid?.dataset.page ?? 24);
const cards = [...(grid?.querySelectorAll<HTMLElement>('[data-card]') ?? [])];
const state = { query: '', sort: 'popular' as Sort, shown: size };
const noun = (n: number, one: string, many: string) => `${n.toLocaleString('en-US')} ${n === 1 ? one : many}`;
const rank = (card: HTMLElement) => Number(card.dataset[`rank${state.sort[0].toUpperCase()}${state.sort.slice(1)}`] ?? 0);

function apply(): void {
  if (!grid) return;
  const ordered = [...cards].sort((a, b) => rank(a) - rank(b));
  const hits = ordered.filter((card) => matches(card.dataset.haystack ?? '', state.query));
  const visible = new Set(hits.slice(0, state.shown));
  for (const card of ordered) {
    card.hidden = !visible.has(card);
    grid.append(card);
  }
  if (none) none.hidden = hits.length > 0 || cards.length === 0;
  const left = hits.length - visible.size;
  if (more) {
    more.hidden = left <= 0;
    more.textContent = `show ${left.toLocaleString('en-US')} more`;
  }
  if (count) {
    const plugins = hits.reduce((n, card) => n + Number(card.dataset.plugins ?? 0), 0);
    count.textContent = `${noun(plugins, 'plugin', 'plugins')} · ${noun(hits.length, 'repository', 'repositories')}`;
  }
}

function reset(): void {
  state.shown = size;
  apply();
}

search?.addEventListener('input', () => {
  state.query = search.value;
  reset();
});
select?.addEventListener('change', () => {
  state.sort = (SORTS as string[]).includes(select.value) ? (select.value as Sort) : 'popular';
  reset();
});
more?.addEventListener('click', () => {
  state.shown += size;
  apply();
});

for (const button of document.querySelectorAll<HTMLButtonElement>('[data-see]')) {
  button.addEventListener('click', () => {
    const sort = button.dataset.see as Sort;
    if (!select || !(SORTS as string[]).includes(sort)) return;
    select.value = sort;
    state.sort = sort;
    state.query = search?.value ?? '';
    reset();
    const calm = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    (document.querySelector('.browse') ?? grid)?.scrollIntoView({ behavior: calm ? 'auto' : 'smooth', block: 'start' });
  });
}

document.addEventListener('keydown', (event) => {
  if (event.key !== '/' || event.metaKey || event.ctrlKey || event.altKey || !search) return;
  const target = event.target as HTMLElement | null;
  if (target && (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))) return;
  event.preventDefault();
  search.focus();
});

document.addEventListener('click', async (event) => {
  const button = (event.target as HTMLElement | null)?.closest<HTMLButtonElement>('.plugins-page .cmd [data-plugin-copy]');
  if (!button) return;
  const label = button.textContent;
  try {
    await navigator.clipboard.writeText(button.dataset.pluginCopy ?? '');
    button.textContent = 'copied';
    setTimeout(() => {
      button.textContent = label === 'copied' ? 'copy' : label;
    }, 1600);
  } catch {}
});

function initial(image: HTMLImageElement): void {
  const holder = image.closest<HTMLElement>('[data-avatar]');
  if (holder) holder.textContent = holder.dataset.initial ?? '?';
}

document.addEventListener(
  'error',
  (event) => {
    if (event.target instanceof HTMLImageElement) initial(event.target);
  },
  true,
);
for (const image of document.querySelectorAll<HTMLImageElement>('[data-avatar] img')) {
  if (image.complete && image.naturalWidth === 0) initial(image);
}

const now = new Date();
for (const time of document.querySelectorAll<HTMLTimeElement>('time[data-ago]')) {
  const ago = timeAgo(time.dateTime, now);
  time.textContent =
    time.dataset.ago === 'joined' ? (ago === 'just now' ? '✦ just joined' : `✦ joined ${ago}`) : `updated ${ago}`;
}

apply();
