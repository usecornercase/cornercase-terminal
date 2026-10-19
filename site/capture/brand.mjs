import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(fileURLToPath(new URL('../..', import.meta.url)));
const scratch = mkdtempSync(join(tmpdir(), 'cornercase-brand-'));
const browser = process.env.CORNERCASE_CAPTURE_BROWSER || 'chromium';
const run = (command, args) => {
  const result = spawnSync(command, args, { encoding: 'utf8' });
  if (result.status !== 0) throw new Error(result.stderr || result.error?.message || `${command} failed`);
};

try {
  const bundle = join(scratch, 'snapshot.mjs');
  run(join(root, 'site/node_modules/.bin/esbuild'), [join(root, 'site/src/lib/demo/snapshot.ts'), '--bundle', '--platform=node', '--format=esm', '--loader:.png=empty', `--outfile=${bundle}`]);
  const { snapshot } = await import(pathToFileURL(bundle));
  const font = (name, axis = 'wght') => pathToFileURL(join(root, `site/node_modules/@fontsource-variable/${name}/files/${name}-latin-${axis}-normal.woff2`));
  const fonts = `@font-face{font-family:Archivo;src:url('${font('archivo', 'wdth')}');font-weight:100 900;font-stretch:62% 125%}@font-face{font-family:Mono;src:url('${font('jetbrains-mono')}');font-weight:100 900}`;
  const mark = (size) => `<svg viewBox="0 0 12 12" width="${size}" height="${size}" shape-rendering="crispEdges"><path fill="#005fff" d="M3 0h9v3H3zM0 3h3v9H0z"/><path fill="#0c0e11" d="M6 6h2v5H6z"/></svg>`;
  for (const name of ['readme', 'og']) {
    const readme = name === 'readme';
    const width = readme ? 1600 : 1200;
    const height = readme ? 840 : 630;
    const scale = readme ? 1.5 : 1;
    const left = readme ? 112 : 64;
    const { svg } = snapshot(name, 'cornercase with its projects, workspaces and agents');
    const page = join(scratch, `${name}.html`);
    writeFileSync(page, `<!doctype html><meta charset="utf-8"><style>${fonts}
      *{box-sizing:border-box}html,body{margin:0;background:#f1f2f4}body{width:${width}px;height:${height}px;font-family:Archivo;color:#0c0e11}
      .banner{position:relative;width:100%;height:100%;overflow:hidden;background:#f1f2f4}
      .brand{position:absolute;left:${left}px;top:${readme ? 132 : 70}px;display:flex;gap:${readme ? 16 : 12}px;align-items:center;font-size:${readme ? 34 : 26}px;font-weight:760;font-stretch:112%;letter-spacing:-.03em}
      h1{position:absolute;left:${left}px;top:${readme ? 250 : 150}px;margin:0;font-weight:780;font-stretch:112%;font-size:${readme ? 84 : 58}px;line-height:1;letter-spacing:-.04em}
      .pitch{position:absolute;left:${left}px;top:${readme ? 572 : 385}px;width:${readme ? 760 : 560}px;font-size:${readme ? 28 : 22}px;line-height:1.45;color:#3c424a}
      .strip{position:absolute;left:${left}px;top:${readme ? 704 : 498}px;font-size:${readme ? 22 : 17}px;font-weight:600;color:#0050d8}
      .terminal{position:absolute;left:${readme ? 1000 : 700}px;top:${readme ? 132 : 70}px;background:#1a1b26;border:1px solid #262838}
      .terminal:before,.terminal:after{content:'';position:absolute;background:#005fff}
      .terminal:before{left:0;top:-${readme ? 22 : 16}px;width:${readme ? 220 : 150}px;height:${readme ? 9 : 6}px}
      .terminal:after{left:-${readme ? 22 : 16}px;top:0;width:${readme ? 9 : 6}px;height:${readme ? 220 : 150}px}
      .top{height:${readme ? 36 : 34}px;border-bottom:1px solid #ffffff14;background:#13141c;display:flex;align-items:center;padding:0 16px;color:#8c93b1;font:${readme ? 13 : 12}px Mono}
      .screen{padding:${readme ? 12 : 14}px}.screen svg{display:block;width:${readme ? 828 : 560}px;height:auto}.screen svg text{font-family:Mono}
      </style><div class="banner"><div class="brand">${mark(readme ? 40 : 30)}cornercase</div><h1>The fastest way<br>to run agents<br>in parallel.</h1><div class="pitch">Every agent in its own corner. Everything a click away. Close the terminal, they keep working.</div><div class="strip">Free and open source, for Linux and macOS.</div><div class="terminal"><div class="top">${readme ? 'cornercase · ~/code/web-shop' : 'cornercase'}</div><div class="screen">${svg}</div></div></div>`);
    run(browser, ['--headless', '--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage', '--disable-background-networking', '--hide-scrollbars', `--force-device-scale-factor=${scale}`, `--window-size=${width},${height}`, '--virtual-time-budget=3000', `--screenshot=${join(root, `site/public/${name}.png`)}`, pathToFileURL(page).href]);
    process.stdout.write(`captured ${name}.png\n`);
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
