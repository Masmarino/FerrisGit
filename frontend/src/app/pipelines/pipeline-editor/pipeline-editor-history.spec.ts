import { tileById } from './pipeline-catalog';
import { FILE, FILE_URL, PARSE_URL, RENDER_URL, answerRender, buttonNamed, cardNames, fileBody, lanes, opened, parsed, rendered, nextRequest, text, verifyRequestsAfterEach } from './pipeline-editor-testing';

describe('PipelineEditor history', () => {
  verifyRequestsAfterEach();

  describe('undo and redo', () => {
    it('undoes the last change, says what it undid, and redoes it', async () => {
      const ctx = opened();
      await answerRender(ctx);
      expect(ctx.internals.canUndo()).toBe(false);

      ctx.internals.moveTo({ name: 'compile' }, 'test');
      ctx.fixture.detectChanges();
      expect(ctx.internals.undoTip()).toMatch(/^Annuler : déplacement du job compile \((⌘Z|Ctrl\+Z)\)$/);

      expect(ctx.internals.undo()).toBe(true);
      ctx.fixture.detectChanges();
      expect(cardNames(lanes(ctx.el)[0])).toEqual(['compile']);
      expect(text(ctx.el.querySelector('p[role="status"]'))).toBe('Annulé : déplacement du job compile');
      expect(ctx.internals.canRedo()).toBe(true);

      expect(ctx.internals.redo()).toBe(true);
      ctx.fixture.detectChanges();
      expect(cardNames(lanes(ctx.el)[1])).toEqual(['unit', 'compile']);
      expect(text(ctx.el.querySelector('p[role="status"]'))).toBe('Rétabli : déplacement du job compile');
      await answerRender(ctx);
    });

    it('makes one step of what is typed in one field of a job', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.patchJob('compile', { image: 'r' });
      ctx.internals.patchJob('compile', { image: 'ru' });
      ctx.internals.patchJob('compile', { image: 'rust:2' });
      ctx.internals.undo();

      expect(ctx.internals.state().jobs.find((j) => j.name === 'compile')?.image).toBe('rust:1');
      expect(ctx.internals.canUndo()).toBe(false);
      await answerRender(ctx);
    });

    it('makes a step of each dependency ticked, however quick', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.patchJob('unit', { needs: [] });
      ctx.internals.patchJob('unit', { needs: ['compile'] });
      ctx.internals.undo();

      expect(ctx.internals.state().jobs.find((j) => j.name === 'unit')).toMatchObject({ needs: [] });
      expect(ctx.internals.canUndo()).toBe(true);
      await answerRender(ctx);
    });

    it('closes the drawer of a job that undoing took away', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.pickerStage.set('test');
      ctx.internals.chooseTile(tileById('custom'));
      expect(ctx.internals.selected()).not.toBeNull();

      ctx.internals.undo();

      expect(ctx.internals.selected()).toBeNull();
      expect(ctx.internals.state().jobs.map((j) => j.name)).toEqual(['compile', 'unit']);
      await answerRender(ctx);
    });

    it('answers ⌘Z and Ctrl+Y on the board, and leaves them to a field being typed in', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.moveTo({ name: 'compile' }, 'test');

      const field = ctx.el.querySelector<HTMLInputElement>('.pipeline-editor__stage-name input')!;
      field.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', metaKey: true, bubbles: true }));
      expect(ctx.internals.state().jobs.find((j) => j.name === 'compile')?.stage).toBe('test');

      const undoKey = new KeyboardEvent('keydown', { key: 'z', metaKey: true, bubbles: true, cancelable: true });
      document.body.dispatchEvent(undoKey);
      expect(ctx.internals.state().jobs.find((j) => j.name === 'compile')?.stage).toBe('build');
      expect(undoKey.defaultPrevented).toBe(true);

      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'y', ctrlKey: true, bubbles: true }));
      expect(ctx.internals.state().jobs.find((j) => j.name === 'compile')?.stage).toBe('test');
      await answerRender(ctx);
    });

    it('is not offered over the YAML, which the text field undoes itself', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.moveTo({ name: 'compile' }, 'test');
      await answerRender(ctx);

      ctx.internals.setMode('yaml');
      ctx.http.expectOne(RENDER_URL).flush(rendered());
      (await nextRequest(ctx, PARSE_URL)).flush(parsed());
      ctx.fixture.detectChanges();

      expect(ctx.internals.canUndo()).toBe(false);
      expect(ctx.el.querySelector('.pipeline-editor__history')).toBeNull();
    });

    it('starts afresh when the repository file is opened again', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.moveTo({ name: 'compile' }, 'test');
      ctx.fixture.detectChanges();

      buttonNamed(ctx.el, 'Revenir au fichier du dépôt').click();
      ctx.fixture.detectChanges();
      buttonNamed(document.body, 'Revenir au fichier').click();
      ctx.http.expectOne(FILE_URL).flush(fileBody(FILE));
      ctx.http.expectOne(PARSE_URL).flush(parsed());

      expect(ctx.internals.canUndo()).toBe(false);
      await answerRender(ctx);
    });

    it('does not offer to go back to the repository file while nothing changed', async () => {
      const ctx = opened();
      await answerRender(ctx);

      expect(buttonNamed(ctx.el, 'Revenir au fichier du dépôt').disabled).toBe(true);
    });
  });
});
