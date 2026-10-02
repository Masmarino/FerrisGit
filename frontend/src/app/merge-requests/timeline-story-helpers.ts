// Storybook-only helpers shared by the merge request timeline stories.
import { componentWrapperDecorator } from '@storybook/angular-vite';

// A leaf item (comment card, thread, system note) hangs its avatar/marker in the timeline's left
// gutter: shown standalone it needs the same 44px gutter and 2px rail as `.mr-timeline__body`
// (see `_timeline-rail.scss`), or the marker would be clipped and float on nothing.
const hairline = 'color-mix(in srgb, var(--border-color) 45%, transparent)';
export const onTimelineRail = componentWrapperDecorator(
  (story) =>
    `<div class="mr-story-rail" style="padding-left: 44px; background-image: linear-gradient(${hairline}, ${hairline}); background-position: 15px 16px; background-size: 2px calc(100% - 16px); background-repeat: no-repeat; max-width: 860px;">${story}</div>`,
);

export const RAIL_AXIS = 16;

/** Play-function guard (jsdom cannot measure layout): every element matching `markerSelector` is centred on the rail within 0.5px. */
export function expectOnRail(canvasElement: HTMLElement, markerSelector: string, railSelector = '.mr-story-rail'): void {
  const rail = canvasElement.querySelector(railSelector);
  if (!rail) throw new Error(`no rail (${railSelector})`);
  const markers = Array.from(canvasElement.querySelectorAll(markerSelector));
  if (markers.length === 0) throw new Error(`nothing on the rail (${markerSelector})`);
  const axis = rail.getBoundingClientRect().left + RAIL_AXIS;
  for (const marker of markers) {
    const box = marker.getBoundingClientRect();
    const offset = box.left + box.width / 2 - axis;
    if (Math.abs(offset) > 0.5) throw new Error(`${markerSelector} off the rail by ${offset.toFixed(2)}px`);
  }
}
