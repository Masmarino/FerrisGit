import { SegmentedControlOption } from '@masmarino/gabarit/segmented-control';
import { t } from '../shared/i18n/translator';

export type RepositoryVisibility = 'private' | 'public';

export const visibilityOptions = (): SegmentedControlOption<RepositoryVisibility>[] => [
  { value: 'private', label: t('common.private') },
  { value: 'public', label: t('common.public') },
];
