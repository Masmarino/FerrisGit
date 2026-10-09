import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { PendingChanges } from '../../shared/pending-changes';
import { FILE, FILE_URL, PARSE_URL, PROPOSAL_URL, RENDER_URL, answerRender, buttonNamed, fileBody, opened, parsed, rendered, setup, nextRequest, text, verifyRequestsAfterEach } from './pipeline-editor-testing';

describe('PipelineEditor saving', () => {
  verifyRequestsAfterEach();

  describe('saving', () => {
    async function edited() {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.patchJob('compile', { image: 'changed' });
      await answerRender(ctx, rendered({ yaml: 'stages:\n- build\n' }));
      return ctx;
    }

    const headerSave = (el: HTMLElement) => Array.from(el.querySelectorAll('gbt-page-header button')).find((b) => text(b) === 'Proposer la modification') as HTMLButtonElement;

    it('is offered only once something changed', async () => {
      const ctx = opened();
      await answerRender(ctx);
      expect(headerSave(ctx.el).disabled).toBe(true);

      ctx.internals.patchJob('compile', { image: 'changed' });
      await answerRender(ctx);

      expect(headerSave(ctx.el).disabled).toBe(false);
    });

    it('is not offered while the latest change has not been written by the server yet', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.patchJob('compile', { image: 'changed' });
      await answerRender(ctx);

      ctx.internals.patchJob('compile', { image: 'changed-again' });
      ctx.fixture.detectChanges();

      // The file shown is still the one rendered before this change, so proposing it would drop the change.
      expect(headerSave(ctx.el).disabled).toBe(true);
      await answerRender(ctx);
      expect(headerSave(ctx.el).disabled).toBe(false);
    });

    it('is not offered when the server could not write the latest change, even though an older file is shown', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.patchJob('compile', { image: 'changed' });
      await answerRender(ctx);

      ctx.internals.patchJob('compile', { image: 'changed-again' });
      (await nextRequest(ctx, RENDER_URL)).flush(null, { status: 500, statusText: 'Server Error' });
      ctx.fixture.detectChanges();

      expect(headerSave(ctx.el).disabled).toBe(true);
    });

    it('is not offered while the server reports problems', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.patchJob('compile', { image: 'changed' });

      await answerRender(ctx, rendered({ problems: [{ code: 'unknown_stage', message: 'x', job: 'compile', stage: 'nope' }] }));

      expect(ctx.internals.canSave()).toBe(false);
    });

    it('sends the file with the base it was opened at, then goes to the merge request', async () => {
      const ctx = await edited();
      const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);

      ctx.internals.openSave();
      ctx.fixture.detectChanges();
      ctx.internals.saveTitle.set('Run the build');
      ctx.internals.save();
      const request = ctx.http.expectOne(PROPOSAL_URL);
      request.flush({ branch: 'pipeline-editor/abc', commitSha: 'c1', mergeRequestId: 'mr-9' });

      expect(request.request.body).toEqual({ yaml: 'stages:\n- build\n', baseSha: 'tip1', title: 'Run the build', description: '' });
      expect(navigate).toHaveBeenCalledWith(['/repositories', 'acme', 'widget', '-', 'merge-requests', 'mr-9']);
    });

    it('says the file changed under the editor when the server answers 409', async () => {
      const ctx = await edited();
      ctx.internals.openSave();
      ctx.internals.save();

      ctx.http.expectOne(PROPOSAL_URL).flush({ error: 'changed' }, { status: 409, statusText: 'Conflict' });
      ctx.fixture.detectChanges();

      expect(text(ctx.fixture.nativeElement.ownerDocument.querySelector('gbt-modal'))).toContain("Le fichier a changé sur main depuis l'ouverture de l'éditeur");
    });

    it("shows the server's reason when it refuses the change", async () => {
      const ctx = await edited();
      ctx.internals.openSave();
      ctx.internals.save();

      ctx.http.expectOne(PROPOSAL_URL).flush({ error: 'the file is the same' }, { status: 400, statusText: 'Bad Request' });
      ctx.fixture.detectChanges();

      expect(text(ctx.fixture.nativeElement.ownerDocument.querySelector('gbt-modal'))).toContain('Le serveur refuse cette modification : the file is the same');
    });

    it('cannot be saved from a repository with no commit, and says what to do instead', async () => {
      const ctx = setup();
      ctx.fixture.detectChanges();
      ctx.http.expectOne(FILE_URL).flush(fileBody(null, { branch: null, baseSha: null }));
      ctx.fixture.detectChanges();
      ctx.internals.rename('build', 'compile');
      await answerRender(ctx);

      expect(ctx.internals.canSave()).toBe(false);
      expect(text(ctx.el.querySelector('.pipeline-editor__notes'))).toContain("aucun commit");
    });

    it('names the file where the repository keeps it', async () => {
      const ctx = setup();
      ctx.fixture.detectChanges();
      ctx.http.expectOne(FILE_URL).flush(fileBody(FILE, { path: 'ci/pipeline.yml' }));
      ctx.http.expectOne(PARSE_URL).flush(parsed());
      ctx.fixture.detectChanges();
      await answerRender(ctx);

      expect(text(ctx.el.querySelector('.pipeline-editor__yaml-head code'))).toBe('ci/pipeline.yml');
    });
  });

  describe('leaving with changes', () => {
    it('lets one leave freely while nothing changed', async () => {
      opened();

      expect(TestBed.inject(PendingChanges).canLeave()).toBe(true);
    });

    it('asks first once something changed, and stays when told to', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.moveTo({ name: 'compile' }, 'test');
      await answerRender(ctx);

      const answer = TestBed.inject(PendingChanges).canLeave() as Promise<boolean>;
      ctx.fixture.detectChanges();
      expect(Array.from(document.querySelectorAll('gbt-confirm-danger-modal'), text).join(' ')).toContain("Vos modifications n'ont pas été proposées");

      buttonNamed(document.body, "Rester dans l'éditeur").click();
      await expect(answer).resolves.toBe(false);
    });

    it('leaves once confirmed', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.moveTo({ name: 'compile' }, 'test');
      await answerRender(ctx);

      const answer = TestBed.inject(PendingChanges).canLeave() as Promise<boolean>;
      ctx.fixture.detectChanges();
      buttonNamed(document.body, 'Quitter sans proposer').click();

      await expect(answer).resolves.toBe(true);
    });

    it('asks the browser to confirm closing the tab, only with changes', async () => {
      const ctx = opened();
      await answerRender(ctx);
      const untouched = new Event('beforeunload', { cancelable: true });
      window.dispatchEvent(untouched);
      expect(untouched.defaultPrevented).toBe(false);

      ctx.internals.moveTo({ name: 'compile' }, 'test');
      const touched = new Event('beforeunload', { cancelable: true });
      window.dispatchEvent(touched);
      expect(touched.defaultPrevented).toBe(true);
      await answerRender(ctx);
    });

    it('goes to the merge request without asking once the change is proposed', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.patchJob('compile', { image: 'rust:2' });
      await answerRender(ctx);
      vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);

      ctx.internals.openSave();
      ctx.internals.save();
      ctx.http.expectOne(PROPOSAL_URL).flush({ mergeRequestId: 'mr-1', branch: 'pipeline-editor/1' });

      expect(TestBed.inject(PendingChanges).canLeave()).toBe(true);
    });

    it('stops being asked once the editor is gone', () => {
      const ctx = opened();
      ctx.internals.moveTo({ name: 'compile' }, 'test');

      ctx.fixture.destroy();

      expect(TestBed.inject(PendingChanges).canLeave()).toBe(true);
      const closing = new Event('beforeunload', { cancelable: true });
      window.dispatchEvent(closing);
      expect(closing.defaultPrevented).toBe(false);
      ctx.http.match(RENDER_URL);
    });
  });
});
