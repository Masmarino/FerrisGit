import { Component, input } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { GbtInput } from '@masmarino/gabarit/input';
import { Panel } from '@masmarino/gabarit/panel';
import { SegmentedControl, SegmentedControlOption } from '@masmarino/gabarit/segmented-control';
import { Select, SelectOption } from '@masmarino/gabarit/select';
import { WorkspaceGrid, WorkspaceSortKey } from './workspace-grid';

const DIRECTION_OPTIONS: SegmentedControlOption<'asc' | 'desc'>[] = [
  { value: 'asc', label: 'Croissant' },
  { value: 'desc', label: 'Décroissant' },
];

/**
 * Search and sort panels for a workspace grid, meant for the page aside:
 *   <fg-workspace-grid #grid … /> <div page-aside><fg-workspace-grid-filters [grid]="grid" /></div>
 */
@Component({
  selector: 'fg-workspace-grid-filters',
  standalone: true,
  imports: [FormsModule, GbtInput, Select, SegmentedControl, Panel],
  templateUrl: './workspace-grid-filters.html',
  styleUrl: './workspace-grid-filters.scss',
})
export class WorkspaceGridFilters {
  grid = input.required<WorkspaceGrid>();

  protected readonly directionOptions = DIRECTION_OPTIONS;

  protected sortOptions(): SelectOption<WorkspaceSortKey>[] {
    return this.grid().sortOptions as SelectOption<WorkspaceSortKey>[];
  }
}
