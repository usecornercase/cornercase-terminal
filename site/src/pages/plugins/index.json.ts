import type { APIRoute } from 'astro';
import source from '../../../plugins/index.json?raw';
import { validate } from '../../lib/plugins/catalogue';

export const GET: APIRoute = () =>
  new Response(JSON.stringify(validate(source)), { headers: { 'Content-Type': 'application/json' } });
