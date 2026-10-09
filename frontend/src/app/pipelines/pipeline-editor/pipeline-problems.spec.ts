import { describeProblem, describeWarning } from './pipeline-problems';

describe('pipeline problems in French', () => {
  it('names the job and the stage that do not match', () => {
    expect(describeProblem({ code: 'unknown_stage', message: 'x', job: 'compile', stage: 'nowhere' })).toEqual({
      message: "Le job «\u00a0compile\u00a0» est dans l'étape «\u00a0nowhere\u00a0», qui n'existe pas.",
      job: 'compile',
    });
  });

  it('names the dependency that is missing or too late', () => {
    expect(describeProblem({ code: 'unknown_dependency', message: 'x', job: 'b', dependency: 'ghost' }).message).toBe("Le job «\u00a0b\u00a0» dépend de «\u00a0ghost\u00a0», qui n'existe pas.");
    expect(describeProblem({ code: 'needs_must_precede_own_stage', message: 'x', job: 'a', dependency: 'z' }).message).toBe('Le job «\u00a0a\u00a0» dépend de «\u00a0z\u00a0», qui est dans une étape ultérieure.');
  });

  it('draws a cycle as a loop and points at its first job', () => {
    expect(describeProblem({ code: 'needs_cycle', message: 'x', jobs: ['a', 'b'] })).toEqual({ message: 'Des dépendances forment une boucle : a → b → a.', job: 'a' });
  });

  it('explains what a valid cache key is', () => {
    expect(describeProblem({ code: 'invalid_cache_key', message: 'x', job: 'a', key: 'Bad Key' }).message).toContain('minuscules, chiffres et tirets seulement');
  });

  it("keeps the parser's detail for malformed YAML, without its English prefix", () => {
    const view = describeProblem({ code: 'invalid_yaml', message: 'invalid YAML: did not find expected node at line 3' });

    expect(view.message).toBe("Le fichier n'est pas un YAML valide : did not find expected node at line 3");
    expect(view.job).toBeNull();
  });

  it('falls back to the server message for a code it does not know', () => {
    expect(describeProblem({ code: 'something_new', message: 'a new rule' }).message).toBe('a new rule');
  });

  it('words the warnings', () => {
    expect(describeWarning({ code: 'empty_image', job: 'a' })).toEqual({ message: "Le job «\u00a0a\u00a0» n'a pas d'image.", job: 'a' });
    expect(describeWarning({ code: 'empty_script', job: 'a' }).message).toBe("Le job «\u00a0a\u00a0» n'a aucune commande.");
    expect(describeWarning({ code: 'duplicate_stage', stage: 'build' })).toEqual({ message: "L'étape «\u00a0build\u00a0» est déclarée deux fois.", job: null });
  });
});
