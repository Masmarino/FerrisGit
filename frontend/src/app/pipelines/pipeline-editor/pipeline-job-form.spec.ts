import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { Checkbox } from '@masmarino/gabarit/checkbox';
import { TagInput } from '@masmarino/gabarit/tag-input';
import { BuilderJob } from './pipeline-builder-model';
import { PipelineJobForm } from './pipeline-job-form';

const JOB: BuilderJob = {
  name: 'unit',
  stage: 'test',
  image: 'rust:1',
  script: ['cargo test', 'cargo clippy'],
  variables: [{ key: 'RUST_LOG', value: 'debug' }],
  needs: ['compile'],
  tags: ['docker'],
  cache: ['cargo'],
};

const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();

describe('PipelineJobForm', () => {
  async function setup(job: BuilderJob = JOB, options: { needOptions?: string[]; problems?: { message: string; job: string | null }[]; secrets?: string[] | null; canManageSecrets?: boolean; engine?: string | null } = {}) {
    TestBed.configureTestingModule({});
    const fixture = TestBed.createComponent(PipelineJobForm);
    fixture.componentRef.setInput('job', job);
    fixture.componentRef.setInput('needOptions', options.needOptions ?? ['compile', 'lint']);
    fixture.componentRef.setInput('problems', options.problems ?? []);
    fixture.componentRef.setInput('secrets', options.secrets ?? null);
    fixture.componentRef.setInput('canManageSecrets', options.canManageSecrets ?? false);
    fixture.componentRef.setInput('engine', options.engine ?? null);
    const patches: Partial<BuilderJob>[] = [];
    fixture.componentInstance.patch.subscribe((patch) => patches.push(patch));
    const removed = vi.fn();
    fixture.componentInstance.remove.subscribe(removed);
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, el, patches, removed };
  }

  const input = (el: HTMLElement, id: string) => el.querySelector<HTMLInputElement>(`#${id}`)!;
  const type = (field: HTMLInputElement | HTMLTextAreaElement, value: string) => {
    field.value = value;
    field.dispatchEvent(new Event('input'));
  };
  const button = (el: HTMLElement, label: string) => Array.from(el.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label)!;

  it('shows the job as it is', async () => {
    const { el } = await setup();

    expect(input(el, 'job-name').value).toBe('unit');
    expect(input(el, 'job-image').value).toBe('rust:1');
    expect(el.querySelector<HTMLTextAreaElement>('#job-command-0')!.value).toBe('cargo test');
    expect(el.querySelector<HTMLTextAreaElement>('#job-command-1')!.value).toBe('cargo clippy');
    expect(el.querySelector<HTMLInputElement>('#job-variable-key-0')!.value).toBe('RUST_LOG');
    expect(el.querySelector<HTMLInputElement>('#job-variable-value-0')!.value).toBe('debug');
  });

  it('sends the image as the user types it', async () => {
    const { el, patches } = await setup();

    type(input(el, 'job-image'), 'node:22');

    expect(patches).toEqual([{ image: 'node:22' }]);
  });

  it('sends a command as it is typed, leaving the others alone', async () => {
    const { el, patches } = await setup();

    type(el.querySelector<HTMLTextAreaElement>('#job-command-1')!, 'cargo clippy -- -D warnings');

    expect(patches).toEqual([{ script: ['cargo test', 'cargo clippy -- -D warnings'] }]);
  });

  it('adds, moves and removes commands', async () => {
    const { el, patches } = await setup();

    button(el, 'Ajouter une commande').click();
    el.querySelector<HTMLButtonElement>('[aria-label="Monter la commande 2"]')!.click();
    el.querySelector<HTMLButtonElement>('[aria-label="Retirer la commande 1"]')!.click();

    expect(patches).toEqual([{ script: ['cargo test', 'cargo clippy', ''] }, { script: ['cargo clippy', 'cargo test'] }, { script: ['cargo clippy'] }]);
  });

  it('cannot move the first command up or the last one down', async () => {
    const { el } = await setup();

    expect(el.querySelector<HTMLButtonElement>('[aria-label="Monter la commande 1"]')!.disabled).toBe(true);
    expect(el.querySelector<HTMLButtonElement>('[aria-label="Descendre la commande 2"]')!.disabled).toBe(true);
  });

  it('renames only when the field is committed, not on every key', async () => {
    const { el, patches } = await setup();
    const field = input(el, 'job-name');

    type(field, 'integration');
    expect(patches).toEqual([]);
    field.dispatchEvent(new Event('change'));
    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));

    expect(patches.filter((p) => 'name' in p).length).toBeLessThanOrEqual(2);
    expect(patches.every((p) => p.name === 'integration')).toBe(true);
  });

  it('edits a variable, adds one and removes one', async () => {
    const { el, patches } = await setup();

    type(el.querySelector<HTMLInputElement>('#job-variable-value-0')!, 'trace');
    button(el, 'Ajouter une variable').click();
    el.querySelector<HTMLButtonElement>('[aria-label="Retirer la variable 1"]')!.click();

    expect(patches).toEqual([{ variables: [{ key: 'RUST_LOG', value: 'trace' }] }, { variables: [{ key: 'RUST_LOG', value: 'debug' }, { key: '', value: '' }] }, { variables: [] }]);
  });

  it('offers the possible dependencies and ticks those already waited for', async () => {
    const { fixture } = await setup();

    const boxes = fixture.debugElement.queryAll(By.directive(Checkbox));

    expect(boxes.map((b) => b.componentInstance.label())).toEqual(['compile', 'lint']);
    expect(boxes.map((b) => b.componentInstance.checked())).toEqual([true, false]);
  });

  it('adds a dependency that is ticked, after the ones it already waits for', async () => {
    const { fixture, patches } = await setup();
    const [, lint] = fixture.debugElement.queryAll(By.directive(Checkbox));

    lint.componentInstance.checked.set(true);
    fixture.detectChanges();

    expect(patches).toEqual([{ needs: ['compile', 'lint'] }]);
  });

  it('drops a dependency that is unticked', async () => {
    const { fixture, patches } = await setup();
    const [compile] = fixture.debugElement.queryAll(By.directive(Checkbox));

    compile.componentInstance.checked.set(false);
    fixture.detectChanges();

    expect(patches).toEqual([{ needs: [] }]);
  });

  it('still lists a dependency that is no longer possible, so it can be removed', async () => {
    const { fixture } = await setup({ ...JOB, needs: ['deploy'] }, { needOptions: ['compile'] });

    const labels = fixture.debugElement.queryAll(By.directive(Checkbox)).map((b) => b.componentInstance.label());

    expect(labels).toEqual(['compile', 'deploy']);
  });

  it('says when there is nothing to wait for', async () => {
    const { el } = await setup({ ...JOB, needs: [] }, { needOptions: [] });

    expect(text(el)).toContain('Aucun autre job dans cette étape ou une étape précédente.');
  });

  it('sends the runner tags and the caches', async () => {
    const { fixture, patches } = await setup();
    const [tags, cache] = fixture.debugElement.queryAll(By.directive(TagInput));

    tags.componentInstance.writeValue?.(['docker']);
    tags.triggerEventHandler('ngModelChange', ['docker', 'gpu']);
    cache.triggerEventHandler('ngModelChange', ['cargo', 'target']);

    expect(patches).toEqual([{ tags: ['docker', 'gpu'] }, { cache: ['cargo', 'target'] }]);
  });

  it('shows what the server says about the job', async () => {
    const { el } = await setup(JOB, { problems: [{ message: "Le job « unit » n'a pas d'image.", job: 'unit' }] });

    expect(text(el.querySelector('gbt-alert'))).toBe("Le job « unit » n'a pas d'image.");
  });

  it('asks to delete the job', async () => {
    const { el, removed } = await setup();

    button(el, 'Supprimer le job').click();

    expect(removed).toHaveBeenCalledTimes(1);
  });

  describe('help', () => {
    it('explains each part of the job in a bubble that opens on a click', async () => {
      const { el } = await setup();
      const tips = Array.from(el.querySelectorAll<HTMLButtonElement>('fg-help-tip button')).map((b) => b.getAttribute('aria-label'));

      expect(tips).toEqual(expect.arrayContaining(['Aide : Nom du job', 'Aide : Image', 'Aide : Commandes', 'Aide : Variables du job', 'Aide : Attend la fin de', 'Aide : Étiquettes du runner', 'Aide : Caches']));
    });
  });

  describe('images and caches', () => {
    it('fills the image from a suggestion', async () => {
      const { el, patches } = await setup();

      el.querySelector<HTMLButtonElement>('[data-image="node:22"] button, button[data-image="node:22"]')!.click();

      expect(patches).toEqual([{ image: 'node:22' }]);
    });

    it('adds a cache from a suggestion, and stops suggesting one that is there', async () => {
      const { el, patches } = await setup();

      expect(el.querySelector('[data-cache="cargo"]')).toBeNull();
      el.querySelector<HTMLButtonElement>('[data-cache="npm"] button, button[data-cache="npm"]')!.click();

      expect(patches).toEqual([{ cache: ['cargo', 'npm'] }]);
    });

    it('says caches do nothing when the instance runs jobs with Docker runners', async () => {
      const ctx = await setup({ ...JOB, cache: ['cargo'] }, { engine: 'docker-runners' });

      expect(text(ctx.el.querySelector('[data-note="cache"]'))).toContain('runners Docker');
    });

    it('is silent about caches with Kubernetes', async () => {
      const ctx = await setup({ ...JOB, cache: ['cargo'] }, { engine: 'kubernetes' });

      expect(ctx.el.querySelector('[data-note="cache"]')).toBeNull();
    });
  });

  describe('variables and secrets', () => {
    const withSecrets = { secrets: ['DEPLOY_TOKEN', 'REGISTRY_USER'], canManageSecrets: true };

    /** The menu's items only exist while it is open. */
    function insertItems(ctx: Awaited<ReturnType<typeof setup>>) {
      ctx.el.querySelector<HTMLButtonElement>('.job-form__insert .gbt-menu__trigger')!.click();
      ctx.fixture.detectChanges();
      return Array.from((ctx.fixture.nativeElement as HTMLElement).ownerDocument.querySelectorAll<HTMLButtonElement>('[data-insert]'));
    }

    it('inserts a variable at the cursor of the command last typed in', async () => {
      const ctx = await setup({ ...JOB, script: ['curl -H "$X" url'] }, withSecrets);
      const field = ctx.el.querySelector<HTMLTextAreaElement>('#job-command-0')!;
      field.focus();
      field.setSelectionRange(9, 11);

      insertItems(ctx).find((b) => b.dataset['insert'] === 'DEPLOY_TOKEN')!.click();

      expect(ctx.patches).toEqual([{ script: ['curl -H "$DEPLOY_TOKEN" url'] }]);
    });

    it('offers the job variables, then the secrets, to insert', async () => {
      const ctx = await setup(JOB, withSecrets);

      expect(insertItems(ctx).map((b) => b.dataset['insert'])).toEqual(['RUST_LOG', 'DEPLOY_TOKEN', 'REGISTRY_USER']);
    });

    it('has nothing to insert when there is no variable and no secret', async () => {
      const ctx = await setup({ ...JOB, variables: [] });

      expect(ctx.el.querySelector('.job-form__insert')).toBeNull();
    });

    it('adds a command when the variable is inserted before any command was touched', async () => {
      const ctx = await setup({ ...JOB, script: [] }, withSecrets);

      insertItems(ctx)[1].click();

      expect(ctx.patches).toEqual([{ script: ['$DEPLOY_TOKEN'] }]);
    });

    it('warns that a variable which looks like a secret is readable by everyone, and can turn it into one', async () => {
      const ctx = await setup({ ...JOB, variables: [{ key: 'API_TOKEN', value: 'abc' }] }, withSecrets);
      const made: number[] = [];
      ctx.fixture.componentInstance.makeSecret.subscribe((index) => made.push(index));

      expect(text(ctx.el.querySelector('[data-secret-warning="0"]'))).toContain('ressemble à un secret');
      button(ctx.el, 'En faire un secret').click();

      expect(made).toEqual([0]);
    });

    it('only says whom to ask when this person cannot create secrets', async () => {
      const ctx = await setup({ ...JOB, variables: [{ key: 'API_TOKEN', value: 'abc' }] });

      expect(text(ctx.el.querySelector('[data-secret-warning="0"]'))).toContain('Demandez à un mainteneur');
      expect(Array.from(ctx.el.querySelectorAll('button')).some((b) => text(b) === 'En faire un secret')).toBe(false);
    });

    it('does not warn about a harmless variable, or an empty one', async () => {
      const ctx = await setup({ ...JOB, variables: [{ key: 'RUST_LOG', value: 'debug' }, { key: 'API_TOKEN', value: '' }] });

      expect(ctx.el.querySelector('[data-secret-warning]')).toBeNull();
    });

    it('refuses a variable name a shell could not read', async () => {
      const ctx = await setup({ ...JOB, variables: [{ key: '1BAD', value: 'x' }] });

      expect(text(ctx.el.querySelector('.job-form__variable'))).toContain('sans commencer par un chiffre');
    });

    it('lists the secrets the commands read', async () => {
      const ctx = await setup({ ...JOB, script: ['curl "$DEPLOY_TOKEN"', 'echo $RUST_LOG'] }, withSecrets);

      expect(text(ctx.el.querySelector('.job-form__secrets'))).toBe('DEPLOY_TOKEN');
    });

    it('does not show the secrets block when this person cannot see the secrets', async () => {
      const ctx = await setup();

      expect(ctx.el.querySelector('.job-form__secrets, #job-secrets-title')).toBeNull();
    });

    it('flags a name read by the commands that nothing provides, and offers to create it when it is a secret', async () => {
      const ctx = await setup({ ...JOB, script: ['curl "$NOTIFY_URL"', 'echo $WHO', 'HOME_DIR=$HOME'] }, withSecrets);
      const asked: string[] = [];
      ctx.fixture.componentInstance.createSecret.subscribe((name) => asked.push(name));

      expect(text(ctx.el.querySelector('[data-note="unknown"]'))).toContain('$NOTIFY_URL, $WHO');
      button(ctx.el, 'Créer le secret NOTIFY_URL').click();
      expect(asked).toEqual(['NOTIFY_URL']);
      expect(Array.from(ctx.el.querySelectorAll('button')).some((b) => text(b) === 'Créer le secret WHO')).toBe(false);
    });
  });
});
