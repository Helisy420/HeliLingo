// The HeliLingo app's local API (src-tauri/src/api.rs): the port and the
// secret token are in %LOCALAPPDATA%\HeliLingo\api.json, written when the
// app starts (Settings → «Контекст и режимы» → «Расширение VS Code»). The
// file is read again on every call: the port changes with each start.

import { readFile } from 'node:fs/promises';
import { request } from 'node:http';
import { join } from 'node:path';

export type Translation = { text: string; sourceLang: string | null; target: string; engine: string };

export class NotRunning extends Error {}

type Info = { port: number; token: string };

async function info(): Promise<Info> {
  const dir = process.env.LOCALAPPDATA;
  if (!dir) throw new NotRunning('no LOCALAPPDATA');
  try {
    const raw = await readFile(join(dir, 'HeliLingo', 'api.json'), 'utf8');
    const j = JSON.parse(raw) as Info;
    if (!j.port || !j.token) throw new Error('incomplete');
    return j;
  } catch {
    throw new NotRunning('api.json');
  }
}

function call<T>(method: 'GET' | 'POST', path: string, body?: unknown): Promise<T> {
  return info().then(
    ({ port, token }) =>
      new Promise<T>((resolve, reject) => {
        const data = body === undefined ? undefined : Buffer.from(JSON.stringify(body), 'utf8');
        const req = request(
          {
            host: '127.0.0.1',
            port,
            path,
            method,
            timeout: 30_000,
            headers: {
              Authorization: `Bearer ${token}`,
              ...(data ? { 'Content-Type': 'application/json', 'Content-Length': data.length } : {}),
            },
          },
          (res) => {
            const chunks: Buffer[] = [];
            res.on('data', (c: Buffer) => chunks.push(c));
            res.on('end', () => {
              const text = Buffer.concat(chunks).toString('utf8');
              let json: unknown;
              try {
                json = JSON.parse(text);
              } catch {
                return reject(new Error(text || `HTTP ${res.statusCode}`));
              }
              if (res.statusCode === 200) resolve(json as T);
              else reject(new Error(describe((json as { error?: unknown }).error) ?? `HTTP ${res.statusCode}`));
            });
          },
        );
        // The app isn't running (a stale api.json): ECONNREFUSED.
        req.on('error', (e: NodeJS.ErrnoException) => reject(e.code === 'ECONNREFUSED' ? new NotRunning(e.message) : e));
        req.on('timeout', () => req.destroy(new Error('timeout')));
        if (data) req.write(data);
        req.end();
      }),
  );
}

/** An engine error from the app ({kind: 'network'} …) as a word. */
function describe(e: unknown): string | undefined {
  if (!e) return undefined;
  if (typeof e === 'string') return e;
  if (typeof e === 'object' && 'kind' in e) return String((e as { kind: unknown }).kind);
  return JSON.stringify(e);
}

export const status = () => call<{ version: string; target: string; secondLang: string }>('GET', '/status');

export type Options = { target?: string; source?: string; engine?: string };

export const translate = (text: string, o: Options = {}) =>
  call<Translation>('POST', '/translate', {
    text,
    target: o.target || undefined,
    source: o.source && o.source !== 'auto' ? o.source : undefined,
    engine: o.engine || undefined,
  });

/** The app's switched-on translators (offline models included). */
export const engines = () => call<{ id: string; name: string; group: string }[]>('GET', '/engines');
