import { signal } from '@angular/core';
import { HISTORY_LIMIT, TYPING_PAUSE_MS, createUndoStack, emptyHistory, record, redo, undo, undoShortcut } from './pipeline-history';

describe('pipeline history', () => {
  it('undoes changes one at a time, latest first, and redoes them in order', () => {
    let history = emptyHistory<string>();
    history = record(history, 'a', 'premier', null, 0);
    history = record(history, 'b', 'second', null, 10);

    const first = undo(history, 'c')!;
    expect(first.state).toBe('b');
    expect(first.label).toBe('second');
    const second = undo(first.history, first.state)!;
    expect(second.state).toBe('a');
    expect(second.label).toBe('premier');
    expect(undo(second.history, second.state)).toBeNull();

    const again = redo(second.history, second.state)!;
    expect(again.state).toBe('b');
    expect(again.label).toBe('premier');
    expect(redo(again.history, again.state)!.state).toBe('c');
  });

  it('makes one step of the keystrokes in a field, until a pause', () => {
    let history = emptyHistory<string>();
    history = record(history, '', 'saisie', 'job:lint:image', 0);
    history = record(history, 'r', 'saisie', 'job:lint:image', 200);
    history = record(history, 'ru', 'saisie', 'job:lint:image', 400);

    expect(history.past.map((entry) => entry.state)).toEqual(['']);

    history = record(history, 'rus', 'saisie', 'job:lint:image', 400 + TYPING_PAUSE_MS);
    expect(history.past.map((entry) => entry.state)).toEqual(['', 'rus']);
  });

  it('does not join keystrokes of another field, nor after an undo', () => {
    let history = emptyHistory<string>();
    history = record(history, 'a', 'saisie', 'job:lint:image', 0);
    history = record(history, 'b', 'saisie', 'job:unit:image', 100);
    expect(history.past).toHaveLength(2);

    const undone = undo(history, 'c')!;
    const after = record(undone.history, undone.state, 'saisie', 'job:unit:image', 150);
    expect(after.past).toHaveLength(2);
  });

  it('forgets what could be redone once something new is done', () => {
    let history = emptyHistory<string>();
    history = record(history, 'a', 'premier', null, 0);
    const undone = undo(history, 'b')!;

    const changed = record(undone.history, undone.state, 'autre', null, 10);

    expect(redo(changed, 'z')).toBeNull();
  });

  it('keeps the latest changes only', () => {
    let history = emptyHistory<number>();
    for (let n = 0; n < HISTORY_LIMIT + 5; n++) {
      history = record(history, n, 'pas', null, n);
    }
    expect(history.past).toHaveLength(HISTORY_LIMIT);
    expect(history.past[0].state).toBe(5);
  });

  describe('over a signal', () => {
    it('sets the signal, takes changes back and puts them back, saying which', () => {
      const value = signal('a');
      const stack = createUndoStack(value);

      stack.change('b', 'second');
      expect(value()).toBe('b');
      expect(stack.nextUndo()).toBe('second');

      expect(stack.undo()).toBe('second');
      expect(value()).toBe('a');
      expect(stack.nextRedo()).toBe('second');

      expect(stack.redo()).toBe('second');
      expect(value()).toBe('b');
      expect(stack.redo()).toBeNull();
    });

    it('records nothing for a change that changes nothing, and forgets everything when cleared', () => {
      const value = signal('a');
      const stack = createUndoStack(value);

      stack.change('a', 'rien');
      expect(stack.nextUndo()).toBeNull();

      stack.change('b', 'second');
      stack.clear();
      expect(stack.undo()).toBeNull();
      expect(value()).toBe('b');
    });
  });

  describe('the shortcut', () => {
    const key = (init: KeyboardEventInit, target: EventTarget = document.body) => {
      const event = new KeyboardEvent('keydown', { bubbles: true, ...init });
      Object.defineProperty(event, 'target', { value: target });
      return undoShortcut(event);
    };

    it('reads ⌘Z and Ctrl+Z as undo, with Shift or Ctrl+Y as redo', () => {
      expect(key({ key: 'z', metaKey: true })).toBe('undo');
      expect(key({ key: 'z', ctrlKey: true })).toBe('undo');
      expect(key({ key: 'Z', metaKey: true, shiftKey: true })).toBe('redo');
      expect(key({ key: 'y', ctrlKey: true })).toBe('redo');
    });

    it('leaves other keys, and those typed in a field, alone', () => {
      expect(key({ key: 'z' })).toBeNull();
      expect(key({ key: 'z', metaKey: true, altKey: true })).toBeNull();
      expect(key({ key: 'x', metaKey: true })).toBeNull();
      expect(key({ key: 'z', metaKey: true }, document.createElement('textarea'))).toBeNull();
    });
  });
});
