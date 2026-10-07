import { execFileSync } from 'node:child_process';
import { JOB_TILES, PIPELINE_TEMPLATES, jobFromTile, stateFromTemplate, tileById } from './pipeline-catalog';
import { shQuote } from './pipeline-tiles-deploy';
import { ParamValues, buildTile, defaultValues, tileProblems } from './pipeline-tile-types';

const tile = (id: string) => tileById(id)!;
const script = (id: string, values: ParamValues = {}) => buildTile(tile(id), values).script;

/** Whether a shell accepts the commands, chained the way the runner chains them. */
function parses(commands: string[]): boolean {
  try {
    execFileSync('sh', ['-n', '-c', commands.join(' && ')], { stdio: 'pipe' });
    return true;
  } catch {
    return false;
  }
}

/** What a shell makes of a quoted text: it must come back as it went in. */
const throughShell = (value: string) => execFileSync('sh', ['-c', `printf %s ${shQuote(value)}`]).toString();

describe('deployment tiles', () => {
  const parametrised = JOB_TILES.filter((t) => t.params && t.params.length > 0);

  it('has a tile for Docker images, a VM over SSH (commands and files), Kubernetes (apply, image) and Helm', () => {
    expect(parametrised.map((t) => t.id)).toEqual(['docker-build', 'ssh-run', 'ssh-copy', 'k8s-apply', 'k8s-image', 'helm-upgrade']);
  });

  describe.each(parametrised.map((t) => [t.id, t] as const))('%s', (_id, t) => {
    it('is valid as it is: its default answers have no problem, and its fields are those answers', () => {
      expect(tileProblems(t, {})).toEqual({});
      expect(buildTile(t, defaultValues(t.params))).toMatchObject({ image: t.image, script: t.script });
    });

    it('writes commands that a shell parses', () => {
      expect(parses(t.script)).toBe(true);
    });

    it('asks questions that each have a label and a default, and a pattern for what goes in a command', () => {
      for (const param of t.params!) {
        expect(param.label, param.id).not.toBe('');
        if (param.kind === 'text' && param.id !== 'wait' && param.default !== '') {
          expect(param.pattern, `${t.id}.${param.id}`).toBeDefined();
          expect(param.pattern!.test(String(param.default)), `${t.id}.${param.id} default`).toBe(true);
        }
      }
    });

    it('names its secrets, and the commands read every one of them', () => {
      expect(t.secrets!.length).toBeGreaterThan(0);
      for (const name of t.secrets!) {
        expect(t.script.join('\n')).toContain(`$${name}`);
      }
    });

    it('cannot be given a value that breaks out of its command', () => {
      for (const param of t.params!.filter((p) => p.kind === 'text' && p.pattern)) {
        for (const evil of ['a b', 'a;rm -rf /', '$(id)', "a'b", 'a`id`', 'a&&b', 'a|b', '-rf']) {
          expect(param.pattern!.test(evil), `${t.id}.${param.id} accepts ${evil}`).toBe(false);
        }
      }
    });
  });

  describe('docker image', () => {
    it('logs in, builds and pushes against the remote daemon, by default', () => {
      expect(script('docker-build')).toEqual([
        'echo "$REGISTRY_PASSWORD" | docker -H "$DOCKER_HOST" login registry.example.com -u "$REGISTRY_USER" --password-stdin',
        'docker -H "$DOCKER_HOST" build -f Dockerfile -t registry.example.com/equipe/application:latest .',
        'docker -H "$DOCKER_HOST" push registry.example.com/equipe/application:latest',
      ]);
      expect(tile('docker-build').secrets).toEqual(['DOCKER_HOST', 'REGISTRY_USER', 'REGISTRY_PASSWORD']);
    });

    it('skips the login, and so its secrets, when told to', () => {
      const built = buildTile(tile('docker-build'), { login: false });

      expect(built.script).toHaveLength(2);
      expect(built.secrets).toEqual(['DOCKER_HOST']);
    });

    it('only builds when the image is not to be published', () => {
      expect(script('docker-build', { push: false, login: false })).toEqual(['docker -H "$DOCKER_HOST" build -f Dockerfile -t registry.example.com/equipe/application:latest .']);
    });

    it('uses the answers: registry, image, version, Dockerfile and context', () => {
      expect(script('docker-build', { registry: 'ghcr.io', image: 'acme/web', tag: '1.4.0', dockerfile: 'docker/Dockerfile.prod', context: 'app', login: false, push: false })[0]).toBe('docker -H "$DOCKER_HOST" build -f docker/Dockerfile.prod -t ghcr.io/acme/web:1.4.0 app');
    });
  });

  describe('VM over SSH', () => {
    it('installs the client, puts the key and the fingerprint in place, and runs the commands from the directory, chained', () => {
      expect(script('ssh-run')).toEqual([
        'apk add --no-cache openssh-client',
        'mkdir -p ~/.ssh',
        'chmod 700 ~/.ssh',
        `printf '%s\\n' "$SSH_PRIVATE_KEY" > ~/.ssh/id_deploy`,
        'chmod 600 ~/.ssh/id_deploy',
        `printf '%s\\n' "$SSH_KNOWN_HOSTS" > ~/.ssh/known_hosts`,
        `ssh -i ~/.ssh/id_deploy -o IdentitiesOnly=yes -o StrictHostKeyChecking=yes -p 22 deploy@vm.example.com 'cd /srv/app && docker compose pull && docker compose up -d'`,
      ]);
      expect(tile('ssh-run').secrets).toEqual(['SSH_PRIVATE_KEY', 'SSH_KNOWN_HOSTS']);
    });

    it('checks the identity of the server by default, and only skips it when asked, without the fingerprint secret', () => {
      const skipped = buildTile(tile('ssh-run'), { verify: 'skip' });

      expect(skipped.secrets).toEqual(['SSH_PRIVATE_KEY']);
      expect(skipped.script.join('\n')).toContain('StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null');
      expect(skipped.script.join('\n')).not.toContain('known_hosts');
      expect(parses(skipped.script)).toBe(true);
    });

    it('passes commands with quotes in them through as written', () => {
      const built = buildTile(tile('ssh-run'), { directory: '', commands: `echo "it's done"\nsystemctl restart app` });

      expect(built.script.at(-1)).toContain(shQuote(`echo "it's done" && systemctl restart app`));
      expect(parses(built.script)).toBe(true);
    });

    it('uses the host, user and port that were given', () => {
      expect(script('ssh-run', { host: '10.0.0.5', user: 'ci', port: '2222' }).at(-1)).toContain('-p 2222 ci@10.0.0.5 ');
    });
  });

  describe('files over SSH', () => {
    it('sends the folder with rsync, creating the destination, and nothing else by default', () => {
      const commands = script('ssh-copy');

      expect(commands[0]).toBe('apk add --no-cache openssh-client rsync');
      expect(commands.at(-1)).toBe('rsync -az --mkpath -e "ssh -i ~/.ssh/id_deploy -o IdentitiesOnly=yes -o StrictHostKeyChecking=yes -p 22" dist/ deploy@vm.example.com:/var/www/app/');
      expect(commands.filter((c) => c.startsWith('ssh '))).toEqual([]);
    });

    it('can mirror the folder, and runs the follow-up commands on the server', () => {
      const built = buildTile(tile('ssh-copy'), { delete: true, after: 'sudo systemctl reload nginx' });

      expect(built.script.join('\n')).toContain('rsync -az --delete --mkpath');
      expect(built.script.at(-1)).toContain(`deploy@vm.example.com 'sudo systemctl reload nginx'`);
      expect(parses(built.script)).toBe(true);
    });

    it('does not double the slash of a destination that ends with one', () => {
      expect(script('ssh-copy', { destination: '/srv/site/' }).at(-1)).toContain(':/srv/site/');
      expect(script('ssh-copy', { destination: '/srv/site/' }).at(-1)).not.toContain('//');
    });
  });

  describe('Kubernetes', () => {
    it('writes the kubeconfig from the secret and applies the manifests', () => {
      expect(script('k8s-apply')).toEqual([
        'apk add --no-cache kubectl',
        'mkdir -p ~/.kube',
        `printf '%s' "$KUBE_CONFIG" > ~/.kube/config`,
        'chmod 600 ~/.kube/config',
        'kubectl apply -n default -f k8s/',
      ]);
      expect(tile('k8s-apply').secrets).toEqual(['KUBE_CONFIG']);
    });

    it('waits for a Deployment when one is named, and goes through sub-folders when asked', () => {
      const commands = script('k8s-apply', { namespace: 'prod', recursive: true, wait: 'web' });

      expect(commands.at(-2)).toBe('kubectl apply -n prod -R -f k8s/');
      expect(commands.at(-1)).toBe('kubectl rollout status deployment/web -n prod --timeout=180s');
    });

    it('changes the image of a Deployment and waits for it, quoting the container so a star is not a glob', () => {
      const commands = script('k8s-image');

      expect(commands.at(-2)).toBe(`kubectl set image deployment/application '*=registry.example.com/equipe/application:latest' -n default`);
      expect(commands.at(-1)).toBe('kubectl rollout status deployment/application -n default --timeout=180s');
      expect(parses(commands)).toBe(true);
    });

    it('restarts a Deployment instead, without asking for an image', () => {
      const commands = script('k8s-image', { mode: 'restart', wait: false });

      expect(commands.at(-1)).toBe('kubectl rollout restart deployment/application -n default');
      expect(tileProblems(tile('k8s-image'), { mode: 'restart', image: '' })).toEqual({});
    });

    it('asks for an image only when it changes the image', () => {
      expect(tileProblems(tile('k8s-image'), { mode: 'set', image: '' })).toEqual({ image: 'À remplir.' });
    });

    it('runs helm upgrade --install with the options that were chosen', () => {
      expect(script('helm-upgrade').at(-1)).toBe('helm upgrade --install application ./chart -n default --create-namespace --wait --timeout 5m');
      expect(script('helm-upgrade', { values: 'values-prod.yaml', create: false, wait: false }).at(-1)).toBe('helm upgrade --install application ./chart -n default -f values-prod.yaml');
    });
  });

  it('asks the questions in French and refuses what no command could use', () => {
    expect(tileProblems(tile('ssh-run'), { host: 'vm; rm -rf /', user: '', port: 'abc' })).toEqual({
      host: 'Un nom de machine ou une adresse IPv4.',
      user: 'À remplir.',
      port: 'Un nombre.',
    });
  });

  it('makes a job from the answers', () => {
    const job = jobFromTile(tile('k8s-image'), 'deploy', [], { deployment: 'web', mode: 'restart' });

    expect(job).toMatchObject({ name: 'update-k8s', stage: 'deploy', image: 'alpine:3.20', tags: [] });
    expect(job.script.at(-2)).toBe('kubectl rollout restart deployment/web -n default');
  });

  describe('shQuote', () => {
    it.each(["plain", "it's", "a b", '$HOME', '`id`', 'a;b', '"q"', "'", '*', '\\', 'ligne\nsuivante'])('hands %j to a shell unchanged', (value) => {
      expect(throughShell(value)).toBe(value);
    });
  });

  describe('templates with a deployment', () => {
    it.each(['node-docker-ssh', 'node-docker-k8s'])('%s chains tests, image and deployment, each waiting for the one before', (id) => {
      const state = stateFromTemplate(PIPELINE_TEMPLATES.find((t) => t.id === id)!);

      expect(state.stages).toEqual(['test', 'package', 'deploy']);
      expect(state.jobs.map((j) => [j.name, j.needs])).toEqual([['unit-tests', []], ['image', ['unit-tests']], [id === 'node-docker-ssh' ? 'deploy-vm' : 'update-k8s', ['image']]]);
      for (const job of state.jobs) {
        expect(parses(job.script), job.name).toBe(true);
      }
    });
  });
});
