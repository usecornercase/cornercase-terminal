import { fromFragment } from './controller';
import { drawCard, fonts } from './card';
import { ERROR, postText, type WindowName } from './model';

const canvas = document.querySelector<HTMLCanvasElement>('[data-card]');
const card = document.querySelector<HTMLElement>('[data-stats]');
const error = document.querySelector<HTMLElement>('[data-error]');
const png = (canvas: HTMLCanvasElement): Promise<Blob> => new Promise((resolve, reject) => canvas.toBlob((blob) => blob ? resolve(blob) : reject(new Error('could not make PNG')), 'image/png'));

async function mount(): Promise<void> {
  if (!canvas || !card || !error) return;
  const state = fromFragment(location.hash);
  if (!state) { card.hidden = true; error.hidden = false; error.textContent = ERROR; return; }
  card.hidden = false; error.hidden = true;
  const { stats, example } = state;
  const note = document.querySelector<HTMLElement>('[data-example]'); if (note) note.hidden = !example;
  await fonts();
  const tabs = [...document.querySelectorAll<HTMLButtonElement>('[data-window]')];
  const x = document.querySelector<HTMLAnchorElement>('[data-x]');
  const linkedin = document.querySelector<HTMLAnchorElement>('[data-linkedin]');
  const draw = () => {
    drawCard(canvas, stats, state.window, example, new URLSearchParams(location.search).has('preview'));
    tabs.forEach((tab) => tab.setAttribute('aria-selected', String(tab.dataset.window === state.window)));
    const publicUrl = new URL(location.href); publicUrl.search = ''; const url = publicUrl.href; const text = postText(stats, state.window);
    if (x) x.href = `https://twitter.com/intent/tweet?text=${encodeURIComponent(text)}&url=${encodeURIComponent(url)}`;
    if (linkedin) linkedin.href = `https://www.linkedin.com/sharing/share-offsite/?url=${encodeURIComponent(url)}`;
    const written = document.querySelector<HTMLElement>('[data-post-text]'); if (written) written.textContent = text;
  };
  draw();
  tabs.forEach((tab) => { tab.onclick = () => { state.select(tab.dataset.window as WindowName); draw(); }; });
  const status = document.querySelector<HTMLElement>('[data-status]');
  const act = (run: () => Promise<void>) => async () => { try { await run(); if (status) status.textContent = 'ready'; } catch { if (status) status.textContent = 'could not share the image; try downloading it'; } };
  const download = document.querySelector<HTMLButtonElement>('[data-download]');
  if (download) download.onclick = act(async () => { const blob = await png(canvas); const url = URL.createObjectURL(blob); const link = document.createElement('a'); link.href = url; link.download = 'cornercase-stats.png'; link.click(); setTimeout(() => URL.revokeObjectURL(url), 1000); });
  const copy = document.querySelector<HTMLButtonElement>('[data-copy-image]');
  if (copy && typeof ClipboardItem !== 'undefined' && navigator.clipboard?.write) {
    copy.hidden = false;
    copy.onclick = async () => { try { await navigator.clipboard.write([new ClipboardItem({ 'image/png': png(canvas) })]); if (status) status.textContent = 'copied'; } catch { if (status) status.textContent = 'could not copy the image; try downloading it'; } };
  }
  const share = document.querySelector<HTMLButtonElement>('[data-share-image]');
  const file = new File([await png(canvas)], 'cornercase-stats.png', { type: 'image/png' });
  if (share && navigator.canShare?.({ files: [file] })) {
    share.hidden = false;
    share.onclick = act(async () => { const file = new File([await png(canvas)], 'cornercase-stats.png', { type: 'image/png' }); await navigator.share({ files: [file], text: postText(stats, state.window), url: location.href }); });
  }
}
void mount();
window.addEventListener('hashchange', () => { void mount(); });
