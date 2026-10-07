/**
 * A name a shell can export, so one a CI variable or a job's variable can have: letters, digits and _, no digit first. The
 * server refuses any other for a CI variable (settings.rs, `is_env_name`); this says so while the name is being typed.
 */
export const isEnvName = (name: string): boolean => /^[A-Za-z_][A-Za-z0-9_]*$/.test(name);

/** What is wrong with a name being typed, in the words of the forms that ask for one; `null` for a fine or empty one. */
export const envNameProblem = (name: string): string | null => (name !== '' && !isEnvName(name) ? 'Lettres, chiffres et _, sans commencer par un chiffre.' : null);
