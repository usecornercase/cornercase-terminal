import * as en from './en';
import * as es from './es';
import * as ptBr from './pt-br';
import type { Page } from './en';

export type { Page };

export const LOCALES = {
  en: { path: '', lang: 'en', og: 'en_US', short: 'EN', name: 'English', page: en.page },
  es: { path: 'es/', lang: 'es', og: 'es_ES', short: 'ES', name: 'Español', page: es.page },
  'pt-br': { path: 'pt-br/', lang: 'pt-BR', og: 'pt_BR', short: 'PT', name: 'Português (Brasil)', page: ptBr.page },
};

export type Locale = keyof typeof LOCALES;

export const locales = Object.keys(LOCALES) as Locale[];

const base = import.meta.env.BASE_URL.replace(/\/$/, '');

export function localeOf(url: URL): Locale {
  const path = url.pathname.slice(base.length).replace(/^\//, '');
  return locales.find((l) => l !== 'en' && path.startsWith(LOCALES[l].path)) ?? 'en';
}

export const home = (locale: Locale): string => `${base}/${LOCALES[locale].path}`;

export const words = (url: URL): Page => LOCALES[localeOf(url)].page;

export type Part = string | { code: string } | { status: 'working' | 'waiting' | 'done' };

export function rich(text: string): Part[] {
  return text
    .split(/(`[^`]+`|\{(?:working|waiting|done)\})/)
    .filter((s) => s !== '')
    .map((s): Part => {
      if (s.startsWith('`')) return { code: s.slice(1, -1) };
      if (s === '{working}' || s === '{waiting}' || s === '{done}') return { status: s.slice(1, -1) as 'working' | 'waiting' | 'done' };
      return s;
    });
}

export const GLYPHS = { working: '◐', waiting: '!', done: '✓' };
