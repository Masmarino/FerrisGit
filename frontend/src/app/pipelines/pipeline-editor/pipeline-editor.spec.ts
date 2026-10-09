import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { CopyButton } from '@masmarino/gabarit/copy-button';
import { By } from '@angular/platform-browser';
import { PageTitleService } from '../../shell/page-title.service';
import { FILE, FILE_URL, PARSE_URL, RENDER_URL, answerRender, buttonNamed, cardNames, fileBody, lanes, opened, parsed, rendered, setup, nextRequest, stageNames, text, verifyRequestsAfterEach } from './pipeline-editor-testing';

describe('PipelineEditor', () => {
  verifyRequestsAfterEach();

  it("opens the repository's file as stages with their jobs, in order", async () => {
    const ctx = opened();
    await answerRender(ctx);

    expect(stageNames(ctx.el)).toEqual(['build', 'test']);
    expect(cardNames(lanes(ctx.el)[0])).toEqual(['compile']);
    expect(cardNames(lanes(ctx.el)[1])).toEqual(['unit']);
    expect(text(ctx.el.querySelector('.pipeline-editor__summary'))).toBe('2 jobs dans 2 étapes');
    expect(text(ctx.el.querySelector('.pipeline-editor__needs'))).toBe('Après compile');
  });

  it("shows the YAML the server wrote, and copies exactly that", async () => {
    const ctx = opened();
    await answerRender(ctx, rendered({ yaml: 'stages:\n- build\njobs: {}\n' }));

    expect(text(ctx.el.querySelector('.pipeline-editor__yaml .code-view__code'))).toBe('stages: - build jobs: {}');
    expect(ctx.fixture.debugElement.query(By.directive(CopyButton)).componentInstance.value()).toBe('stages:\n- build\njobs: {}\n');
  });

  it('sets the page title', () => {
    opened();

    expect(TestBed.inject(PageTitleService).title()).toBe('Éditeur de pipeline');
  });

  it('starts from an empty pipeline when the repository has no file', async () => {
    const ctx = setup();
    ctx.fixture.detectChanges();
    ctx.http.expectOne(FILE_URL).flush(fileBody(null));
    ctx.fixture.detectChanges();
    await answerRender(ctx);

    expect(stageNames(ctx.el)).toEqual(['build', 'test']);
    expect(ctx.el.querySelectorAll('.pipeline-editor__card')).toHaveLength(0);
    expect(text(ctx.el.querySelector('.pipeline-editor__notes'))).toContain("n'a pas encore de fichier .ferrisgit-ci.yml");
  });

  it('says so, and starts empty, when the file is not valid YAML', async () => {
    const ctx = opened(parsed({ definition: null, problems: [{ code: 'invalid_yaml', message: 'invalid YAML: did not find expected node' }] }));
    await answerRender(ctx);

    expect(text(ctx.el.querySelector('.pipeline-editor__notes'))).toContain("Le fichier du dépôt n'a pas pu être lu.");
    expect(text(ctx.el.querySelector('.pipeline-editor__notes'))).toContain('did not find expected node');
    expect(stageNames(ctx.el)).toEqual(['build', 'test']);
  });

  it('warns about what a rewrite would lose: comments and fields the server ignores', async () => {
    const ctx = opened(parsed({ hasComments: true, ignoredFields: ['include', 'jobs.compile.when'] }));
    await answerRender(ctx);

    const notes = text(ctx.el.querySelector('.pipeline-editor__notes'));
    expect(notes).toContain('contient des commentaires');
    expect(notes).toContain('include, jobs.compile.when.');
  });

  it("offers a retry when the file cannot be loaded", () => {
    const ctx = setup();
    ctx.fixture.detectChanges();
    ctx.http.expectOne(FILE_URL).flush(null, { status: 500, statusText: 'Server Error' });
    ctx.fixture.detectChanges();

    expect(text(ctx.el)).toContain("Le fichier n'a pas pu être chargé");
    const retry = Array.from(ctx.el.querySelectorAll('button')).find((b) => text(b) === 'Réessayer')!;
    retry.click();
    ctx.http.expectOne(FILE_URL).flush(fileBody(null));
  });

  it('is for contributors: a reader gets an explanation, not the editor', () => {
    const ctx = setup('reader');
    ctx.fixture.detectChanges();
    ctx.http.expectOne(FILE_URL).flush(fileBody(null));
    ctx.fixture.detectChanges();

    expect(text(ctx.el)).toContain('Réservé aux contributeurs');
    expect(ctx.el.querySelector('.pipeline-editor__board')).toBeNull();
  });

  describe('what the server says', () => {
    it('lists the problems, marks the job and opens it from the list', async () => {
      const ctx = opened();
      await answerRender(
        ctx,
        rendered({ problems: [{ code: 'unknown_dependency', message: 'x', job: 'unit', dependency: 'ghost' }] }),
      );

      expect(text(ctx.el.querySelector('.pipeline-editor__issues'))).toBe("Le job « unit » dépend de « ghost », qui n'existe pas.");
      expect(text(ctx.el.querySelector('[data-job="unit"] gbt-badge'))).toBe('1 problème');
      expect(ctx.el.querySelector('[data-job="unit"]')?.classList).toContain('pipeline-editor__card--problem');

      ctx.el.querySelector<HTMLButtonElement>('.pipeline-editor__issue-link')!.click();
      expect(ctx.internals.selected()).toBe('unit');
    });

    it('lists the warnings apart from the problems', async () => {
      const ctx = opened();
      await answerRender(ctx, rendered({ warnings: [{ code: 'empty_image', job: 'compile' }] }));

      expect(text(ctx.el)).toContain("Le job « compile » n'a pas d'image.");
      expect(text(ctx.el)).toContain('À vérifier');
      expect(text(ctx.el)).not.toContain('À corriger');
    });

    it('says when the server could not produce the file, and keeps the pipeline', async () => {
      const ctx = opened();
      (await nextRequest(ctx, RENDER_URL)).flush(null, { status: 500, statusText: 'Server Error' });
      ctx.fixture.detectChanges();

      expect(text(ctx.el)).toContain("Le serveur n'a pas pu produire le fichier");
      expect(cardNames(lanes(ctx.el)[0])).toEqual(['compile']);
    });
  });

  it('goes back to the pipeline list', () => {
    const ctx = opened();
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);

    Array.from(ctx.el.querySelectorAll('button')).find((b) => text(b) === 'Retour aux pipelines')!.click();

    expect(navigate).toHaveBeenCalledWith(['/repositories', 'acme', 'widget', '-', 'pipelines']);
  });

  it('starts again from the repository file, once asked', async () => {
    const ctx = opened();
    await answerRender(ctx);
    ctx.internals.patchJob('compile', { image: 'changed' });
    ctx.fixture.detectChanges();

    buttonNamed(ctx.el, 'Revenir au fichier du dépôt').click();
    ctx.fixture.detectChanges();
    expect(Array.from(document.querySelectorAll('gbt-confirm-danger-modal'), text).join(' ')).toContain('Les modifications faites ici seront perdues');
    ctx.http.expectNone(FILE_URL);

    buttonNamed(document.body, 'Revenir au fichier').click();
    ctx.fixture.detectChanges();
    ctx.http.expectOne(FILE_URL).flush(fileBody(FILE));
    ctx.http.expectOne(PARSE_URL).flush(parsed());
    ctx.fixture.detectChanges();

    expect(ctx.internals.state().jobs.find((j) => j.name === 'compile')?.image).toBe('rust:1');
    await answerRender(ctx);
  });
});
