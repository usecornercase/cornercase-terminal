const DEV_SOCKETS = 'ws://localhost:* ws://127.0.0.1:* http://localhost:* http://127.0.0.1:*';

export function policy(images: string, dev = false): string {
  return [
    "default-src 'none'",
    dev ? "script-src 'self' 'unsafe-inline'" : "script-src 'self'",
    dev ? "style-src 'self' 'unsafe-inline'" : "style-src 'self'",
    "font-src 'self'",
    `img-src ${images}`,
    dev ? `connect-src 'self' ${DEV_SOCKETS}` : "connect-src 'none'",
    "base-uri 'none'",
    "form-action 'none'",
  ].join('; ');
}
