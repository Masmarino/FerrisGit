import { TestBed } from '@angular/core/testing';
import { of, throwError } from 'rxjs';
import { HttpErrorResponse } from '@angular/common/http';
import { DOCS_SEARCH_LIMIT, DocsSearchService, parseDocsPage, searchTokens } from './docs-search.service';
import { DocsIndex, DocsService } from './docs.service';
import { DOCS_INDEX, fakeDocsService } from './docs-fixtures';

describe('DocsSearchService', () => {
  function setup(docs = fakeDocsService()) {
    const index = vi.fn(docs.index);
    const page = vi.fn(docs.page);
    TestBed.configureTestingModule({ providers: [{ provide: DocsService, useValue: { index, page } }] });
    return { search: TestBed.inject(DocsSearchService), index, page };
  }

  it('fetches nothing before the first search, then every page once', async () => {
    const { search, page } = setup();
    expect(page).not.toHaveBeenCalled();

    await search.search('pipeline');
    await search.search('docker');

    expect(page).toHaveBeenCalledTimes(5);
  });

  it('finds a word whatever its case and accents', async () => {
    const { search } = setup();

    const hits = await search.search('PRESENTATION');

    expect(hits[0].pageTitle).toBe('Présentation');
    expect(hits[0].sectionTitle).toBe('Démarrer');
    expect(hits[0].commands).toEqual(['/docs', 'demarrer', 'presentation']);
  });

  it('ranks a page by its title before a heading, and a heading before the text', async () => {
    const docs: DocsIndex = {
      sections: [
        {
          slug: 's',
          title: 'Section',
          pages: [
            { slug: 'texte', title: 'Texte', description: '' },
            { slug: 'titre', title: 'Runners', description: '' },
            { slug: 'intertitre', title: 'Intertitre', description: '' },
          ],
        },
      ],
    };
    const pages: Record<string, string> = {
      's/texte': '# Texte\n\nOn parle des runners ici.',
      's/titre': '# Runners\n\nRien de plus.',
      's/intertitre': '# Intertitre\n\n## Les runners\n\nRien.',
    };
    const { search } = setup(fakeDocsService({ index: () => of(docs), page: (key) => of(pages[key]) }));

    const hits = await search.search('runners');

    expect(hits.map((hit) => hit.pageTitle)).toEqual(['Runners', 'Intertitre', 'Texte']);
  });

  it('requires every word of the query', async () => {
    const { search } = setup();

    expect((await search.search('variables chiffrées')).map((hit) => hit.pageTitle)).toEqual(['Référence de .ferrisgit-ci.yml']);
    expect(await search.search('variables kubernetes')).toEqual([]);
  });

  it('lands on the heading that matched, with the id the rendered page gives it', async () => {
    const { search } = setup();

    const [hit] = await search.search('prédéfinies');

    expect(hit.heading).toBe('Variables prédéfinies');
    expect(hit.fragment).toBe('user-content-variables-prédéfinies');
  });

  it('shows the text around the match, accents as written', async () => {
    const { search } = setup();

    const [hit] = await search.search('affichee');

    expect(hit.excerpt.match).toBe('affichée');
    expect(hit.excerpt.before).toMatch(/^….* chiffrée n'est jamais $/);
    expect(hit.excerpt.after.startsWith(' dans les journaux')).toBe(true);
  });

  it('falls back to the description when only the title matches', async () => {
    const { search } = setup();

    const [hit] = await search.search('installation');

    expect(hit.pageTitle).toBe('Installation');
    expect(hit.excerpt).toEqual({ before: 'Démarrer FerrisGit avec Docker Compose.', match: '', after: '' });
  });

  it(`returns at most ${DOCS_SEARCH_LIMIT} results`, async () => {
    const many: DocsIndex = { sections: [{ slug: 's', title: 'S', pages: Array.from({ length: 12 }, (_, i) => ({ slug: `p${i}`, title: `Page ${i}`, description: '' })) }] };
    const { search } = setup(fakeDocsService({ index: () => of(many), page: () => of('# Page\n\nUn runner.') }));

    expect(await search.search('runner')).toHaveLength(DOCS_SEARCH_LIMIT);
  });

  it('leaves out a page that fails to load, and searches the others', async () => {
    const { search } = setup(
      fakeDocsService({ page: (key) => (key === 'ci-cd/reference-yaml' ? throwError(() => new HttpErrorResponse({ status: 500 })) : fakeDocsService().page(...(key.split('/') as [string, string]))) }),
    );

    expect((await search.search('variables')).map((hit) => hit.pageTitle)).toEqual([]);
    expect((await search.search('docker')).map((hit) => hit.pageTitle)).toEqual(['Installation']);
  });

  it('fails when the index cannot be read, and tries again on the next search', async () => {
    let calls = 0;
    const { search } = setup(fakeDocsService({ index: () => (++calls === 1 ? throwError(() => new HttpErrorResponse({ status: 503 })) : of(DOCS_INDEX)) }));

    await expect(search.search('docker')).rejects.toBeTruthy();
    expect((await search.search('docker')).map((hit) => hit.pageTitle)).toEqual(['Installation']);
  });

  it('answers nothing for a query without words', async () => {
    const { search, page } = setup();

    expect(await search.search('  ?! ')).toEqual([]);
    expect(page).not.toHaveBeenCalled();
  });
});

describe('searchTokens', () => {
  it('lower-cases, drops accents and punctuation, and keeps each word once', () => {
    expect(searchTokens('Référence  .ferrisgit-ci.yml référence')).toEqual(['reference', 'ferrisgit', 'ci', 'yml']);
  });
});

describe('parseDocsPage', () => {
  it('keeps the text of the body without the Markdown marks', () => {
    const { body } = parseDocsPage('# Titre\n\nDu **gras**, du `code` et [un lien](/docs/a/b).\n\n> **Note** : une remarque.\n\n- un point\n1. une étape');

    expect(body).toBe('Du gras, du code et un lien. Note : une remarque. un point une étape');
  });

  it('keeps the inside of code blocks, not their fences, and never reads a # in them as a heading', () => {
    const { headings, body } = parseDocsPage('# Titre\n\n```bash\n# un commentaire\ncargo test\n```\n\n## Suite');

    expect(body).toBe('# un commentaire cargo test');
    expect(headings.map((heading) => heading.text)).toEqual(['Suite']);
  });

  it('numbers repeated headings like the rendered page, the title counting too', () => {
    const { headings } = parseDocsPage('# Exemples\n\n## Exemples\n\n## Rust\n\n### Rust\n\n##### Trop profond');

    expect(headings).toEqual([
      { id: 'user-content-exemples-2', text: 'Exemples' },
      { id: 'user-content-rust', text: 'Rust' },
      { id: 'user-content-rust-2', text: 'Rust' },
    ]);
  });

  it('reads a table as its cells, without the separator row', () => {
    expect(parseDocsPage('| Clé | Rôle |\n| --- | --- |\n| `stages` | L’ordre |').body).toBe('Clé Rôle stages L’ordre');
  });
});
