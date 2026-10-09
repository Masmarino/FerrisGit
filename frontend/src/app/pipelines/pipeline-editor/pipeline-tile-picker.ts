import { Component, computed, input, output, signal } from '@angular/core';
import { Badge } from '@masmarino/gabarit/badge';
import { Icon } from '@masmarino/gabarit/icon';
import { HelpTip } from './help-tip';
import { help } from './pipeline-help';
import { jobTiles, JobTile, ParamValues, tileCategories } from './pipeline-catalog';
import { PipelineTileForm } from './pipeline-tile-form';
import { BuilderJob, mainCommand } from './pipeline-builder-model';
import { PredictedJob } from './pipeline-prediction';

/** The ready-made jobs, grouped by purpose. Picking one is all it takes: it comes with an image and commands. */
@Component({
  selector: 'fg-pipeline-tile-picker',
  standalone: true,
  imports: [Badge, Icon, HelpTip, PipelineTileForm],
  templateUrl: './pipeline-tile-picker.html',
  styleUrl: './pipeline-tile-picker.scss',
})
export class PipelineTilePicker {
  stage = input.required<string>();
  /** The repository's secret names, or `null` when this person cannot see them. */
  secrets = input<string[] | null>(null);

  engine = input<string | null>(null);

  /** The jobs proposed for this repository that the pipeline does not have yet. They are offered first. */
  suggestions = input<PredictedJob[]>([]);

  /** A tile and the answers to its questions (none for a tile without questions). */
  chosen = output<{ tile: JobTile; values: ParamValues }>();
  /** A job proposed for this repository, as it was predicted. */
  chosenJob = output<BuilderJob>();

  /** What a suggested job runs, setup aside: that is what tells two of them apart. */
  protected readonly mainCommand = mainCommand;

  /** The tile whose questions are being answered. */
  protected readonly asking = signal<JobTile | null>(null);

  protected readonly help = help();
  protected readonly sections = computed(() => tileCategories().map((category) => ({ ...category, tiles: jobTiles().filter((tile) => tile.category === category.id) })).filter((section) => section.tiles.length > 0));

  /** The secrets a tile reads that the repository does not have yet, when that can be known. */
  protected missing(tile: JobTile): string[] {
    const secrets = this.secrets();
    return secrets === null ? [] : (tile.secrets ?? []).filter((name) => !secrets.includes(name));
  }

  /**
   * Why a tile cannot work on this instance, or `null` when it can. A Kubernetes Pod gets neither the repository's
   * secrets nor a copy of the repository, so a tile that needs either cannot work there.
   */
  protected unavailable(tile: JobTile): 'secrets' | 'source' | null {
    if (this.engine() !== 'kubernetes') {
      return null;
    }
    return tile.needsSecrets ? 'secrets' : tile.needsSource ? 'source' : null;
  }

  protected choose(tile: JobTile): void {
    if (this.unavailable(tile) !== null) {
      return;
    }
    if (tile.params && tile.params.length > 0) {
      this.asking.set(tile);
    } else {
      this.chosen.emit({ tile, values: {} });
    }
  }
}
