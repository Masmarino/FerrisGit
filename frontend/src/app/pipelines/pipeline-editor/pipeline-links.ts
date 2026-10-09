import { Component, DestroyRef, ElementRef, afterNextRender, afterRenderEffect, inject, input, signal } from '@angular/core';

/** A `needs`: the job `to` waits for the job `from`. */
export interface PipelineLink {
  from: string;
  to: string;
  /** A link of the job under the pointer or with the focus. */
  highlighted: boolean;
  /** `from` is in a later stage than `to`, which the server refuses. */
  invalid: boolean;
}

interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

interface DrawnLink {
  key: string;
  d: string;
  highlighted: boolean;
  invalid: boolean;
}

const round = (value: number) => Math.round(value * 10) / 10;

/**
 * The same curve as on a pipeline's page (gbt-job-graph): from the right edge of the job waited for to the left edge of
 * the job that waits, so that both pages draw links alike. Two jobs of the same stage (the server allows it) sit one
 * above the other, so their link is a bracket on the right of the column, from one right edge to the other.
 */
export function linkPath(from: Box, to: Box): string {
  const x1 = from.left + from.width;
  const y1 = from.top + from.height / 2;
  const y2 = to.top + to.height / 2;
  if (to.left < x1) {
    const x2 = to.left + to.width;
    const out = Math.max(x1, x2) + 16;
    return `M ${round(x1)} ${round(y1)} C ${round(out)} ${round(y1)}, ${round(out)} ${round(y2)}, ${round(x2)} ${round(y2)}`;
  }
  const x2 = to.left;
  const dx = Math.max(24, (x2 - x1) / 2);
  return `M ${round(x1)} ${round(y1)} C ${round(x1 + dx)} ${round(y1)}, ${round(x2 - dx)} ${round(y2)}, ${round(x2)} ${round(y2)}`;
}

/**
 * The links between the jobs of the board, drawn behind the cards. It sits inside the board, measures the cards it
 * finds there by their `data-job`, and redraws whenever the links or the board's size change. It is decorative (the
 * cards say the same thing in words, "Après …"), so it is hidden from assistive technology.
 */
@Component({
  selector: 'fg-pipeline-links',
  standalone: true,
  template: `
    <svg class="pipeline-links__svg" aria-hidden="true" focusable="false">
      @for (link of drawn(); track link.key) {
        <path [attr.d]="link.d" [attr.data-link]="link.key" [attr.data-highlighted]="link.highlighted || null" [attr.data-invalid]="link.invalid || null" />
      }
    </svg>
  `,
  styles: `
    :host {
      position: absolute;
      inset: 0;
      z-index: 0;
      pointer-events: none;
    }
    .pipeline-links__svg {
      width: 100%;
      height: 100%;
      overflow: visible;
    }
    path {
      fill: none;
      stroke: var(--border-color);
      stroke-width: 2;
    }
    /* The links of the pointed job, in the indigo its cards take. */
    path[data-highlighted] {
      stroke: var(--color-info-vivid-base);
    }
    /* A link the server refuses, in the red of its problem. */
    path[data-invalid] {
      stroke: var(--color-error-base);
      stroke-dasharray: 4 4;
    }
  `,
})
export class PipelineLinks {
  links = input.required<PipelineLink[]>();

  protected readonly drawn = signal<DrawnLink[]>([]);
  private host = inject<ElementRef<HTMLElement>>(ElementRef);
  private destroyRef = inject(DestroyRef);

  constructor() {
    afterRenderEffect(() => {
      this.links();
      this.measure();
    });
    // Cards move whenever something above them in their stage changes size (a card that grows, a stage header once its
    // font has loaded), without the board itself changing. So every part of every stage is watched, and the board too
    // for window resizes.
    afterNextRender(() => {
      const board = this.host.nativeElement.parentElement;
      if (!board) return;
      document.fonts?.ready.then(() => this.measure());
      if (typeof ResizeObserver === 'undefined') return;
      const observer = new ResizeObserver(() => this.measure());
      const watch = () => {
        observer.disconnect();
        observer.observe(board);
        board.querySelectorAll('.pipeline-editor__lane, .pipeline-editor__lane > *, [data-job]').forEach((part) => observer.observe(part));
      };
      watch();
      // Cards and stages being added or removed. Not the links being drawn, or every drawing would trigger another.
      const stages = new MutationObserver((changes) => {
        if (changes.some((change) => !this.host.nativeElement.contains(change.target))) watch();
      });
      stages.observe(board, { childList: true, subtree: true });
      this.destroyRef.onDestroy(() => {
        observer.disconnect();
        stages.disconnect();
      });
    });
  }

  private measure(): void {
    const board = this.host.nativeElement.parentElement;
    if (!board) return;
    const origin = board.getBoundingClientRect();
    const boxes = new Map<string, Box>();
    board.querySelectorAll<HTMLElement>('[data-job]').forEach((card) => {
      const rect = card.getBoundingClientRect();
      boxes.set(card.dataset['job'] ?? '', { left: rect.left - origin.left + board.scrollLeft, top: rect.top - origin.top + board.scrollTop, width: rect.width, height: rect.height });
    });
    const drawn = this.links().flatMap((link) => {
      const from = boxes.get(link.from);
      const to = boxes.get(link.to);
      return from && to ? [{ key: `${link.from}->${link.to}`, d: linkPath(from, to), highlighted: link.highlighted, invalid: link.invalid }] : [];
    });
    // Highlighted links last, so that they are drawn over the others where they cross.
    this.drawn.set([...drawn.filter((link) => !link.highlighted), ...drawn.filter((link) => link.highlighted)]);
  }
}
