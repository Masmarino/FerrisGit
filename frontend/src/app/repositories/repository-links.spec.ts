import { TreeEntry } from './repositories.service';
import { pathBreadcrumb, repositoryLink, sortEntries } from './repository-links';
import { repositoryFixture } from './repository-fixtures';

describe('repositoryLink', () => {
  it('points at a folder or a file of a ref', () => {
    expect(repositoryLink(['acme', 'widget'], 'tree', 'main')).toEqual(['/repositories', 'acme', 'widget', '-', 'tree', 'main']);
    expect(repositoryLink(['acme', 'widget'], 'blob', 'v1', ['src', 'lib.rs'])).toEqual(['/repositories', 'acme', 'widget', '-', 'blob', 'v1', 'src', 'lib.rs']);
  });
});

describe('pathBreadcrumb', () => {
  const repo = repositoryFixture({ path: ['acme', 'widget'] });

  it('is empty until the repository has loaded', () => {
    expect(pathBreadcrumb(null, 'main', ['src'])).toEqual([]);
  });

  it('starts at the repository and stops before the last segment', () => {
    expect(pathBreadcrumb(repo, 'main', ['src', 'app', 'main.ts'])).toEqual([
      { name: 'widget', link: ['/repositories', 'acme', 'widget', '-', 'tree', 'main'] },
      { name: 'src', link: ['/repositories', 'acme', 'widget', '-', 'tree', 'main', 'src'] },
      { name: 'app', link: ['/repositories', 'acme', 'widget', '-', 'tree', 'main', 'src', 'app'] },
    ]);
  });
});

describe('sortEntries', () => {
  const entry = (name: string, isDir: boolean): TreeEntry => ({ name, isDir, lastCommit: null });

  it('puts folders first, then sorts by name without touching the input', () => {
    const entries = [entry('b.txt', false), entry('src', true), entry('a.txt', false), entry('docs', true)];
    expect(sortEntries(entries).map((e) => e.name)).toEqual(['docs', 'src', 'a.txt', 'b.txt']);
    expect(entries[0].name).toBe('b.txt');
  });
});
