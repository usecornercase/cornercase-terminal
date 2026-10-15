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
  const font = (name) => pathToFileURL(join(root, `site/node_modules/@fontsource-variable/${name}/files/${name}-latin-wght-normal.woff2`));
  const fonts = `@font-face{font-family:Heading;src:url('${font('bricolage-grotesque')}');font-weight:100 900}@font-face{font-family:Body;src:url('${font('geist')}');font-weight:100 900}@font-face{font-family:Mono;src:url('${font('jetbrains-mono')}');font-weight:100 900}`;
  for (const name of ['readme', 'og']) {
    const readme = name === 'readme';
    const width = readme ? 1600 : 1200;
    const height = readme ? 840 : 630;
    const scale = readme ? 1.5 : 1;
    const { svg } = snapshot(name, 'cornercase with its projects, workspaces and agents');
    const page = join(scratch, `${name}.html`);
    writeFileSync(page, `<!doctype html><meta charset="utf-8"><style>${fonts}
      *{box-sizing:border-box}html,body{margin:0;background:transparent}body{width:${width}px;height:${height}px;font-family:Body;color:#f3efff}
      .banner{position:relative;width:100%;height:100%;overflow:hidden;border-radius:${readme ? 22 : 0}px;background:radial-gradient(ellipse at 0 0,#342454 0,transparent 54%),radial-gradient(ellipse at 85% 100%,#241939 0,transparent 54%),#09080e}
      .banner:before{content:'';position:absolute;inset:0;background-image:radial-gradient(#85729b55 .8px,transparent 1px);background-size:24px 24px;opacity:.35}
      .brand{position:absolute;left:${readme ? 112 : 64}px;top:${readme ? 150 : 94}px;display:flex;gap:${readme ? 18 : 12}px;align-items:center;font-size:${readme ? 32 : 26}px;font-weight:650;letter-spacing:-.4px}
      .logo{position:relative;width:${readme ? 40 : 30}px;height:${readme ? 40 : 30}px;background:${readme ? 'transparent' : '#0b0912'};border-radius:4px}.logo:before{content:'';position:absolute;left:${readme ? 0 : 8}px;top:${readme ? 10 : 12}px;width:${readme ? 10 : 4}px;height:${readme ? 30 : 12}px;background:#895bff}.logo:after{content:'';position:absolute;left:${readme ? 10 : 12}px;top:${readme ? 0 : 8}px;width:${readme ? 30 : 12}px;height:${readme ? 10 : 4}px;background:#895bff}
      h1{position:absolute;left:${readme ? 112 : 64}px;top:${readme ? 262 : 164}px;margin:0;font-family:Heading;font-weight:760;font-size:${readme ? 88 : 56}px;line-height:1.02;letter-spacing:${readme ? -3.5 : -2.8}px}h1 b{font-weight:inherit;background:linear-gradient(90deg,#a780ff,${readme ? '#c1a2ff' : '#6fdced'});color:transparent;background-clip:text}
      .pitch{position:absolute;left:${readme ? 112 : 64}px;top:${readme ? 572 : 377}px;font-size:${readme ? 28 : 22}px;line-height:${readme ? 40 : 34}px;letter-spacing:-.4px;color:#c0bbcf}
      .pills{position:absolute;left:${readme ? 112 : 64}px;top:${readme ? 694 : 474}px;display:flex;gap:16px;font: ${readme ? 20 : 14}px Mono}.pills span{border:1px solid #746a874d;border-radius:100px;padding:${readme ? '15px 23px' : '12px 18px'};background:#ffffff03}
      .terminal{position:absolute;left:${readme ? 990 : 690}px;top:${readme ? 105 : 90}px;background:#1a1b26;border:1px solid #41425980;border-radius:18px;overflow:hidden;${readme ? 'transform:perspective(1600px) rotateY(-7deg);transform-origin:left bottom;' : ''}}
      .top{height:${readme ? 34 : 36}px;border-bottom:1px solid #41425955;background:#181824;display:flex;align-items:center;gap:8px;padding:0 16px;color:#656d98;font:12px Mono}.top i{width:11px;height:11px;background:#3d405e;border-radius:100px}.top em{font-style:normal;flex:1;text-align:center}
      .screen{padding:${readme ? 10 : 16}px}.screen svg{display:block;width:${readme ? 828 : 560}px;height:auto}.screen svg text{font-family:Mono}
      </style><div class="banner"><div class="brand"><span class="logo"></span>cornercase</div><h1>The fastest way<br>to run agents<br><b>in parallel.</b></h1><div class="pitch">Every agent in its own corner. Everything a click away.<br>Close the terminal, they keep working.</div><div class="pills"><span>Linux &amp; macOS</span><span>Rust</span><span>open source</span></div><div class="terminal"><div class="top"><i></i><i></i><i></i>${readme ? '<em>cornercase — ~/code/web-shop</em>' : ''}</div><div class="screen">${svg}</div></div></div>`);
    run(browser, ['--headless', '--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage', '--disable-background-networking', '--hide-scrollbars', '--default-background-color=00000000', `--force-device-scale-factor=${scale}`, `--window-size=${width},${height}`, '--virtual-time-budget=3000', `--screenshot=${join(root, `site/public/${name}.png`)}`, pathToFileURL(page).href]);
    process.stdout.write(`captured ${name}.png\n`);
  }
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
