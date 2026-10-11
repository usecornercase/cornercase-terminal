import type { Locale } from '.';
import { type Demo, demo as en } from './en';
import { demo as es } from './es';
import { demo as ptBr } from './pt-br';

export const DEMO: Record<Locale, Demo> = { en, es, 'pt-br': ptBr };

export const demoWords = (locale: string | undefined): Demo => DEMO[(locale ?? 'en') as Locale] ?? en;
