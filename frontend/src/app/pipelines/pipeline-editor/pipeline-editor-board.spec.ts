import { TestBed } from '@angular/core/testing';
import { tileById } from './pipeline-catalog';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { answerRender, card, cardNames, lanes, menuItems, opened, settle, stageNames, text, verifyRequestsAfterEach } from './pipeline-editor-testing';

describe('PipelineEditor board', () => {
  verifyRequestsAfterEach();

  describe('moving jobs', () => {
    it('drops a job on another stage at the place it was dropped', async () => {
      const ctx = opened();
      await answerRender(ctx);
      const dropped = { item: { data: 'compile' }, previousContainer: { id: 'a' }, container: { id: 'b' }, previousIndex: 0, currentIndex: 0 };

      ctx.internals.drop(dropped, 'test');
      ctx.fixture.detectChanges();

      expect(cardNames(lanes(ctx.el)[0])).toEqual([]);
      expect(cardNames(lanes(ctx.el)[1])).toEqual(['compile', 'unit']);
      await answerRender(ctx);
    });

    it('does nothing when a job is dropped where it was', async () => {
      const ctx = opened();
      await answerRender(ctx);
      const same = { id: 'a' };

      ctx.internals.drop({ item: { data: 'compile' }, previousContainer: same, container: same, previousIndex: 0, currentIndex: 0 }, 'build');

      expect(cardNames(lanes(ctx.el)[0])).toEqual(['compile']);
    });

    it('moves a job to the end of a stage from its menu, and announces it', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.moveTo({ name: 'compile' }, 'test');
      ctx.fixture.detectChanges();

      expect(cardNames(lanes(ctx.el)[1])).toEqual(['unit', 'compile']);
      expect(text(ctx.el.querySelector('p[role="status"]'))).toBe("Job compile déplacé vers l'étape test");
      await answerRender(ctx);
    });

    it('offers every card its actions, moving it to each other stage among them', async () => {
      const ctx = opened();
      await answerRender(ctx);

      expect(menuItems(ctx, 'compile')).toEqual(['Modifier', 'Dupliquer', 'Déplacer vers test', 'Supprimer']);
    });
  });

  describe('stages', () => {
    it('adds a stage from the form and clears the field', async () => {
      const ctx = opened();
      await answerRender(ctx);
      const input = ctx.el.querySelector<HTMLInputElement>('.pipeline-editor__new-stage input')!;
      input.value = 'deploy';
      input.dispatchEvent(new Event('input'));
      ctx.fixture.detectChanges();

      ctx.el.querySelector<HTMLFormElement>('.pipeline-editor__new-stage')!.dispatchEvent(new Event('submit'));
      ctx.fixture.detectChanges();
      await settle(ctx);

      expect(stageNames(ctx.el)).toEqual(['build', 'test', 'deploy']);
      await answerRender(ctx);
    });

    it('renames a stage and its jobs follow', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.rename('build', 'compile-all');
      ctx.fixture.detectChanges();
      await settle(ctx);

      expect(stageNames(ctx.el)).toEqual(['compile-all', 'test']);
      expect(cardNames(lanes(ctx.el)[0])).toEqual(['compile']);
      await answerRender(ctx);
    });
  });

  describe('editing a job', () => {
    it('opens a card in the drawer', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.el.querySelector<HTMLButtonElement>('[data-job="unit"] .pipeline-editor__card-open')!.click();
      ctx.fixture.detectChanges();
      // ngModel writes its value a tick after the form is created.
      await ctx.fixture.whenStable();
      ctx.fixture.detectChanges();

      expect(ctx.internals.selected()).toBe('unit');
      const fields = Array.from(ctx.el.querySelectorAll<HTMLInputElement>('fg-pipeline-job-form input')).map((input) => input.value);
      expect(fields.slice(0, 2)).toEqual(['unit', 'rust:1']);
    });

    it('adds a job to a stage and opens it', async () => {
      const ctx = opened();
      await answerRender(ctx);

      Array.from(lanes(ctx.el)[1].querySelectorAll('button')).find((b) => text(b) === 'Ajouter un job')!.click();
      ctx.fixture.detectChanges();
      ctx.internals.chooseTile(tileById('custom')!);
      ctx.fixture.detectChanges();

      expect(cardNames(lanes(ctx.el)[1])).toEqual(['unit', 'job']);
      expect(ctx.internals.selected()).toBe('job');
      await answerRender(ctx);
    });

    it('follows a job to its new name', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.patchJob('compile', {});
      ctx.el.querySelector<HTMLButtonElement>('[data-job="compile"] .pipeline-editor__card-open')!.click();

      ctx.internals.patchJob('compile', { name: 'build-all' });
      ctx.fixture.detectChanges();

      expect(ctx.internals.selected()).toBe('build-all');
      expect(ctx.internals.state().jobs.find((j) => j.name === 'unit')).toBeTruthy();
      await answerRender(ctx);
    });

    it('sends the change to the server after a pause, as the definition', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.patchJob('compile', { image: 'node:22' });
      ctx.fixture.detectChanges();
      const request = await answerRender(ctx);

      expect(request.request.body.definition.jobs.compile.image).toBe('node:22');
      expect(request.request.body.definition.jobs.unit.needs).toEqual(['compile']);
    });

    it('sends one request for a burst of changes', async () => {
      const ctx = opened();
      await answerRender(ctx);

      for (const image of ['n', 'no', 'nod', 'node']) {
        ctx.internals.patchJob('compile', { image });
        ctx.fixture.detectChanges();
      }
      const request = await answerRender(ctx);

      expect(request.request.body.definition.jobs.compile.image).toBe('node');
    });

    it('removes a job and closes the drawer', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.el.querySelector<HTMLButtonElement>('[data-job="compile"] .pipeline-editor__card-open')!.click();
      ctx.fixture.detectChanges();

      Array.from(ctx.el.querySelectorAll('fg-pipeline-job-form button')).find((b) => text(b) === 'Supprimer le job')!.dispatchEvent(new Event('click'));
      ctx.fixture.detectChanges();

      expect(cardNames(lanes(ctx.el)[0])).toEqual([]);
      expect(ctx.internals.selected()).toBeNull();
      expect(ctx.internals.state().jobs.find((j) => j.name === 'unit')).toMatchObject({ stage: 'test' });
      await answerRender(ctx);
    });
  });

  describe('card actions', () => {
    it('duplicates a job right after it, and opens the copy', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.duplicate('compile');
      ctx.fixture.detectChanges();

      expect(cardNames(lanes(ctx.el)[0])).toEqual(['compile', 'compile-2']);
      expect(ctx.internals.selected()).toBe('compile-2');
      expect(text(ctx.el.querySelector('p[role="status"]'))).toBe('Job compile copié en compile-2');
      await answerRender(ctx);
    });

    it('deletes a job from its menu without asking, and says how to undo it', async () => {
      const ctx = opened();
      await answerRender(ctx);

      menuItems(ctx, 'unit');
      Array.from(card(ctx.el, 'unit').querySelectorAll<HTMLButtonElement>('[role="menuitem"]')).find((b) => text(b) === 'Supprimer')!.click();
      ctx.fixture.detectChanges();

      expect(cardNames(lanes(ctx.el)[1])).toEqual([]);
      expect(TestBed.inject(GbtToastService).toasts().map((t) => t.message)).toEqual([expect.stringMatching(/^Job « unit » supprimé\. (⌘Z|Ctrl\+Z) pour l'annuler\.$/)]);

      ctx.internals.undo();
      ctx.fixture.detectChanges();
      expect(cardNames(lanes(ctx.el)[1])).toEqual(['unit']);
      await answerRender(ctx);
    });

    it('shows what a job is for, its last command, and how many come before it', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.patchJob('unit', { script: ['cd crates', '', 'cargo test --doc'] });
      ctx.fixture.detectChanges();

      expect(text(card(ctx.el, 'compile').querySelector('.pipeline-editor__card-command'))).toBe('cargo build');
      expect(text(card(ctx.el, 'unit').querySelector('.pipeline-editor__card-command-text'))).toBe('cargo test --doc');
      expect(text(card(ctx.el, 'unit').querySelector('.pipeline-editor__card-more'))).toBe('+1');
      expect(card(ctx.el, 'compile').querySelector('.pipeline-editor__card-more')).toBeNull();
      await answerRender(ctx);
    });

    it('marks the jobs a pointed job waits for, and those that wait for it', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.pointedJob.set('unit');
      ctx.fixture.detectChanges();
      expect(card(ctx.el, 'compile').dataset['relation']).toBe('waited');
      expect(card(ctx.el, 'unit').classList).toContain('pipeline-editor__card--pointed');

      ctx.internals.pointedJob.set('compile');
      ctx.fixture.detectChanges();
      expect(card(ctx.el, 'unit').dataset['relation']).toBe('waiting');

      card(ctx.el, 'compile').dispatchEvent(new MouseEvent('mouseleave'));
      ctx.fixture.detectChanges();
      expect(card(ctx.el, 'unit').dataset['relation']).toBeUndefined();
    });
  });

  describe('the ties between jobs', () => {
    it('draws one per need, as on a pipeline page', async () => {
      const ctx = opened();
      await answerRender(ctx);

      expect(ctx.internals.links()).toEqual([{ from: 'compile', to: 'unit', highlighted: false, invalid: false }]);
      expect(Array.from(ctx.el.querySelectorAll('fg-pipeline-links path'), (path) => path.getAttribute('data-link'))).toEqual(['compile->unit']);
    });

    it('brings out the ties of the pointed job, from either end', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.pointedJob.set('compile');
      ctx.fixture.detectChanges();

      expect(ctx.internals.links()[0].highlighted).toBe(true);
      expect(ctx.el.querySelector('fg-pipeline-links path')!.hasAttribute('data-highlighted')).toBe(true);
    });

    it('does not mark a need on a job of the same stage, which the server allows', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.moveTo({ name: 'compile' }, 'test');
      ctx.fixture.detectChanges();

      // The moved card takes the focus, so its tie may also stand out: only whether it is refused matters here.
      expect(ctx.internals.links()).toEqual([expect.objectContaining({ from: 'compile', to: 'unit', invalid: false })]);
      expect(ctx.el.querySelector('fg-pipeline-links path')!.hasAttribute('data-invalid')).toBe(false);
      await answerRender(ctx);
    });

    it('marks a need the server refuses: a job of a later stage', async () => {
      const ctx = opened();
      await answerRender(ctx);

      ctx.internals.patchJob('compile', { needs: ['unit'] });
      ctx.fixture.detectChanges();

      expect(ctx.internals.links()).toContainEqual(expect.objectContaining({ from: 'unit', to: 'compile', invalid: true }));
      expect(ctx.el.querySelector('fg-pipeline-links path[data-link="unit->compile"]')!.hasAttribute('data-invalid')).toBe(true);
      await answerRender(ctx);
    });

    it('leaves out a need of a job that does not exist, and hides the ties while a card is carried', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.patchJob('unit', { needs: ['compile', 'gone'] });
      ctx.fixture.detectChanges();
      expect(ctx.internals.links().map((link) => link.from)).toEqual(['compile']);

      ctx.internals.dragging.set(true);
      ctx.fixture.detectChanges();

      expect(ctx.el.querySelector('fg-pipeline-links')).toBeNull();
      await answerRender(ctx);
    });
  });
});
