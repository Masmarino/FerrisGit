import { t } from '../../shared/i18n/translator';

export type TileCategory = 'compile' | 'test' | 'quality' | 'package' | 'deploy' | 'custom';

/** What people fill in to adapt a tile: a few words, a choice, a switch, or a few lines. */
export type ParamValue = string | boolean;
export type ParamValues = Record<string, ParamValue>;

export interface TileParam {
  id: string;
  label: string;
  kind: 'text' | 'choice' | 'toggle' | 'lines';
  /** What it is for, and what to put in it. */
  hint?: string;
  placeholder?: string;
  default: ParamValue;
  options?: { value: string; label: string }[];
  /** An empty value is refused (a toggle is never empty). */
  required?: boolean;
  /** What a text may look like. Every value that ends up in a command has one, so that nothing ever needs escaping. */
  pattern?: RegExp;
  patternMessage?: string;
  /** Shown only for some answers to the other questions. */
  showIf?: (values: ParamValues) => boolean;
}

/** The parts of a job that a tile decides, from the answers when it asks questions. */
export interface TileBuild {
  image: string;
  script: string[];
  variables?: Record<string, string>;
  cache?: string[];
  tags?: string[];
  /** The repository secrets its commands read. They have to exist for the job to work. */
  secrets?: string[];
}

/** A job ready to drop into a stage: what it does in one sentence, and the image and commands that do it. */
export interface JobTile extends TileBuild {
  id: string;
  category: TileCategory;
  title: string;
  /** What it does, in a sentence anyone can understand. */
  summary: string;
  /** What to know before using it: what it needs, and what it does not do. */
  help: string;
  icon: string;
  /** The name a new job gets (made unique if it is taken). */
  jobName: string;
  /** The stage it usually goes in. */
  stage: string;
  /** Questions to answer first. The fields above are what the default answers give. */
  params?: TileParam[];
  /** The job for a given set of answers. */
  build?: (values: ParamValues) => TileBuild;
  /**
   * Whether it reads repository secrets. Those only reach jobs run by Docker runners, so it cannot work with
   * Kubernetes. Set on every tile that names secrets.
   */
  needsSecrets?: boolean;
  /**
   * Whether its commands work on the repository's files. A Kubernetes Pod gets no copy of the repository, so it cannot
   * work there as it is.
   */
  needsSource?: boolean;
}

export const defaultValues = (params: readonly TileParam[] = []): ParamValues => Object.fromEntries(params.map((param) => [param.id, param.default]));

/** What a tile builds for the given answers, the others taking their defaults. */
export function buildTile(tile: JobTile, values: ParamValues = {}): TileBuild {
  return tile.build ? tile.build({ ...defaultValues(tile.params), ...values }) : tile;
}

/**
 * What is wrong with an answer, or `null`: a visible question that is required and empty, or a text that does not match
 * its pattern.
 */
function paramProblem(param: TileParam, values: ParamValues): string | null {
  if (param.showIf && !param.showIf(values)) {
    return null;
  }
  const value = values[param.id] ?? param.default;
  if (typeof value !== 'string') {
    return null;
  }
  const text = value.trim();
  if (text === '') {
    return param.required ? t('pipelines.editor.required') : null;
  }
  if (param.pattern && !param.pattern.test(text)) {
    return param.patternMessage ?? t('pipelines.editor.invalid');
  }
  return null;
}

export const tileProblems = (tile: JobTile, values: ParamValues): Record<string, string> =>
  Object.fromEntries((tile.params ?? []).flatMap((param) => {
    const problem = paramProblem(param, { ...defaultValues(tile.params), ...values });
    return problem ? [[param.id, problem]] : [];
  }));
