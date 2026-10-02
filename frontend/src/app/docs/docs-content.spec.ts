// Checks the real docs in the repository's `docs/` (through the `frontend/docs` symlink) against their index, so a page
// missing from either fails `npm test` rather than a reader.
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { parseDocsPage } from './docs-search.service';
import { DocsIndex, locateDocsPage } from './docs.service';

const ROOT = resolve(process.cwd(), 'docs');
// Working notes that may sit next to the docs.
const NOT_SECTIONS = new Set(['superpowers']);

const index: DocsIndex = JSON.parse(readFileSync(join(ROOT, 'index.json'), 'utf8'));
const pages = index.sections.flatMap((section) => section.pages.map((page) => ({ section, page, file: join(ROOT, section.slug, `${page.slug}.md`) })));
const read = (file: string) => readFileSync(file, 'utf8');

describe('docs/ content', () => {
  it('lists at least one section, each with pages', () => {
    expect(index.sections.length).toBeGreaterThan(0);
    for (const section of index.sections) {
      expect(section.pages.length, section.slug).toBeGreaterThan(0);
    }
  });

  it('uses unique, URL-safe slugs', () => {
    const sections = index.sections.map((section) => section.slug);
    expect(new Set(sections).size, 'section slugs').toBe(sections.length);
    for (const section of index.sections) {
      const slugs = section.pages.map((page) => page.slug);
      expect(new Set(slugs).size, `page slugs of ${section.slug}`).toBe(slugs.length);
      for (const slug of [section.slug, ...slugs]) {
        expect(slug).toMatch(/^[a-z0-9]+(-[a-z0-9]+)*$/);
      }
    }
  });

  it('has a file for every page of the index', () => {
    const missing = pages.filter(({ file }) => !existsSync(file)).map(({ section, page }) => `${section.slug}/${page.slug}.md`);
    expect(missing).toEqual([]);
  });

  it('lists every Markdown file of a section folder in the index', () => {
    const listed = new Set(pages.map(({ section, page }) => `${section.slug}/${page.slug}.md`));
    const folders = readdirSync(ROOT).filter((name) => !NOT_SECTIONS.has(name) && statSync(join(ROOT, name)).isDirectory());
    const files = folders.flatMap((folder) => readdirSync(join(ROOT, folder)).filter((name) => name.endsWith('.md')).map((name) => `${folder}/${name}`));
    expect(files.filter((file) => !listed.has(file))).toEqual([]);
  });

  it('starts every page with its index title as the only level-1 heading', () => {
    for (const { section, page, file } of pages) {
      const text = read(file);
      const name = `${section.slug}/${page.slug}.md`;
      expect(text.split(/\r?\n/)[0], name).toBe(`# ${page.title}`);
      // Code blocks can hold `# comments`, so only count headings outside them.
      let fenced = false;
      const titles = text.split(/\r?\n/).filter((line) => {
        if (/^\s*(```|~~~)/.test(line)) {
          fenced = !fenced;
        }
        return !fenced && /^# /.test(line);
      });
      expect(titles, name).toHaveLength(1);
    }
  });

  it('links only to pages and headings that exist', () => {
    const anchors = new Map(pages.map(({ section, page, file }) => [`${section.slug}/${page.slug}`, new Set(parseDocsPage(read(file)).headings.map((heading) => heading.id))]));
    const broken: string[] = [];
    for (const { section, page, file } of pages) {
      for (const [, target] of read(file).matchAll(/\]\((\/docs[^)\s]*)\)/g)) {
        const [path, fragment] = target.split('#');
        const [, , targetSection, targetPage, ...rest] = path.split('/');
        const key = `${targetSection}/${targetPage}`;
        const exists = rest.length === 0 && locateDocsPage(index, targetSection ?? null, targetPage ?? null) !== null;
        const anchorOk = !fragment || anchors.get(key)?.has(`user-content-${decodeURIComponent(fragment)}`);
        if (!exists || !anchorOk) {
          broken.push(`${section.slug}/${page.slug}.md → ${target}`);
        }
      }
    }
    expect(broken).toEqual([]);
  });
});
