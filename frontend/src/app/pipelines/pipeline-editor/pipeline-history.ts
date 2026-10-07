/** A change that can be undone: the state before it, and what it did in words ("suppression du job lint"). */
export interface HistoryEntry<T> {
  state: T;
  label: string;
}

export interface EditHistory<T> {
  past: HistoryEntry<T>[];
  future: HistoryEntry<T>[];
  /** The change being typed: further changes with the same key soon after are the same step. */
  typing: { key: string; at: number } | null;
}

/** Enough to walk back through an afternoon's work without holding every keystroke. */
export const HISTORY_LIMIT = 100;
/** Keystrokes closer than this in one field are one step to undo, not one per letter. */
export const TYPING_PAUSE_MS = 1500;

export function emptyHistory<T>(): EditHistory<T> {
  return { past: [], future: [], typing: null };
}

/**
 * Records a change made over `before`. A change with the same `typingKey` as the previous one, within the pause, joins
 * it: undoing goes back to before the first of them. Any new change drops what could be redone.
 */
export function record<T>(history: EditHistory<T>, before: T, label: string, typingKey: string | null, now: number): EditHistory<T> {
  const typing = typingKey === null ? null : { key: typingKey, at: now };
  const joins = typingKey !== null && history.typing?.key === typingKey && now - history.typing.at < TYPING_PAUSE_MS && history.past.length > 0;
  if (joins) {
    return { ...history, future: [], typing };
  }
  return { past: [...history.past, { state: before, label }].slice(-HISTORY_LIMIT), future: [], typing };
}

/** Steps back from `current`: what to show, and what was undone. `null` when there is nothing to undo. */
export function undo<T>(history: EditHistory<T>, current: T): { history: EditHistory<T>; state: T; label: string } | null {
  const entry = history.past.at(-1);
  if (!entry) {
    return null;
  }
  return { history: { past: history.past.slice(0, -1), future: [...history.future, { state: current, label: entry.label }], typing: null }, state: entry.state, label: entry.label };
}

/** Steps forward again after an undo. `null` when there is nothing to redo. */
export function redo<T>(history: EditHistory<T>, current: T): { history: EditHistory<T>; state: T; label: string } | null {
  const entry = history.future.at(-1);
  if (!entry) {
    return null;
  }
  return { history: { past: [...history.past, { state: current, label: entry.label }], future: history.future.slice(0, -1), typing: null }, state: entry.state, label: entry.label };
}
