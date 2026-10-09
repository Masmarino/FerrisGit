import { envNameProblem, isEnvName } from './ci-variable-name';

describe('isEnvName', () => {
  it('knows which names an environment variable can have', () => {
    expect(['A', '_a', 'npm_config_cache', 'A1'].every(isEnvName)).toBe(true);
    expect(['1A', 'A-B', 'A B', ''].some(isEnvName)).toBe(false);
  });

  it('says what is wrong with a name being typed, and nothing about an empty one', () => {
    expect(envNameProblem('1A')).toBe('Lettres, chiffres et _, sans commencer par un chiffre.');
    expect(envNameProblem('API_KEY')).toBeNull();
    expect(envNameProblem('')).toBeNull();
  });
});
