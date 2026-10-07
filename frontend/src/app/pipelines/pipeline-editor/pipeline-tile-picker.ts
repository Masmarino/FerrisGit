import { Component, computed, input, output, signal } from '@angular/core';
import { Badge } from '@masmarino/gabarit/badge';
import { Icon } from '@masmarino/gabarit/icon';
import { HelpTip } from './help-tip';
import { HELP } from './pipeline-help';
import { JOB_TILES, JobTile, ParamValues, TILE_CATEGORIES } from './pipeline-catalog';
import { PipelineTileForm } from './pipeline-tile-form';

/** The jobs ready to use, by what they are for. Picking one is the whole gesture: it comes with an image and commands. */
@Component({
  selector: 'fg-pipeline-tile-picker',
  standalone: true,
  imports: [Badge, Icon, HelpTip, PipelineTileForm],
  templateUrl: './pipeline-tile-picker.html',
  styleUrl: './pipeline-tile-picker.scss',
})
export class PipelineTilePicker {
  stage = input.required<string>();
  /** The repository's secrets by name, or `null` when this person cannot see them. */
  secrets = input<string[] | null>(null);

  engine = input<string | null>(null);

  /** A tile and the answers to its questions (none for a tile without questions). */
  chosen = output<{ tile: JobTile; values: ParamValues }>();

  /** The tile whose questions are being answered. */
  protected readonly asking = signal<JobTile | null>(null);

  protected readonly help = HELP;
  protected readonly sections = computed(() => TILE_CATEGORIES.map((category) => ({ ...category, tiles: JOB_TILES.filter((tile) => tile.category === category.id) })).filter((section) => section.tiles.length > 0));

  /** The secrets a tile reads that the repository does not have yet, when that can be known. */
  protected missing(tile: JobTile): string[] {
    const secrets = this.secrets();
    return secrets === null ? [] : (tile.secrets ?? []).filter((name) => !secrets.includes(name));
  }

  /** Secrets only reach jobs run by Docker runners, so a tile that reads some cannot work with Kubernetes. */
  protected unavailable(tile: JobTile): boolean {
    return this.engine() === 'kubernetes' && tile.needsSecrets === true;
  }

  protected choose(tile: JobTile): void {
    if (this.unavailable(tile)) {
      return;
    }
    if (tile.params && tile.params.length > 0) {
      this.asking.set(tile);
    } else {
      this.chosen.emit({ tile, values: {} });
    }
  }
}
