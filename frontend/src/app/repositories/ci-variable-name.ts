/**
 * Whether a shell can export `name`, which is what a CI variable or a job variable name has to be: letters, digits and
 * _, not starting with a digit. The server refuses any other name for a CI variable (settings.rs, `is_env_name`); this
 * lets the form say so while the name is being typed.
 */
export const isEnvName = (name: string): boolean => /^[A-Za-z_][A-Za-z0-9_]*$/.test(name);

/**
 * What is wrong with a name being typed, in the words of the forms that ask for one, or `null` for a valid or empty
 * name.
 */
export const envNameProblem = (name: string): string | null => (name !== '' && !isEnvName(name) ? 'Lettres, chiffres et _, sans commencer par un chiffre.' : null);
