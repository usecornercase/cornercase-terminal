import { fromFragment } from './controller';
import { ERROR, postText, type WindowName } from './model';
import { TICKET } from './pose';
import { HALO, SHADOW, blurred, render, textures } from './stage';
import { Tilt } from './tilt';

const card = document.querySelector<HTMLElement>('[data-stats]');
const error = document.querySelector<HTMLElement>('[data-error]');
const need = <T extends Element>(selector: string): T => { const found = document.querySelector<T>(selector); if (!found) throw new Error(`missing ${selector}`); return found; };
const png = (canvas: HTMLCanvasElement): Promise<Blob> => new Promise((resolve, reject) => canvas.toBlob((blob) => blob ? resolve(blob) : reject(new Error('could not make PNG')), 'image/png'));
const SCALE = 2;

let mounts = 0;
let tilt: Tilt | null = null;
let masks: string[] = [];

async function fonts(): Promise<void> {
  await Promise.all(['800 64px "Archivo Variable"', '400 20px "Archivo Variable"', '600 20px "Archivo Variable"', '400 20px "JetBrains Mono Variable"', '500 20px "JetBrains Mono Variable"'].map((font) => document.fonts.load(font)));
}


async function mount(): Promise<void> {
  const current = ++mounts;
  const stale = () => current !== mounts;
  if (!card || !error) return;
  const state = fromFragment(location.hash);
  if (!state) { tilt?.stop(); tilt = null; card.hidden = true; error.hidden = false; error.textContent = ERROR; return; }
  card.hidden = false; error.hidden = true;
  const { stats, example } = state;
  const note = document.querySelector<HTMLElement>('[data-example]'); if (note) note.hidden = !example;
  await fonts();
  if (stale()) return;
  const preview = new URLSearchParams(location.search).has('preview');
  const stage = need<HTMLElement>('[data-stage]');
  const faceCanvas = need<HTMLCanvasElement>('[data-face]');
  const shadow = need<HTMLCanvasElement>('[data-shadow]');
  const halo = need<HTMLCanvasElement>('[data-halo]');
  const foils = [...document.querySelectorAll<HTMLElement>('[data-foil]')];
  const glare = need<HTMLElement>('[data-glare]');
  for (const [element, b] of [[shadow, SHADOW], [halo, HALO]] as const) {
    const picture = blurred(b); element.width = picture.width; element.height = picture.height; element.getContext('2d')?.drawImage(picture, 0, 0);
    element.style.width = `${picture.width / b.texel}px`; element.style.height = `${picture.height / b.texel}px`;
  }
  tilt?.stop();
  tilt = new Tilt({ stage, space: need('[data-space]'), backdrop: need('[data-backdrop]'), shadow, halo, ticket: need('[data-ticket]'), foils, glare });
  const tabs = [...document.querySelectorAll<HTMLButtonElement>('[data-window]')];
  const x = document.querySelector<HTMLAnchorElement>('[data-x]');
  const linkedin = document.querySelector<HTMLAnchorElement>('[data-linkedin]');
  let turns = 0;
  const draw = async () => {
    if (stale()) return;
    const turn = ++turns;
    const shown = state.face();
    const t = textures(shown, SCALE);
    const layers: [HTMLElement, HTMLCanvasElement][] = [[glare, t.shape], ...foils.map((layer): [HTMLElement, HTMLCanvasElement] => [layer, t.foils[layer.dataset.foil ?? '']])];
    const made = (await Promise.all(layers.map(([, source]) => png(source)))).map((blob) => URL.createObjectURL(blob));
    if (stale() || turn !== turns) { made.forEach((url) => URL.revokeObjectURL(url)); return; }
    faceCanvas.width = TICKET.w * SCALE; faceCanvas.height = TICKET.h * SCALE;
    faceCanvas.getContext('2d')?.drawImage(t.face, 0, 0);
    layers.forEach(([element], i) => { for (const name of ['mask-image', '-webkit-mask-image']) element.style.setProperty(name, `url("${made[i]}")`); });
    masks.forEach((url) => URL.revokeObjectURL(url)); masks = made;
    tabs.forEach((tab) => tab.setAttribute('aria-selected', String(tab.dataset.window === state.window)));
    stage.setAttribute('aria-label', `cornercase stats ticket: ${shown.parallel ? `${shown.hero} in parallel, ${shown.work} of agent work in ${shown.busy}` : shown.hero}, ${shown.window}`);
    const publicUrl = new URL(location.href); publicUrl.search = ''; const url = publicUrl.href; const text = postText(stats, state.window);
    if (x) x.href = `https://twitter.com/intent/tweet?text=${encodeURIComponent(text)}&url=${encodeURIComponent(url)}`;
    if (linkedin) linkedin.href = `https://www.linkedin.com/sharing/share-offsite/?url=${encodeURIComponent(url)}`;
    const written = document.querySelector<HTMLElement>('[data-post-text]'); if (written) written.textContent = text;
  };
  await draw();
  const image = (): Promise<Blob> => {
    const out = document.createElement('canvas');
    render(out, state.face(), { height: preview ? 630 : 675, scale: SCALE });
    return png(out);
  };
  tabs.forEach((tab) => { tab.onclick = () => { state.select(tab.dataset.window as WindowName); void draw(); }; });
  const status = document.querySelector<HTMLElement>('[data-status]');
  const act = (run: () => Promise<void>) => async () => {
    if (stale()) return;
    try {
      await run();
      if (stale()) return;
      if (status) status.textContent = 'ready';
    } catch {
      if (stale()) return;
      if (status) status.textContent = 'could not share the image; try downloading it';
    }
  };
  const download = document.querySelector<HTMLButtonElement>('[data-download]');
  if (download) download.onclick = act(async () => {
    const blob = await image();
    if (stale()) return;
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a'); link.href = url; link.download = 'cornercase-stats.png'; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  });
  const copy = document.querySelector<HTMLButtonElement>('[data-copy-image]');
  if (copy && typeof ClipboardItem !== 'undefined' && navigator.clipboard?.write) {
    copy.hidden = false;
    copy.onclick = async () => {
      if (stale()) return;
      try {
        await navigator.clipboard.write([new ClipboardItem({ 'image/png': image() })]);
        if (stale()) return;
        if (status) status.textContent = 'copied';
      } catch {
        if (stale()) return;
        if (status) status.textContent = 'could not copy the image; try downloading it';
      }
    };
  }
  const share = document.querySelector<HTMLButtonElement>('[data-share-image]');
  if (share && navigator.canShare?.({ files: [new File([''], 'cornercase-stats.png', { type: 'image/png' })] })) {
    share.hidden = false;
    share.onclick = act(async () => {
      const blob = await image();
      if (stale()) return;
      const file = new File([blob], 'cornercase-stats.png', { type: 'image/png' });
      await navigator.share({ files: [file], text: postText(stats, state.window), url: location.href });
      if (stale()) return;
    });
  }
}
void mount();
window.addEventListener('hashchange', () => { void mount(); });
