import { computed, inject, signal } from '@angular/core';
import type { SelectOption } from '@masmarino/gabarit';
import { Label, LabelsService } from '../labels/labels.service';
import { Milestone, MilestonesService } from '../milestones/milestones.service';

export const labelSelectOptions = (labels: Label[]): SelectOption<string>[] => labels.map((label) => ({ value: label.id, label: label.name, color: label.color }));

export const milestoneSelectOptions = (milestones: Milestone[], noneLabel: string): SelectOption<string | null>[] => [
  { value: null, label: noneLabel },
  ...milestones.map((milestone) => ({ value: milestone.id, label: milestone.title })),
];

/**
 * Label and milestone filters shared by the issue list and the board: selection, options and the data behind them.
 * The server applies both, so any change calls `refetch`. Call it from an injection context.
 */
export function createIssueFilters(repositoryId: () => string, refetch: () => void) {
  const labelsService = inject(LabelsService);
  const milestonesService = inject(MilestonesService);

  const labels = signal<Label[]>([]);
  const milestones = signal<Milestone[]>([]);
  const selectedLabelIds = signal<string[]>([]);
  const selectedMilestoneId = signal<string | null>(null);
  const hasSelection = computed(() => selectedLabelIds().length > 0 || selectedMilestoneId() !== null);

  return {
    selectedLabelIds,
    selectedMilestoneId,
    hasSelection,
    labelOptions: computed(() => labelSelectOptions(labels())),
    milestoneOptions: computed(() => milestoneSelectOptions(milestones(), 'Tous les milestones')),
    milestoneTitleById: computed(() => new Map(milestones().map((milestone) => [milestone.id, milestone.title]))),

    /** The server filters to send with the issues request. */
    params: () => ({ labelIds: selectedLabelIds(), milestoneId: selectedMilestoneId() ?? undefined }),

    loadOptions(): void {
      labelsService.listForRepository(repositoryId()).subscribe({ next: (list) => labels.set(list) });
      milestonesService.listForRepository(repositoryId()).subscribe({ next: (list) => milestones.set(list) });
    },

    selectLabels(labelIds: string[]): void {
      selectedLabelIds.set(labelIds);
      refetch();
    },

    selectMilestone(milestoneId: string | null): void {
      selectedMilestoneId.set(milestoneId);
      refetch();
    },

    /** Clears the selection and refetches only if there was one. */
    clear(): void {
      const hadSelection = hasSelection();
      selectedLabelIds.set([]);
      selectedMilestoneId.set(null);
      if (hadSelection) {
        refetch();
      }
    },
  };
}
