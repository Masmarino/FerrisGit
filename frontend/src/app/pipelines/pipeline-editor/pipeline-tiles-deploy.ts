import { JobTile, ParamValues, TileBuild, TileParam, defaultValues } from './pipeline-tile-types';
import { t } from '../../shared/i18n/translator';

/** Quotes a text for a POSIX shell, so that it reaches the command as a single argument whatever it contains. */
export const shQuote = (text: string): string => `'${text.replace(/'/g, `'\\''`)}'`;

const lines = (value: unknown): string[] =>
  String(value ?? '')
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '');

const text = (values: ParamValues, id: string): string => String(values[id] ?? '').trim();
const on = (values: ParamValues, id: string): boolean => values[id] === true;

const HOST = /^[A-Za-z0-9][A-Za-z0-9.-]*$/;
const NAME = /^[A-Za-z_][A-Za-z0-9_-]*$/;
const PORT = /^[0-9]{1,5}$/;
const PATH = /^[A-Za-z0-9_./~][A-Za-z0-9_./~-]*$/;
const K8S_NAME = /^[a-z0-9]([-a-z0-9.]*[a-z0-9])?$/;
const IMAGE_REF = /^[A-Za-z0-9][A-Za-z0-9._:/@-]*$/;

const pathMessage = (): string => t('pipelines.tiles.shared.pathMessage');
const k8sMessage = (): string => t('pipelines.tiles.shared.k8sMessage');

/** A tile with questions. Its default fields are what the default answers build. */
function questionTile(tile: Omit<JobTile, keyof TileBuild | 'needsSecrets'> & { params: TileParam[]; build: (values: ParamValues) => TileBuild }): JobTile {
  const defaults = tile.build(defaultValues(tile.params));
  return { ...tile, ...defaults, needsSecrets: (defaults.secrets ?? []).length > 0 };
}

// --- Docker image -----------------------------------------------------------------------------------------------

const dockerBuild = (): JobTile =>
  questionTile({
  id: 'docker-build',
  category: 'package',
  title: t('pipelines.tiles.docker-build.title'),
  summary: t('pipelines.tiles.docker-build.summary'),
  help: t('pipelines.tiles.docker-build.help'),
  icon: 'upload',
  jobName: 'image',
  stage: 'package',
  params: [
    { id: 'registry', label: t('pipelines.tiles.docker-build.params.registry.label'), kind: 'text', default: 'registry.example.com', required: true, placeholder: 'registry.example.com', hint: t('pipelines.tiles.docker-build.params.registry.hint'), pattern: /^[A-Za-z0-9][A-Za-z0-9.-]*(:[0-9]+)?$/, patternMessage: t('pipelines.tiles.docker-build.params.registry.patternMessage') },
    { id: 'image', label: t('pipelines.tiles.docker-build.params.image.label'), kind: 'text', default: 'equipe/application', required: true, placeholder: 'equipe/application', hint: t('pipelines.tiles.docker-build.params.image.hint'), pattern: /^[a-z0-9][a-z0-9._/-]*$/, patternMessage: t('pipelines.tiles.docker-build.params.image.patternMessage') },
    { id: 'tag', label: t('pipelines.tiles.docker-build.params.tag.label'), kind: 'text', default: 'latest', required: true, hint: t('pipelines.tiles.docker-build.params.tag.hint'), pattern: /^[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}$/, patternMessage: t('pipelines.tiles.docker-build.params.tag.patternMessage') },
    { id: 'dockerfile', label: t('pipelines.tiles.docker-build.params.dockerfile.label'), kind: 'text', default: 'Dockerfile', required: true, pattern: PATH, patternMessage: pathMessage() },
    { id: 'context', label: t('pipelines.tiles.docker-build.params.context.label'), kind: 'text', default: '.', required: true, hint: t('pipelines.tiles.docker-build.params.context.hint'), pattern: PATH, patternMessage: pathMessage() },
    { id: 'login', label: t('pipelines.tiles.docker-build.params.login.label'), kind: 'toggle', default: true, hint: t('pipelines.tiles.docker-build.params.login.hint') },
    { id: 'push', label: t('pipelines.tiles.docker-build.params.push.label'), kind: 'toggle', default: true },
  ],
  build: (values) => {
    const reference = `${text(values, 'registry')}/${text(values, 'image')}:${text(values, 'tag')}`;
    const docker = 'docker -H "$DOCKER_HOST"';
    const login = on(values, 'login');
    return {
      image: 'docker:27-cli',
      script: [
        ...(login ? [`echo "$REGISTRY_PASSWORD" | ${docker} login ${text(values, 'registry')} -u "$REGISTRY_USER" --password-stdin`] : []),
        `${docker} build -f ${text(values, 'dockerfile')} -t ${reference} ${text(values, 'context')}`,
        ...(on(values, 'push') ? [`${docker} push ${reference}`] : []),
      ],
      secrets: ['DOCKER_HOST', ...(login ? ['REGISTRY_USER', 'REGISTRY_PASSWORD'] : [])],
    };
  },
});

// --- Virtual machine over SSH ------------------------------------------------------------------------------------

const SSH_OPTIONS = '-i ~/.ssh/id_deploy -o IdentitiesOnly=yes';

const sshParams = (): TileParam[] => [
  { id: 'host', label: t('pipelines.tiles.ssh.params.host.label'), kind: 'text', default: 'vm.example.com', required: true, placeholder: 'vm.example.com', hint: t('pipelines.tiles.ssh.params.host.hint'), pattern: HOST, patternMessage: t('pipelines.tiles.ssh.params.host.patternMessage') },
  { id: 'user', label: t('pipelines.tiles.ssh.params.user.label'), kind: 'text', default: 'deploy', required: true, hint: t('pipelines.tiles.ssh.params.user.hint'), pattern: NAME, patternMessage: t('pipelines.tiles.ssh.params.user.patternMessage') },
  { id: 'port', label: t('pipelines.tiles.ssh.params.port.label'), kind: 'text', default: '22', required: true, pattern: PORT, patternMessage: t('pipelines.tiles.ssh.params.port.patternMessage') },
  {
    id: 'verify',
    label: t('pipelines.tiles.ssh.params.verify.label'),
    kind: 'choice',
    default: 'known-hosts',
    options: [
      { value: 'known-hosts', label: t('pipelines.tiles.ssh.params.verify.options.known-hosts') },
      { value: 'skip', label: t('pipelines.tiles.ssh.params.verify.options.skip') },
    ],
    hint: t('pipelines.tiles.ssh.params.verify.hint'),
  },
];

/** The commands that put the key (and the server's fingerprint) where ssh looks for them, for the given answers. */
function sshSetup(values: ParamValues): { script: string[]; secrets: string[]; options: string } {
  const verify = values['verify'] !== 'skip';
  return {
    script: [
      'mkdir -p ~/.ssh',
      'chmod 700 ~/.ssh',
      `printf '%s\\n' "$SSH_PRIVATE_KEY" > ~/.ssh/id_deploy`,
      'chmod 600 ~/.ssh/id_deploy',
      ...(verify ? [`printf '%s\\n' "$SSH_KNOWN_HOSTS" > ~/.ssh/known_hosts`] : []),
    ],
    secrets: ['SSH_PRIVATE_KEY', ...(verify ? ['SSH_KNOWN_HOSTS'] : [])],
    options: `${SSH_OPTIONS} -o StrictHostKeyChecking=${verify ? 'yes' : 'no -o UserKnownHostsFile=/dev/null'} -p ${text(values, 'port')}`,
  };
}

const remote = (values: ParamValues, commands: string[]): string[] => {
  const steps = [...(text(values, 'directory') ? [`cd ${text(values, 'directory')}`] : []), ...commands];
  return steps.length === 0 ? [] : [`ssh ${sshSetup(values).options} ${text(values, 'user')}@${text(values, 'host')} ${shQuote(steps.join(' && '))}`];
};

const sshRun = (): JobTile =>
  questionTile({
  id: 'ssh-run',
  category: 'deploy',
  title: t('pipelines.tiles.ssh-run.title'),
  summary: t('pipelines.tiles.ssh-run.summary'),
  help: t('pipelines.tiles.ssh-run.help'),
  icon: 'server',
  jobName: 'deploy-vm',
  stage: 'deploy',
  params: [
    ...sshParams(),
    { id: 'directory', label: t('pipelines.tiles.ssh-run.params.directory.label'), kind: 'text', default: '/srv/app', hint: t('pipelines.tiles.ssh-run.params.directory.hint'), pattern: PATH, patternMessage: pathMessage() },
    { id: 'commands', label: t('pipelines.tiles.ssh-run.params.commands.label'), kind: 'lines', default: 'docker compose pull\ndocker compose up -d', required: true, hint: t('pipelines.tiles.ssh-run.params.commands.hint') },
  ],
  build: (values) => {
    const setup = sshSetup(values);
    return { image: 'alpine:3.20', script: ['apk add --no-cache openssh-client', ...setup.script, ...remote(values, lines(values['commands']))], secrets: setup.secrets };
  },
});

const sshCopy = (): JobTile =>
  questionTile({
  id: 'ssh-copy',
  category: 'deploy',
  title: t('pipelines.tiles.ssh-copy.title'),
  summary: t('pipelines.tiles.ssh-copy.summary'),
  help: t('pipelines.tiles.ssh-copy.help'),
  icon: 'server',
  jobName: 'copy-to-vm',
  stage: 'deploy',
  params: [
    ...sshParams(),
    { id: 'source', label: t('pipelines.tiles.ssh-copy.params.source.label'), kind: 'text', default: 'dist/', required: true, hint: t('pipelines.tiles.ssh-copy.params.source.hint'), pattern: PATH, patternMessage: pathMessage() },
    { id: 'destination', label: t('pipelines.tiles.ssh-copy.params.destination.label'), kind: 'text', default: '/var/www/app', required: true, hint: t('pipelines.tiles.ssh-copy.params.destination.hint'), pattern: PATH, patternMessage: pathMessage() },
    { id: 'delete', label: t('pipelines.tiles.ssh-copy.params.delete.label'), kind: 'toggle', default: false, hint: t('pipelines.tiles.ssh-copy.params.delete.hint') },
    { id: 'after', label: t('pipelines.tiles.ssh-copy.params.after.label'), kind: 'lines', default: '', hint: t('pipelines.tiles.ssh-copy.params.after.hint') },
  ],
  build: (values) => {
    const setup = sshSetup(values);
    const destination = text(values, 'destination').replace(/\/+$/, '');
    const copy = `rsync -az ${on(values, 'delete') ? '--delete ' : ''}--mkpath -e "ssh ${setup.options}" ${text(values, 'source')} ${text(values, 'user')}@${text(values, 'host')}:${destination}/`;
    const after = lines(values['after']);
    return {
      image: 'alpine:3.20',
      script: ['apk add --no-cache openssh-client rsync', ...setup.script, copy, ...remote({ ...values, directory: '' }, after)],
      secrets: setup.secrets,
    };
  },
});

// --- Kubernetes ------------------------------------------------------------------------------------------------

const KUBE_SECRET = 'KUBE_CONFIG';
const kubeHelp = (): string => t('pipelines.tiles.shared.kubeHelp');

const kubeSetup = (): string[] => ['mkdir -p ~/.kube', `printf '%s' "$${KUBE_SECRET}" > ~/.kube/config`, 'chmod 600 ~/.kube/config'];

const namespaceParam = (): TileParam => ({ id: 'namespace', label: t('pipelines.tiles.shared.params.namespace.label'), kind: 'text', default: 'default', required: true, hint: t('pipelines.tiles.shared.params.namespace.hint'), pattern: K8S_NAME, patternMessage: k8sMessage() });

const kubeApply = (): JobTile =>
  questionTile({
  id: 'k8s-apply',
  category: 'deploy',
  title: t('pipelines.tiles.k8s-apply.title'),
  summary: t('pipelines.tiles.k8s-apply.summary'),
  help: t('pipelines.tiles.k8s-apply.help', { kubeHelp: kubeHelp() }),
  icon: 'server',
  jobName: 'deploy-k8s',
  stage: 'deploy',
  params: [
    namespaceParam(),
    { id: 'manifests', label: t('pipelines.tiles.k8s-apply.params.manifests.label'), kind: 'text', default: 'k8s/', required: true, hint: t('pipelines.tiles.k8s-apply.params.manifests.hint'), pattern: PATH, patternMessage: pathMessage() },
    { id: 'recursive', label: t('pipelines.tiles.k8s-apply.params.recursive.label'), kind: 'toggle', default: false },
    { id: 'wait', label: t('pipelines.tiles.k8s-apply.params.wait.label'), kind: 'text', default: '', placeholder: 'application', hint: t('pipelines.tiles.k8s-apply.params.wait.hint'), pattern: K8S_NAME, patternMessage: k8sMessage() },
  ],
  build: (values) => ({
    image: 'alpine:3.20',
    script: [
      'apk add --no-cache kubectl',
      ...kubeSetup(),
      `kubectl apply -n ${text(values, 'namespace')} ${on(values, 'recursive') ? '-R ' : ''}-f ${text(values, 'manifests')}`,
      ...(text(values, 'wait') ? [`kubectl rollout status deployment/${text(values, 'wait')} -n ${text(values, 'namespace')} --timeout=180s`] : []),
    ],
    secrets: [KUBE_SECRET],
  }),
});

const kubeImage = (): JobTile =>
  questionTile({
  id: 'k8s-image',
  category: 'deploy',
  title: t('pipelines.tiles.k8s-image.title'),
  summary: t('pipelines.tiles.k8s-image.summary'),
  help: t('pipelines.tiles.k8s-image.help', { kubeHelp: kubeHelp() }),
  icon: 'server',
  jobName: 'update-k8s',
  stage: 'deploy',
  params: [
    namespaceParam(),
    { id: 'deployment', label: t('pipelines.tiles.k8s-image.params.deployment.label'), kind: 'text', default: 'application', required: true, pattern: K8S_NAME, patternMessage: k8sMessage() },
    {
      id: 'mode',
      label: t('pipelines.tiles.k8s-image.params.mode.label'),
      kind: 'choice',
      default: 'set',
      options: [
        { value: 'set', label: t('pipelines.tiles.k8s-image.params.mode.options.set') },
        { value: 'restart', label: t('pipelines.tiles.k8s-image.params.mode.options.restart') },
      ],
    },
    { id: 'container', label: t('pipelines.tiles.k8s-image.params.container.label'), kind: 'text', default: '*', required: true, hint: t('pipelines.tiles.k8s-image.params.container.hint'), pattern: /^(\*|[a-z0-9]([-a-z0-9]*[a-z0-9])?)$/, patternMessage: k8sMessage(), showIf: (values) => values['mode'] !== 'restart' },
    { id: 'image', label: t('pipelines.tiles.k8s-image.params.image.label'), kind: 'text', default: 'registry.example.com/equipe/application:latest', required: true, hint: t('pipelines.tiles.k8s-image.params.image.hint'), pattern: IMAGE_REF, patternMessage: t('pipelines.tiles.k8s-image.params.image.patternMessage'), showIf: (values) => values['mode'] !== 'restart' },
    { id: 'wait', label: t('pipelines.tiles.k8s-image.params.wait.label'), kind: 'toggle', default: true, hint: t('pipelines.tiles.k8s-image.params.wait.hint') },
  ],
  build: (values) => {
    const target = `deployment/${text(values, 'deployment')}`;
    const namespace = `-n ${text(values, 'namespace')}`;
    return {
      image: 'alpine:3.20',
      script: [
        'apk add --no-cache kubectl',
        ...kubeSetup(),
        values['mode'] === 'restart' ? `kubectl rollout restart ${target} ${namespace}` : `kubectl set image ${target} ${shQuote(`${text(values, 'container')}=${text(values, 'image')}`)} ${namespace}`,
        ...(on(values, 'wait') ? [`kubectl rollout status ${target} ${namespace} --timeout=180s`] : []),
      ],
      secrets: [KUBE_SECRET],
    };
  },
});

const helmUpgrade = (): JobTile =>
  questionTile({
  id: 'helm-upgrade',
  category: 'deploy',
  title: t('pipelines.tiles.helm-upgrade.title'),
  summary: t('pipelines.tiles.helm-upgrade.summary'),
  help: t('pipelines.tiles.helm-upgrade.help', { kubeHelp: kubeHelp() }),
  icon: 'server',
  jobName: 'helm',
  stage: 'deploy',
  params: [
    { id: 'release', label: t('pipelines.tiles.helm-upgrade.params.release.label'), kind: 'text', default: 'application', required: true, pattern: K8S_NAME, patternMessage: k8sMessage() },
    { id: 'chart', label: t('pipelines.tiles.helm-upgrade.params.chart.label'), kind: 'text', default: './chart', required: true, hint: t('pipelines.tiles.helm-upgrade.params.chart.hint'), pattern: PATH, patternMessage: pathMessage() },
    namespaceParam(),
    { id: 'values', label: t('pipelines.tiles.helm-upgrade.params.values.label'), kind: 'text', default: '', placeholder: 'values-prod.yaml', hint: t('pipelines.tiles.helm-upgrade.params.values.hint'), pattern: PATH, patternMessage: pathMessage() },
    { id: 'create', label: t('pipelines.tiles.helm-upgrade.params.create.label'), kind: 'toggle', default: true },
    { id: 'wait', label: t('pipelines.tiles.helm-upgrade.params.wait.label'), kind: 'toggle', default: true, hint: t('pipelines.tiles.helm-upgrade.params.wait.hint') },
  ],
  build: (values) => ({
    image: 'alpine:3.20',
    script: [
      'apk add --no-cache helm',
      ...kubeSetup(),
      `helm upgrade --install ${text(values, 'release')} ${text(values, 'chart')} -n ${text(values, 'namespace')}${on(values, 'create') ? ' --create-namespace' : ''}${text(values, 'values') ? ` -f ${text(values, 'values')}` : ''}${on(values, 'wait') ? ' --wait --timeout 5m' : ''}`,
    ],
    secrets: [KUBE_SECRET],
  }),
});

export const deployTiles = (): JobTile[] => [dockerBuild(), sshRun(), sshCopy(), kubeApply(), kubeImage(), helmUpgrade()];
