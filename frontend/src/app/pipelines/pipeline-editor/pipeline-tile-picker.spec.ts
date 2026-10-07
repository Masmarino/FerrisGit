import { TestBed } from '@angular/core/testing';
import { JOB_TILES, ParamValues, JobTile, TILE_CATEGORIES } from './pipeline-catalog';
import { PipelineTilePicker } from './pipeline-tile-picker';

const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();

describe('PipelineTilePicker', () => {
  function setup(secrets: string[] | null = null, engine: string | null = null) {
    TestBed.configureTestingModule({});
    const fixture = TestBed.createComponent(PipelineTilePicker);
    fixture.componentRef.setInput('stage', 'test');
    fixture.componentRef.setInput('secrets', secrets);
    fixture.componentRef.setInput('engine', engine);
    const chosen: { tile: JobTile; values: ParamValues }[] = [];
    fixture.componentInstance.chosen.subscribe((choice) => chosen.push(choice));
    fixture.detectChanges();
    return { fixture, el: fixture.nativeElement as HTMLElement, chosen };
  }

  it('says where the job is going', () => {
    const { el } = setup();

    expect(text(el.querySelector('.tile-picker__intro'))).toContain('étape test');
  });

  it('groups every tile under its category, in the order of the categories', () => {
    const { el } = setup();

    const headings = Array.from(el.querySelectorAll('.tile-picker__heading')).map(text);

    expect(headings).toEqual(TILE_CATEGORIES.filter((c) => JOB_TILES.some((t) => t.category === c.id)).map((c) => c.title));
    expect(el.querySelectorAll('.tile-picker__tile')).toHaveLength(JOB_TILES.length);
  });

  it('hands over the tile that was clicked', () => {
    const { el, chosen } = setup();

    el.querySelector<HTMLButtonElement>('[data-tile="go-test"] .tile-picker__choose')!.click();

    expect(chosen.map((c) => c.tile.id)).toEqual(['go-test']);
    expect(chosen[0].values).toEqual({});
  });

  it('explains each tile in a bubble of its own', () => {
    const { el } = setup();

    const tip = el.querySelector<HTMLButtonElement>('[data-tile="http-deploy"] fg-help-tip button')!;

    expect(tip.getAttribute('aria-label')).toBe('Aide : Déployer par un appel HTTP');
  });

  it('shows the secrets a tile needs that the repository lacks', () => {
    const { el } = setup(['DEPLOY_URL']);

    expect(text(el.querySelector('[data-tile="http-deploy"] [data-note="secrets"]'))).toBe('Secrets à créer : DEPLOY_TOKEN');
    expect(el.querySelector('[data-tile="go-test"] gbt-badge')).toBeNull();
  });

  it('gives each missing secret a badge of its own, so that a long list wraps instead of leaving the tile', () => {
    const { el } = setup([]);

    const note = el.querySelector('[data-tile="docker-build"] [data-note="secrets"]')!;
    expect(Array.from(note.querySelectorAll('gbt-badge'), text)).toEqual(['DOCKER_HOST', 'REGISTRY_USER', 'REGISTRY_PASSWORD']);
    // Read aloud as words, not as one run of names.
    expect(note.textContent!.replace(/\s+/g, ' ').trim()).toBe('Secrets à créer : DOCKER_HOST REGISTRY_USER REGISTRY_PASSWORD');
  });

  it('shows nothing about secrets when it cannot know which exist', () => {
    const { el } = setup(null);

    expect(el.querySelector('gbt-badge')).toBeNull();
  });

  describe('tiles with questions', () => {
    it('asks them before making the job, and hands over the answers', () => {
      const { fixture, el, chosen } = setup();

      el.querySelector<HTMLButtonElement>('[data-tile="ssh-run"] .tile-picker__choose')!.click();
      fixture.detectChanges();
      expect(chosen).toEqual([]);
      expect(el.querySelector('fg-pipeline-tile-form')).not.toBeNull();
      expect(el.querySelector('.tile-picker__tiles')).toBeNull();

      Array.from(el.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === 'Ajouter le job')!.click();

      expect(chosen.map((c) => c.tile.id)).toEqual(['ssh-run']);
      expect(chosen[0].values).toMatchObject({ host: 'vm.example.com', user: 'deploy', port: '22' });
    });

    it('goes back to the tiles without making anything', () => {
      const { fixture, el, chosen } = setup();
      el.querySelector<HTMLButtonElement>('[data-tile="k8s-apply"] .tile-picker__choose')!.click();
      fixture.detectChanges();

      Array.from(el.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === 'Retour')!.click();
      fixture.detectChanges();

      expect(chosen).toEqual([]);
      expect(el.querySelectorAll('.tile-picker__tile')).toHaveLength(JOB_TILES.length);
    });
  });

  describe('with Kubernetes running the jobs', () => {
    it('does not offer the tiles that read secrets, and says why', () => {
      const { el, chosen } = setup(null, 'kubernetes');
      const deploy = el.querySelector<HTMLButtonElement>('[data-tile="ssh-run"] .tile-picker__choose')!;

      deploy.click();

      expect(deploy.disabled).toBe(true);
      expect(text(el.querySelector('[data-tile="ssh-run"] [data-note="kubernetes"]'))).toBe("Pas avec Kubernetes Les secrets n'y sont pas transmis.");
      expect(chosen).toEqual([]);
      expect(el.querySelector('[data-tile="custom"] .tile-picker__choose')).toHaveProperty('disabled', false);
    });

    it('does not offer the tiles that work on the repository either: a Pod gets no copy of it', () => {
      const { el, chosen } = setup(null, 'kubernetes');
      const test = el.querySelector<HTMLButtonElement>('[data-tile="go-test"] .tile-picker__choose')!;

      test.click();

      expect(test.disabled).toBe(true);
      expect(text(el.querySelector('[data-tile="go-test"] [data-note="kubernetes"]'))).toBe("Pas avec Kubernetes Le dépôt n'y est pas copié.");
      expect(chosen).toEqual([]);
    });

    it('offers them all with Docker runners', () => {
      const { el } = setup(null, 'docker-runners');

      expect(el.querySelectorAll('.tile-picker__choose:disabled')).toHaveLength(0);
    });
  });
});
