import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import fr from '../../../../public/i18n/fr.json';
import { t } from './translator';

/**
 * The French file is the reference: every key the code names exists in it, and every key in it is used. Keys built at
 * run time (`t(\`issues.state.${state}\`)`) cannot be found in the source, so their prefixes are listed here.
 */
const DYNAMIC_PREFIXES: string[] = [
  // notification-display.ts: the role a notification names.
  'notifications.roles.',
  // quick-search.ts: the search words of each item.
  'shell.quickSearch.keywords.',
];

const SOURCE_ROOT = join(process.cwd(), 'src', 'app');

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) {
      return sourceFiles(path);
    }
    return /\.(ts|html)$/.test(name) && !/\.(spec|stories)\.ts$/.test(name) ? [path] : [];
  });
}

function leafKeys(node: unknown, prefix = ''): string[] {
  if (typeof node === 'string') {
    return [prefix];
  }
  return Object.entries(node as Record<string, unknown>).flatMap(([key, child]) => leafKeys(child, prefix ? `${prefix}.${key}` : key));
}

const topLevel = Object.keys(fr);
const keyLiteral = topLevel.length === 0 ? null : new RegExp(`['"\`]((?:${topLevel.join('|')})(?:\\.[\\w]+)+)['"\`]`, 'g');
const sources = sourceFiles(SOURCE_ROOT).map((path) => readFileSync(path, 'utf8'));
const referenced = new Set(keyLiteral ? sources.flatMap((source) => [...source.matchAll(keyLiteral)].map((match) => match[1])) : []);
const defined = new Set(leafKeys(fr));
const plural = /_(one|other)$/;
const isDynamic = (key: string) => DYNAMIC_PREFIXES.some((prefix) => key.startsWith(prefix));

describe('translations (fr.json)', () => {
  it('defines every key the code refers to', () => {
    const missing = [...referenced].filter((key) => !defined.has(key) && !defined.has(`${key}_one`) && !isDynamic(key));

    expect(missing).toEqual([]);
  });

  it('has no key that nothing refers to', () => {
    const unused = [...defined].filter((key) => !referenced.has(key) && !referenced.has(key.replace(plural, '')) && !isDynamic(key));

    expect(unused).toEqual([]);
  });

  it('leaves no value empty', () => {
    const lookup = (key: string) => key.split('.').reduce<unknown>((node, part) => (node as Record<string, unknown>)[part], fr) as string;

    expect([...defined].filter((key) => lookup(key).trim() === '')).toEqual([]);
  });
});
