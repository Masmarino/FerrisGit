import { branchPatternProblem } from './branch-pattern';

describe('branchPatternProblem', () => {
  it('accepts branch names and * patterns, and says nothing about an empty one', () => {
    for (const pattern of ['', 'main', 'release/*', '*-stable', '*', 'feature/a.b']) {
      expect(branchPatternProblem(pattern), pattern).toBeNull();
    }
  });

  it('refuses what the server refuses', () => {
    for (const pattern of ['refs/heads/main', '-x', '/x', 'x/', 'a..b', 'a//b', 'x.lock', 'a b', 'a~1', 'a^', 'a:b', 'a?', 'a[b', 'a\\b', 'a'.repeat(256)]) {
      expect(branchPatternProblem(pattern), pattern).toBe("Un nom de branche, sans refs/heads/ ni espace ; * remplace n'importe quelle suite de caractères.");
    }
  });
});
