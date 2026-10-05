import { SegmentedControlOption } from '@masmarino/gabarit/segmented-control';

export type RepositoryVisibility = 'private' | 'public';

export const VISIBILITY_OPTIONS: SegmentedControlOption<RepositoryVisibility>[] = [
  { value: 'private', label: 'Privé' },
  { value: 'public', label: 'Public' },
];
