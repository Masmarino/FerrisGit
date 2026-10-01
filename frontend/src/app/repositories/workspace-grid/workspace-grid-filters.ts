import { Component, input } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { GbtInput, Panel, SegmentedControl, SegmentedControlOption, Select, SelectOption } from '@masmarino/gabarit';
import { WorkspaceGrid, WorkspaceSortKey } from './workspace-grid';

const DIRECTION_OPTIONS: SegmentedControlOption<'asc' | 'desc'>[] = [
  { value: 'asc', label: 'Croissant' },
  { value: 'desc', label: 'Décroissant' },
];

/**
 * The aside panels ("Recherche", "Trier") bound to a `fg-workspace-grid`'s own search and sort:
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
