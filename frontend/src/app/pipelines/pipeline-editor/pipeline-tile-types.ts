export type TileCategory = 'compile' | 'test' | 'quality' | 'package' | 'deploy' | 'custom';

/** What a person fills in to adapt a tile: a few words, a choice, a switch, or a few lines. */
export type ParamValue = string | boolean;
export type ParamValues = Record<string, ParamValue>;

export interface TileParam {
  id: string;
  label: string;
  kind: 'text' | 'choice' | 'toggle' | 'lines';
  /** What it is for and what to put in it. */
  hint?: string;
  placeholder?: string;
  default: ParamValue;
  options?: { value: string; label: string }[];
  /** An empty value is refused (a toggle is never empty). */
  required?: boolean;
  /** What a text may look like. Anything that ends up in a command has one, so that nothing needs escaping. */
  pattern?: RegExp;
  patternMessage?: string;
  /** Shown only for some answers to the others. */
  showIf?: (values: ParamValues) => boolean;
}

/** The parts of a job a tile decides, from the answers when it has questions. */
export interface TileBuild {
  image: string;
  script: string[];
  variables?: Record<string, string>;
  cache?: string[];
  tags?: string[];
  /** Secrets of the repository its commands read: they have to exist for the job to work. */
  secrets?: string[];
}

/** A job ready to drop in a stage: what it does in a sentence, and the image and commands that do it. */
export interface JobTile extends TileBuild {
  id: string;
  category: TileCategory;
  title: string;
  /** What it does, in a sentence anyone can read. */
  summary: string;
  /** What to know before using it: what it needs, what it does not do. */
  help: string;
  icon: string;
  /** The name a new job gets (made unique if taken). */
  jobName: string;
  /** The stage it usually sits in. */
  stage: string;
  /** Questions to answer first. The fields above are what the answers' defaults give. */
  params?: TileParam[];
  /** The job for some answers. */
  build?: (values: ParamValues) => TileBuild;
  /**
   * Whether it reads secrets of the repository: those only reach jobs run by Docker runners, so with Kubernetes it
   * cannot work. Set for every tile that names secrets.
   */
  needsSecrets?: boolean;
}

export const defaultValues = (params: readonly TileParam[] = []): ParamValues => Object.fromEntries(params.map((param) => [param.id, param.default]));

/** What a tile builds for some answers, the others being their defaults. */
export function buildTile(tile: JobTile, values: ParamValues = {}): TileBuild {
  return tile.build ? tile.build({ ...defaultValues(tile.params), ...values }) : tile;
}

/** The question's problem, or `null`: a shown question that is required and empty, or whose text does not fit. */
export function paramProblem(param: TileParam, values: ParamValues): string | null {
  if (param.showIf && !param.showIf(values)) {
    return null;
  }
  const value = values[param.id] ?? param.default;
  if (typeof value !== 'string') {
    return null;
  }
  const text = value.trim();
  if (text === '') {
    return param.required ? 'À remplir.' : null;
  }
  if (param.pattern && !param.pattern.test(text)) {
    return param.patternMessage ?? 'Valeur non valide.';
  }
  return null;
}

export const tileProblems = (tile: JobTile, values: ParamValues): Record<string, string> =>
  Object.fromEntries((tile.params ?? []).flatMap((param) => {
    const problem = paramProblem(param, { ...defaultValues(tile.params), ...values });
    return problem ? [[param.id, problem]] : [];
  }));
