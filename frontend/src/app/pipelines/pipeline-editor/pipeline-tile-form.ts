import { Component, computed, input, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { Checkbox } from '@masmarino/gabarit/checkbox';
import { GbtInput } from '@masmarino/gabarit/input';
import { SegmentedControl } from '@masmarino/gabarit/segmented-control';
import { Textarea } from '@masmarino/gabarit/textarea';
import { HelpTip } from './help-tip';
import { JobTile, ParamValue, ParamValues, TileParam, buildTile, defaultValues, tileProblems } from './pipeline-tile-types';
import { TranslocoPipe } from '@jsverse/transloco';

/**
 * The questions a tile asks before it makes a job, with a preview of what the job will run as they are answered.
 * Nothing is created until the last button, and going back drops the answers.
 */
@Component({
  selector: 'fg-pipeline-tile-form',
  standalone: true,
  imports: [TranslocoPipe, FormsModule, Badge, Button, Checkbox, GbtInput, SegmentedControl, Textarea, HelpTip],
  templateUrl: './pipeline-tile-form.html',
  styleUrl: './pipeline-tile-form.scss',
})
export class PipelineTileForm {
  tile = input.required<JobTile>();
  stage = input.required<string>();
  /** The repository's secret names, or `null` when this person cannot see them. */
  secrets = input<string[] | null>(null);

  confirmed = output<ParamValues>();
  back = output<void>();

  protected readonly values = signal<ParamValues | null>(null);
  protected readonly answers = computed<ParamValues>(() => ({ ...defaultValues(this.tile().params), ...(this.values() ?? {}) }));
  protected readonly problems = computed(() => tileProblems(this.tile(), this.answers()));
  protected readonly valid = computed(() => Object.keys(this.problems()).length === 0);
  protected readonly built = computed(() => buildTile(this.tile(), this.answers()));
  /** The questions that apply, given the answers so far. */
  protected readonly shown = computed(() => (this.tile().params ?? []).filter((param) => !param.showIf || param.showIf(this.answers())));
  /** The secrets the job will read, and whether the repository has them (when that can be known). */
  protected readonly needed = computed(() => {
    const known = this.secrets();
    return (this.built().secrets ?? []).map((name) => ({ name, present: known === null ? null : known.includes(name) }));
  });
  protected readonly command = computed(() => this.built().script.join('\n'));
  /** A problem shows once the person has touched the field, not on a freshly opened form. */
  protected readonly touched = signal<Set<string>>(new Set());

  protected text(param: TileParam): string {
    return String(this.answers()[param.id] ?? '');
  }

  protected checked(param: TileParam): boolean {
    return this.answers()[param.id] === true;
  }

  protected set(param: TileParam, value: ParamValue): void {
    this.values.set({ ...(this.values() ?? {}), [param.id]: value });
    this.touched.update((ids) => new Set(ids).add(param.id));
  }

  protected problem(param: TileParam): string | null {
    return this.touched().has(param.id) ? (this.problems()[param.id] ?? null) : null;
  }

  protected submit(): void {
    if (!this.valid()) {
      // Show every problem at once, so that nobody has to find the faulty field by trial and error.
      this.touched.set(new Set(Object.keys(this.problems())));
      return;
    }
    this.confirmed.emit(this.answers());
  }
}
