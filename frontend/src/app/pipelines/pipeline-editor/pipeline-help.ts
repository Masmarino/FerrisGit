import { perLanguage, t } from '../../shared/i18n/translator';

/**
 * What each part of the editor is for, written for someone who has never written a pipeline. Shown in the help bubbles.
 */
export interface HelpText {
  title: string;
  body: string;
}

export const help = perLanguage(() => ({
  pipeline: {
    title: t('pipelines.help.pipeline.title'),
    body: t('pipelines.help.pipeline.body'),
  },
  stage: {
    title: t('pipelines.help.stage.title'),
    body: t('pipelines.help.stage.body'),
  },
  newStage: {
    title: t('pipelines.help.newStage.title'),
    body: t('pipelines.help.newStage.body'),
  },
  job: {
    title: t('pipelines.help.job.title'),
    body: t('pipelines.help.job.body'),
  },
  tiles: {
    title: t('pipelines.help.tiles.title'),
    body: t('pipelines.help.tiles.body'),
  },
  name: {
    title: t('pipelines.help.name.title'),
    body: t('pipelines.help.name.body'),
  },
  image: {
    title: t('pipelines.help.image.title'),
    body: t('pipelines.help.image.body'),
  },
  script: {
    title: t('pipelines.help.script.title'),
    body: t('pipelines.help.script.body'),
  },
  insert: {
    title: t('pipelines.help.insert.title'),
    body: t('pipelines.help.insert.body'),
  },
  variables: {
    title: t('pipelines.help.variables.title'),
    body: t('pipelines.help.variables.body'),
  },
  secrets: {
    title: t('pipelines.help.secrets.title'),
    body: t('pipelines.help.secrets.body'),
  },
  needs: {
    title: t('pipelines.help.needs.title'),
    body: t('pipelines.help.needs.body'),
  },
  tags: {
    title: t('pipelines.help.tags.title'),
    body: t('pipelines.help.tags.body'),
  },
  cache: {
    title: t('pipelines.help.cache.title'),
    body: t('pipelines.help.cache.body'),
  },
  yaml: {
    title: t('pipelines.help.yaml.title'),
    body: t('pipelines.help.yaml.body'),
  },
  propose: {
    title: t('pipelines.help.propose.title'),
    body: t('pipelines.help.propose.body'),
  },
}) satisfies Record<string, HelpText>);
