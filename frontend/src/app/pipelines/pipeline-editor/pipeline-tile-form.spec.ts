import { TestBed } from '@angular/core/testing';
import { JobTile, ParamValues, tileById } from './pipeline-catalog';
import { PipelineTileForm } from './pipeline-tile-form';

const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();

describe('PipelineTileForm', () => {
  async function setup(tile: JobTile, secrets: string[] | null = null) {
    TestBed.configureTestingModule({});
    const fixture = TestBed.createComponent(PipelineTileForm);
    fixture.componentRef.setInput('tile', tile);
    fixture.componentRef.setInput('stage', 'deploy');
    fixture.componentRef.setInput('secrets', secrets);
    const confirmed: ParamValues[] = [];
    fixture.componentInstance.confirmed.subscribe((values) => confirmed.push(values));
    const back = vi.fn();
    fixture.componentInstance.back.subscribe(back);
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    return { fixture, el: fixture.nativeElement as HTMLElement, confirmed, back };
  }

  const field = (el: HTMLElement, id: string) => el.querySelector<HTMLInputElement | HTMLTextAreaElement>(`#tile-param-${id}`)!;
  const type = (el: HTMLElement, id: string, value: string) => {
    const input = field(el, id);
    input.value = value;
    input.dispatchEvent(new Event('input'));
  };
  const button = (el: HTMLElement, label: string) => Array.from(el.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label)!;

  it('shows a field for each question, filled with its default', async () => {
    const { el } = await setup(tileById('ssh-run')!);

    expect(field(el, 'host').value).toBe('vm.example.com');
    expect(field(el, 'user').value).toBe('deploy');
    expect(field(el, 'port').value).toBe('22');
    expect(field(el, 'commands').value).toBe('docker compose pull\ndocker compose up -d');
  });

  it('shows what the job will run, and updates it as the answers change', async () => {
    const { fixture, el } = await setup(tileById('ssh-run')!);
    expect(text(el.querySelector('.tile-form__preview'))).toContain('deploy@vm.example.com');

    type(el, 'host', '10.0.0.5');
    fixture.detectChanges();

    expect(text(el.querySelector('.tile-form__preview'))).toContain('deploy@10.0.0.5');
    expect(text(el.querySelector('.tile-form__preview'))).not.toContain('vm.example.com');
  });

  it('says where the job is going and in which image it runs', async () => {
    const { el } = await setup(tileById('k8s-apply')!);

    expect(text(el.querySelector('.tile-form__lede'))).toContain('étape deploy');
    expect(text(el.querySelector('#tile-form-preview')?.parentElement)).toContain('alpine:3.20');
  });

  it('hands over every answer, defaults included, when the job is added', async () => {
    const { el, confirmed } = await setup(tileById('ssh-run')!);

    type(el, 'port', '2222');
    button(el, 'Ajouter le job').click();

    expect(confirmed).toHaveLength(1);
    expect(confirmed[0]).toMatchObject({ host: 'vm.example.com', port: '2222', verify: 'known-hosts' });
  });

  it('does not add a job with an answer that no command could use, and shows every problem', async () => {
    const { fixture, el, confirmed } = await setup(tileById('ssh-run')!);

    type(el, 'host', 'vm; reboot');
    type(el, 'user', '');
    button(el, 'Ajouter le job').click();
    fixture.detectChanges();

    expect(confirmed).toEqual([]);
    expect(text(field(el, 'host').closest('[data-param]'))).toContain('Un nom de machine ou une adresse IPv4.');
    expect(text(field(el, 'user').closest('[data-param]'))).toContain('À remplir.');
  });

  it('does not scold a field before it is touched', async () => {
    const { el } = await setup(tileById('ssh-run')!);

    expect(el.querySelector('.gbt-input__error, [role="alert"]')).toBeNull();
  });

  it('shows a question only when the answers to the others call for it', async () => {
    const { fixture, el } = await setup(tileById('k8s-image')!);
    expect(el.querySelector('[data-param="image"]')).not.toBeNull();

    fixture.componentInstance['set'](tileById('k8s-image')!.params!.find((p) => p.id === 'mode')!, 'restart');
    fixture.detectChanges();

    expect(el.querySelector('[data-param="image"]')).toBeNull();
    expect(el.querySelector('[data-param="container"]')).toBeNull();
    expect(text(el.querySelector('.tile-form__preview'))).toContain('rollout restart');
  });

  it('lists the secrets the job will read, saying which exist when it can know', async () => {
    const known = await setup(tileById('ssh-run')!, ['SSH_PRIVATE_KEY']);

    expect(text(known.el.querySelector('[data-secret="SSH_PRIVATE_KEY"]'))).toContain('Déjà créé');
    expect(text(known.el.querySelector('[data-secret="SSH_KNOWN_HOSTS"]'))).toContain('À créer');
  });

  it('only names the secrets when it cannot know which exist', async () => {
    const { el } = await setup(tileById('k8s-apply')!, null);

    expect(text(el.querySelector('[data-secret="KUBE_CONFIG"]'))).toBe('KUBE_CONFIG');
  });

  it('follows the secrets to the answers: no login, no registry secrets', async () => {
    const { fixture, el } = await setup(tileById('docker-build')!);
    expect(el.querySelector('[data-secret="REGISTRY_USER"]')).not.toBeNull();

    fixture.componentInstance['set'](tileById('docker-build')!.params!.find((p) => p.id === 'login')!, false);
    fixture.detectChanges();

    expect(el.querySelector('[data-secret="REGISTRY_USER"]')).toBeNull();
    expect(el.querySelector('[data-secret="DOCKER_HOST"]')).not.toBeNull();
  });

  it('goes back without answering', async () => {
    const { el, back, confirmed } = await setup(tileById('ssh-run')!);

    button(el, 'Retour').click();

    expect(back).toHaveBeenCalledTimes(1);
    expect(confirmed).toEqual([]);
  });
});
