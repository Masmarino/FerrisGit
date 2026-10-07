import { BuilderJob } from './pipeline-builder-model';
import { KNOWN_TILE_SECRETS } from './pipeline-catalog';

/** A name a shell or a job's variable can have. */
const ENV_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;

export const isEnvName = (name: string) => ENV_NAME.test(name);

/** Names that look like a credential, which do not belong in a file everyone reads. */
const SECRET_LOOKING = /(TOKEN|SECRET|PASSWORD|PASSWD|PASSPHRASE|CREDENTIAL|PRIVATE|API_?KEY|ACCESS_?KEY|_KEY$)/i;

export const looksLikeSecret = (name: string) => SECRET_LOOKING.test(name);

/** Set by the shell or the image itself, so never missing. */
const ALWAYS_THERE = new Set(['HOME', 'PATH', 'PWD', 'OLDPWD', 'USER', 'HOSTNAME', 'SHELL', 'TERM', 'LANG', 'TMPDIR', 'IFS', 'UID', 'PS1', 'RANDOM', 'LINENO']);

const REFERENCE = /\$(?:\{([A-Za-z_][A-Za-z0-9_]*)[^}]*\}|([A-Za-z_][A-Za-z0-9_]*))/g;
const ASSIGNMENT = /(?:^|[\s;&|])(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)=/g;

/** The variables the job's commands read: `$NOM` and `${NOM}`, in order of first use. */
export function referencedNames(job: Pick<BuilderJob, 'script'>): string[] {
  const names: string[] = [];
  for (const line of job.script) {
    for (const match of line.matchAll(REFERENCE)) {
      const name = match[1] ?? match[2];
      if (!names.includes(name)) {
        names.push(name);
      }
    }
  }
  return names;
}

/** Variables the commands define themselves (`NOM=valeur`, `export NOM=…`), which are therefore not missing. */
function assignedNames(job: Pick<BuilderJob, 'script'>): Set<string> {
  const names = new Set<string>();
  for (const line of job.script) {
    for (const match of line.matchAll(ASSIGNMENT)) {
      names.add(match[1]);
    }
  }
  return names;
}

/**
 * The names a job's commands read that nothing here provides: neither one of its variables, nor a secret of the
 * repository, nor something the shell or the commands define. The image may still provide them (CARGO_HOME…), so this is a
 * thing to check, not an error.
 */
export function unknownReferences(job: Pick<BuilderJob, 'script' | 'variables'>, secretNames: readonly string[]): string[] {
  const known = new Set<string>([...job.variables.map((row) => row.key.trim()), ...secretNames, ...assignedNames(job), ...ALWAYS_THERE]);
  return referencedNames(job).filter((name) => !known.has(name));
}

/** Which jobs read each secret, by name. A secret nothing reads has an empty list. */
export function secretUsage(jobs: readonly BuilderJob[], secretNames: readonly string[]): Map<string, string[]> {
  const usage = new Map<string, string[]>(secretNames.map((name) => [name, []]));
  for (const job of jobs) {
    for (const name of referencedNames(job)) {
      usage.get(name)?.push(job.name);
    }
  }
  return usage;
}

/** The secrets a job reads that the repository does not have yet. */
export function missingSecrets(job: Pick<BuilderJob, 'script' | 'variables'>, wanted: readonly string[], secretNames: readonly string[]): string[] {
  const own = new Set(job.variables.map((row) => row.key.trim()));
  const reads = new Set(referencedNames(job));
  return wanted.filter((name) => reads.has(name) && !own.has(name) && !secretNames.includes(name));
}

/** What a job reads that is surely a secret: a name the catalog's tiles expect, or one that looks like a credential. */
export const wantedSecretNames = (job: Pick<BuilderJob, 'script'>): string[] => referencedNames(job).filter((name) => KNOWN_TILE_SECRETS.includes(name) || looksLikeSecret(name));
