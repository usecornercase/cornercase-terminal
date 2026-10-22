import type { Style } from '../term/grid';

export interface Icon {
  glyph: string;
  colour: number | null;
}

const icon = (glyph: string, colour: number | null): Icon => ({ glyph, colour });

const FOLDER = icon('\u{e5ff}', 4);
const FOLDER_OPEN = icon('\u{e5fe}', 4);
const GENERIC = icon('\u{e64e}', null);

export const NAMES: [string[], Icon][] = [
  [['cargo.toml'], icon('\u{e68b}', 1)],
  [['cargo.lock', 'package-lock.json', 'pnpm-lock.yaml', 'go.sum'], icon('\u{e672}', null)],
  [['package.json'], icon('\u{e616}', 1)],
  [['tsconfig.json', 'jsconfig.json'], icon('\u{e69d}', 4)],
  [['dockerfile', 'containerfile', 'docker-compose.yml', 'docker-compose.yaml', 'compose.yml', 'compose.yaml', '.dockerignore'], icon('\u{e650}', 4)],
  [['makefile', 'gnumakefile'], icon('\u{e673}', 3)],
  [['.gitignore', '.gitattributes', '.gitmodules', '.gitkeep'], icon('\u{e65d}', 1)],
  [['.env', '.envrc', '.env.local', '.env.example'], icon('\u{e615}', 3)],
  [['readme', 'readme.md'], icon('\u{e66a}', 6)],
  [['license', 'license.md', 'license.txt', 'licence', 'copying'], icon('\u{e60a}', 3)],
  [['.editorconfig'], icon('\u{e652}', null)],
  [['gemfile', 'rakefile'], icon('\u{e605}', 1)],
  [['go.mod'], icon('\u{e627}', 6)],
  [['.bashrc', '.zshrc', '.profile', '.bash_profile'], icon('\u{e691}', 2)],
];

export const EXTENSIONS: [string[], Icon][] = [
  [['rs'], icon('\u{e68b}', 1)],
  [['ts', 'mts', 'cts'], icon('\u{e628}', 4)],
  [['tsx'], icon('\u{e625}', 4)],
  [['js', 'mjs', 'cjs'], icon('\u{e60c}', 3)],
  [['jsx'], icon('\u{e625}', 6)],
  [['py', 'pyi'], icon('\u{e606}', 3)],
  [['go'], icon('\u{e627}', 6)],
  [['rb'], icon('\u{e605}', 1)],
  [['java'], icon('\u{e66d}', 1)],
  [['kt', 'kts'], icon('\u{e634}', 5)],
  [['swift'], icon('\u{e699}', 1)],
  [['c'], icon('\u{e649}', 4)],
  [['h'], icon('\u{e649}', 5)],
  [['cpp', 'cc', 'cxx'], icon('\u{e646}', 4)],
  [['hpp', 'hh', 'hxx'], icon('\u{e646}', 5)],
  [['cs'], icon('\u{e648}', 2)],
  [['php'], icon('\u{e608}', 5)],
  [['lua'], icon('\u{e620}', 4)],
  [['sh', 'bash', 'zsh', 'fish'], icon('\u{e691}', 2)],
  [['toml'], icon('\u{e6b2}', 1)],
  [['yaml', 'yml'], icon('\u{e6a8}', 5)],
  [['json', 'jsonc', 'json5'], icon('\u{e60b}', 3)],
  [['md', 'markdown', 'mdx'], icon('\u{e609}', 4)],
  [['html', 'htm'], icon('\u{e60e}', 1)],
  [['css'], icon('\u{e614}', 4)],
  [['scss', 'sass'], icon('\u{e603}', 5)],
  [['sql', 'db', 'sqlite'], icon('\u{e64d}', 6)],
  [['vue'], icon('\u{e6a0}', 2)],
  [['svelte'], icon('\u{e697}', 1)],
  [['graphql', 'gql'], icon('\u{e662}', 5)],
  [['ex', 'exs'], icon('\u{e62d}', 5)],
  [['hs'], icon('\u{e61f}', 5)],
  [['scala'], icon('\u{e68e}', 1)],
  [['dart'], icon('\u{e64c}', 6)],
  [['zig'], icon('\u{e6a9}', 3)],
  [['tf', 'tfvars'], icon('\u{e69a}', 5)],
  [['vim'], icon('\u{e62b}', 2)],
  [['xml'], icon('\u{e619}', 3)],
  [['svg'], icon('\u{e698}', 3)],
  [['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'ico'], icon('\u{e60d}', 5)],
  [['pdf'], icon('\u{e67d}', 1)],
  [['zip', 'tar', 'gz', 'tgz', 'xz', 'bz2', '7z', 'zst', 'rar'], icon('\u{e6aa}', 3)],
  [['lock', 'lockb'], icon('\u{e672}', null)],
  [['csv', 'tsv'], icon('\u{e64a}', 2)],
  [['ini', 'conf', 'cfg'], icon('\u{e615}', null)],
  [['env'], icon('\u{e615}', 3)],
  [['ttf', 'otf', 'woff', 'woff2'], icon('\u{e659}', null)],
  [['mp3', 'wav', 'flac', 'ogg'], icon('\u{e638}', 6)],
  [['mp4', 'mov', 'webm', 'mkv'], icon('\u{e69f}', 5)],
];

export const SLOT = 2;

function find(table: [string[], Icon][], key: string): Icon | undefined {
  const lower = key.toLowerCase();
  return table.find(([keys]) => keys.includes(lower))?.[1];
}

export function folderIcon(open: boolean): Icon {
  return open ? FOLDER_OPEN : FOLDER;
}

export function fileIcon(name: string): Icon {
  const dot = name.lastIndexOf('.');
  const extension = dot > 0 ? name.slice(dot + 1) : '';
  return find(NAMES, name) ?? (extension ? find(EXTENSIONS, extension) : undefined) ?? GENERIC;
}

export function fileIconAt(path: string): Icon {
  return fileIcon(path.slice(path.lastIndexOf('/') + 1));
}

export function iconStyle(icon: Icon, soft: Style): Style {
  return icon.colour === null ? soft : { fg: icon.colour };
}

export function tableText(): string {
  const text = (kind: string, table: [string[], Icon][]) =>
    table.flatMap(([keys, i]) => keys.map((k) => `${kind} ${k.toLowerCase()} ${i.glyph.codePointAt(0)?.toString(16)} ${NAME_OF[i.colour ?? -1]}`));
  const head = [
    `folder closed ${FOLDER.glyph.codePointAt(0)?.toString(16)} blue`,
    `folder open ${FOLDER_OPEN.glyph.codePointAt(0)?.toString(16)} blue`,
    `file - ${GENERIC.glyph.codePointAt(0)?.toString(16)} soft`,
  ];
  return [...head, ...text('name', NAMES), ...text('ext', EXTENSIONS)].join('\n') + '\n';
}

const NAME_OF: Record<number, string> = { [-1]: 'soft', 1: 'red', 2: 'green', 3: 'yellow', 4: 'blue', 5: 'magenta', 6: 'cyan' };
