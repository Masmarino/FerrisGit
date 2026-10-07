import { Component, computed, input, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Alert } from '@masmarino/gabarit/alert';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { Checkbox } from '@masmarino/gabarit/checkbox';
import { GbtInput } from '@masmarino/gabarit/input';
import { Menu, MenuItem } from '@masmarino/gabarit/menu';
import { TagInput } from '@masmarino/gabarit/tag-input';
import { Textarea } from '@masmarino/gabarit/textarea';
import { HelpTip } from './help-tip';
import { BuilderJob, BuilderVariable } from './pipeline-builder-model';
import { HELP } from './pipeline-help';
import { ProblemView } from './pipeline-problems';
import { KNOWN_TILE_SECRETS } from './pipeline-catalog';
import { isEnvName, looksLikeSecret, referencedNames, unknownReferences } from './pipeline-references';

/** Images people start from. A click fills the field; anything else can still be typed. */
const IMAGE_SUGGESTIONS = ['alpine:3.20', 'rust:1', 'node:22', 'python:3.13', 'golang:1.23', 'docker:27-cli'];
const CACHE_SUGGESTIONS = ['cargo-home', 'cargo-target', 'npm', 'pip'];

/**
 * The fields of one job, in the drawer. It holds no state of its own: every change goes out as a patch and the editor
 * applies it, so what is shown is always the editor's job.
 */
@Component({
  selector: 'fg-pipeline-job-form',
  standalone: true,
  imports: [FormsModule, Alert, Badge, Button, Checkbox, GbtInput, Menu, MenuItem, TagInput, Textarea, HelpTip],
  templateUrl: './pipeline-job-form.html',
  styleUrl: './pipeline-job-form.scss',
})
export class PipelineJobForm {
  job = input.required<BuilderJob>();
  /** The jobs this one may wait for. */
  needOptions = input<string[]>([]);
  problems = input<ProblemView[]>([]);
  /** The repository's secrets by name; `null` when this person cannot see them (only maintainers can). */
  secrets = input<string[] | null>(null);
  /** Whether this person can create secrets, so that a variable which is really one can be turned into one. */
  canManageSecrets = input(false);
  engine = input<string | null>(null);

  patch = output<Partial<BuilderJob>>();
  remove = output<void>();
  /** A variable of the job is a secret in disguise: save it as one and take it out of the file. */
  makeSecret = output<number>();
  /** A secret the commands read does not exist yet. */
  createSecret = output<string>();

  protected readonly help = HELP;
  protected readonly imageSuggestions = IMAGE_SUGGESTIONS;
  protected readonly cacheSuggestions = computed(() => CACHE_SUGGESTIONS.filter((key) => !this.job().cache.includes(key)));

  /** What can be ticked: the possible jobs, plus any that is already waited for but no longer possible, so it can be unticked. */
  protected readonly needChoices = computed(() => {
    const options = this.needOptions();
    return [...options, ...this.job().needs.filter((need) => !options.includes(need))];
  });

  /** Something to insert in a command: the job's own variables, then the repository's secrets. */
  protected readonly insertable = computed(() => {
    const own = this.job().variables.map((row) => row.key.trim()).filter((key) => isEnvName(key));
    return [...new Set([...own, ...(this.secrets() ?? [])])];
  });

  /** The secrets of the repository the commands read, and those they read that do not exist. */
  protected readonly usedSecrets = computed(() => {
    const secrets = this.secrets();
    return secrets === null ? [] : referencedNames(this.job()).filter((name) => secrets.includes(name) && !this.hasVariable(name));
  });
  protected readonly unknown = computed(() => {
    const secrets = this.secrets();
    return secrets === null ? [] : unknownReferences(this.job(), secrets);
  });
  /** Names that are surely secrets nobody has created yet, which a maintainer can create from here. */
  protected readonly creatable = computed(() => (this.canManageSecrets() ? this.unknown().filter((name) => looksLikeSecret(name) || KNOWN_TILE_SECRETS.includes(name)) : []));
  protected readonly cachesIgnored = computed(() => this.engine() === 'docker-runners' && this.job().cache.length > 0);

  /** The command last typed in, so that an inserted variable lands there, at the cursor. */
  private focused: { index: number; field: HTMLTextAreaElement | null } = { index: -1, field: null };
  protected readonly insertedAt = signal<number | null>(null);

  private hasVariable(name: string): boolean {
    return this.job().variables.some((row) => row.key.trim() === name);
  }

  protected variableProblem(row: BuilderVariable): string | null {
    const key = row.key.trim();
    if (key !== '' && !isEnvName(key)) {
      return 'Lettres, chiffres et _, sans commencer par un chiffre.';
    }
    return null;
  }

  protected looksSecret(row: BuilderVariable): boolean {
    return row.value !== '' && looksLikeSecret(row.key.trim());
  }

  protected setCommand(index: number, text: string): void {
    this.patch.emit({ script: this.job().script.map((line, i) => (i === index ? text : line)) });
  }

  protected addCommand(): void {
    this.patch.emit({ script: [...this.job().script, ''] });
  }

  protected removeCommand(index: number): void {
    this.patch.emit({ script: this.job().script.filter((_, i) => i !== index) });
  }

  protected moveCommand(index: number, by: -1 | 1): void {
    const script = [...this.job().script];
    const target = index + by;
    if (target < 0 || target >= script.length) {
      return;
    }
    [script[index], script[target]] = [script[target], script[index]];
    this.patch.emit({ script });
  }

  protected noteFocus(index: number, event: Event): void {
    const field = event.target instanceof HTMLTextAreaElement ? event.target : null;
    this.focused = { index, field };
  }

  /** Writes `$NAME` where the cursor was in the last command touched, or in a new command when there is none. */
  protected insert(name: string): void {
    const script = [...this.job().script];
    const token = `$${name}`;
    const { index } = this.focused;
    if (index < 0 || index >= script.length) {
      script.push(token);
      this.patch.emit({ script });
      this.insertedAt.set(script.length - 1);
      return;
    }
    const field = this.focused.field;
    const line = script[index];
    const start = field?.selectionStart ?? line.length;
    const end = field?.selectionEnd ?? start;
    script[index] = `${line.slice(0, start)}${token}${line.slice(end)}`;
    this.patch.emit({ script });
    this.insertedAt.set(index);
  }

  protected setVariable(index: number, change: Partial<BuilderVariable>): void {
    this.patch.emit({ variables: this.job().variables.map((row, i) => (i === index ? { ...row, ...change } : row)) });
  }

  protected addVariable(): void {
    this.patch.emit({ variables: [...this.job().variables, { key: '', value: '' }] });
  }

  protected removeVariable(index: number): void {
    this.patch.emit({ variables: this.job().variables.filter((_, i) => i !== index) });
  }

  protected toggleNeed(name: string, checked: boolean): void {
    const needs = this.job().needs;
    this.patch.emit({ needs: checked ? [...needs.filter((need) => need !== name), name] : needs.filter((need) => need !== name) });
  }

  protected addCache(key: string): void {
    this.patch.emit({ cache: [...this.job().cache, key] });
  }
}
