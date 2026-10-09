import { JobTile, ParamValues, TileBuild, TileParam, defaultValues } from './pipeline-tile-types';

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

const pathMessage = 'Lettres, chiffres et . _ / ~ - seulement, sans espace ni tiret au début.';
const k8sMessage = 'Minuscules, chiffres, - et . ; commence et finit par une lettre ou un chiffre.';

/** A tile with questions. Its default fields are what the default answers build. */
function questionTile(tile: Omit<JobTile, keyof TileBuild | 'needsSecrets'> & { params: TileParam[]; build: (values: ParamValues) => TileBuild }): JobTile {
  const defaults = tile.build(defaultValues(tile.params));
  return { ...tile, ...defaults, needsSecrets: (defaults.secrets ?? []).length > 0 };
}

// --- Docker image -----------------------------------------------------------------------------------------------

const dockerBuild = questionTile({
  id: 'docker-build',
  category: 'package',
  title: 'Construire et publier une image Docker',
  summary: 'docker build, puis docker push vers votre registre.',
  help: "Le runner ne donne pas accès au Docker de sa machine : la construction se fait sur un démon Docker distant que vous exploitez, désigné par le secret DOCKER_HOST (tcp://hôte:2376). S'il est protégé par TLS, ajoutez les secrets DOCKER_TLS_VERIFY et DOCKER_CERT_PATH. La connexion au registre utilise les secrets REGISTRY_USER et REGISTRY_PASSWORD.",
  icon: 'upload',
  jobName: 'image',
  stage: 'package',
  params: [
    { id: 'registry', label: 'Registre', kind: 'text', default: 'registry.example.com', required: true, placeholder: 'registry.example.com', hint: 'Où publier l\'image : registry.example.com, ghcr.io, docker.io…', pattern: /^[A-Za-z0-9][A-Za-z0-9.-]*(:[0-9]+)?$/, patternMessage: 'Un nom de serveur, avec un port si besoin.' },
    { id: 'image', label: "Nom de l'image", kind: 'text', default: 'equipe/application', required: true, placeholder: 'equipe/application', hint: 'Le chemin de l\'image dans le registre, sans la version.', pattern: /^[a-z0-9][a-z0-9._/-]*$/, patternMessage: 'Minuscules, chiffres et . _ / - seulement.' },
    { id: 'tag', label: 'Version (tag)', kind: 'text', default: 'latest', required: true, hint: "FerrisGit ne fournit ni numéro de commit ni nom de branche pour nommer la version : prenez un nom fixe, « latest » ou un numéro que vous changez à la main.", pattern: /^[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}$/, patternMessage: 'Lettres, chiffres et . _ - seulement.' },
    { id: 'dockerfile', label: 'Fichier Dockerfile', kind: 'text', default: 'Dockerfile', required: true, pattern: PATH, patternMessage: pathMessage },
    { id: 'context', label: 'Dossier de construction', kind: 'text', default: '.', required: true, hint: 'Le dossier envoyé au démon : « . » est la racine du dépôt.', pattern: PATH, patternMessage: pathMessage },
    { id: 'login', label: 'Se connecter au registre', kind: 'toggle', default: true, hint: 'Avec les secrets REGISTRY_USER et REGISTRY_PASSWORD. Décochez pour un registre sans authentification.' },
    { id: 'push', label: "Publier l'image après la construction", kind: 'toggle', default: true },
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
  { id: 'host', label: 'Serveur', kind: 'text', default: 'vm.example.com', required: true, placeholder: 'vm.example.com', hint: 'Le nom ou l\'adresse de la machine, joignable depuis le runner.', pattern: HOST, patternMessage: 'Un nom de machine ou une adresse IPv4.' },
  { id: 'user', label: 'Utilisateur', kind: 'text', default: 'deploy', required: true, hint: 'Le compte avec lequel se connecter : de préférence un compte dédié au déploiement.', pattern: NAME, patternMessage: 'Lettres, chiffres, _ et - seulement.' },
  { id: 'port', label: 'Port', kind: 'text', default: '22', required: true, pattern: PORT, patternMessage: 'Un nombre.' },
  {
    id: 'verify',
    label: "Vérifier l'identité du serveur",
    kind: 'choice',
    default: 'known-hosts',
    options: [
      { value: 'known-hosts', label: 'Oui (recommandé)' },
      { value: 'skip', label: 'Non' },
    ],
    hint: "Recommandé : le secret SSH_KNOWN_HOSTS contient l'empreinte du serveur, obtenue une fois avec ssh-keyscan -t ed25519 vm.example.com. Sans elle, n'importe qui se faisant passer pour le serveur recevrait vos fichiers.",
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

const sshRun = questionTile({
  id: 'ssh-run',
  category: 'deploy',
  title: 'Déployer sur une VM (SSH)',
  summary: 'Se connecte en SSH et lance vos commandes sur la machine.',
  help: "Le job installe le client SSH, dépose la clé du secret SSH_PRIVATE_KEY, puis exécute vos commandes sur la machine : redémarrer un service, tirer une nouvelle image avec docker compose… Créez une paire de clés dédiée, mettez la clé publique sur la machine et la clé privée dans le secret. Une pipeline part à chaque push : ce job se relance donc à chaque fois.",
  icon: 'server',
  jobName: 'deploy-vm',
  stage: 'deploy',
  params: [
    ...sshParams(),
    { id: 'directory', label: 'Dossier sur le serveur', kind: 'text', default: '/srv/app', hint: 'Les commandes partent de ce dossier. Laissez vide pour le dossier de connexion.', pattern: PATH, patternMessage: pathMessage },
    { id: 'commands', label: 'Commandes à lancer sur le serveur', kind: 'lines', default: 'docker compose pull\ndocker compose up -d', required: true, hint: 'Une par ligne, dans l\'ordre ; la suivante ne part que si la précédente a réussi.' },
  ],
  build: (values) => {
    const setup = sshSetup(values);
    return { image: 'alpine:3.20', script: ['apk add --no-cache openssh-client', ...setup.script, ...remote(values, lines(values['commands']))], secrets: setup.secrets };
  },
});

const sshCopy = questionTile({
  id: 'ssh-copy',
  category: 'deploy',
  title: 'Copier des fichiers sur une VM (SSH)',
  summary: 'Envoie un dossier du dépôt sur la machine avec rsync.',
  help: "Pratique pour un site statique ou un programme déjà construit : le dossier est envoyé avec rsync, seuls les fichiers modifiés voyagent. Les fichiers doivent exister dans le dépôt : un job ne reçoit rien d'un job précédent, construisez-les dans ce même job si besoin. Mêmes secrets que le déploiement SSH.",
  icon: 'server',
  jobName: 'copy-to-vm',
  stage: 'deploy',
  params: [
    ...sshParams(),
    { id: 'source', label: 'Dossier à envoyer', kind: 'text', default: 'dist/', required: true, hint: 'Un dossier du dépôt. Avec une barre finale (dist/), son contenu est envoyé ; sans (dist), le dossier lui-même.', pattern: PATH, patternMessage: pathMessage },
    { id: 'destination', label: 'Dossier sur le serveur', kind: 'text', default: '/var/www/app', required: true, hint: 'Créé s\'il n\'existe pas.', pattern: PATH, patternMessage: pathMessage },
    { id: 'delete', label: "Supprimer sur le serveur ce qui n'est plus dans le dossier", kind: 'toggle', default: false, hint: "Le dossier du serveur devient une copie exacte. À éviter s'il contient autre chose." },
    { id: 'after', label: 'Commandes à lancer ensuite sur le serveur', kind: 'lines', default: '', hint: 'Facultatif : recharger un service, par exemple. Une par ligne.' },
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
const KUBE_HELP = "Le job installe kubectl, écrit le contenu du secret KUBE_CONFIG (un fichier kubeconfig) là où l'outil le cherche, puis agit sur le cluster. Créez pour cela un compte de service aux droits limités au strict nécessaire (un espace de noms, quelques types d'objets), pas un accès administrateur.";

const kubeSetup = (): string[] => ['mkdir -p ~/.kube', `printf '%s' "$${KUBE_SECRET}" > ~/.kube/config`, 'chmod 600 ~/.kube/config'];

const namespaceParam = (): TileParam => ({ id: 'namespace', label: 'Espace de noms', kind: 'text', default: 'default', required: true, hint: 'Le « namespace » du cluster où déployer.', pattern: K8S_NAME, patternMessage: k8sMessage });

const kubeApply = questionTile({
  id: 'k8s-apply',
  category: 'deploy',
  title: 'Déployer des manifestes sur Kubernetes',
  summary: 'kubectl apply sur un dossier de fichiers YAML du dépôt.',
  help: `${KUBE_HELP} Les manifestes sont ceux du dépôt, avec les images déjà choisies : kubectl apply crée ce qui manque et met à jour le reste.`,
  icon: 'server',
  jobName: 'deploy-k8s',
  stage: 'deploy',
  params: [
    namespaceParam(),
    { id: 'manifests', label: 'Manifestes', kind: 'text', default: 'k8s/', required: true, hint: 'Un fichier YAML ou un dossier du dépôt.', pattern: PATH, patternMessage: pathMessage },
    { id: 'recursive', label: 'Inclure les sous-dossiers', kind: 'toggle', default: false },
    { id: 'wait', label: "Attendre la fin du déploiement d'un Deployment", kind: 'text', default: '', placeholder: 'application', hint: 'Facultatif : le nom d\'un Deployment. Le job n\'est réussi que lorsque ses pods sont prêts, et échoue sinon.', pattern: K8S_NAME, patternMessage: k8sMessage },
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

const kubeImage = questionTile({
  id: 'k8s-image',
  category: 'deploy',
  title: 'Mettre à jour un Deployment Kubernetes',
  summary: "Change l'image d'un Deployment, ou le redémarre, et attend qu'il soit prêt.",
  help: `${KUBE_HELP} Pour déployer la version qu'une étape précédente vient de publier, choisissez « Changer l'image » avec la même référence. Avec la version « latest », une référence inchangée ne relance rien : choisissez alors « Redémarrer ».`,
  icon: 'server',
  jobName: 'update-k8s',
  stage: 'deploy',
  params: [
    namespaceParam(),
    { id: 'deployment', label: 'Deployment', kind: 'text', default: 'application', required: true, pattern: K8S_NAME, patternMessage: k8sMessage },
    {
      id: 'mode',
      label: 'Action',
      kind: 'choice',
      default: 'set',
      options: [
        { value: 'set', label: "Changer l'image" },
        { value: 'restart', label: 'Redémarrer' },
      ],
    },
    { id: 'container', label: 'Conteneur', kind: 'text', default: '*', required: true, hint: '« * » met à jour tous les conteneurs du Deployment.', pattern: /^(\*|[a-z0-9]([-a-z0-9]*[a-z0-9])?)$/, patternMessage: k8sMessage, showIf: (values) => values['mode'] !== 'restart' },
    { id: 'image', label: 'Image', kind: 'text', default: 'registry.example.com/equipe/application:latest', required: true, hint: 'La référence complète, avec sa version.', pattern: IMAGE_REF, patternMessage: 'Une référence d\'image, sans espace.', showIf: (values) => values['mode'] !== 'restart' },
    { id: 'wait', label: "Attendre que le déploiement soit prêt", kind: 'toggle', default: true, hint: 'Le job échoue si les nouveaux pods ne deviennent pas prêts en trois minutes.' },
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

const helmUpgrade = questionTile({
  id: 'helm-upgrade',
  category: 'deploy',
  title: 'Déployer avec Helm',
  summary: 'helm upgrade --install sur un chart du dépôt ou d\'un dépôt de charts.',
  help: `${KUBE_HELP} Le chart peut être un dossier du dépôt (./chart). Pour un chart publié ailleurs, ajoutez d'abord sa commande « helm repo add » dans le tiroir du job.`,
  icon: 'server',
  jobName: 'helm',
  stage: 'deploy',
  params: [
    { id: 'release', label: 'Nom de la release', kind: 'text', default: 'application', required: true, pattern: K8S_NAME, patternMessage: k8sMessage },
    { id: 'chart', label: 'Chart', kind: 'text', default: './chart', required: true, hint: 'Un dossier du dépôt (./chart) ou une référence nom/chart.', pattern: PATH, patternMessage: pathMessage },
    namespaceParam(),
    { id: 'values', label: 'Fichier de valeurs', kind: 'text', default: '', placeholder: 'values-prod.yaml', hint: 'Facultatif : un fichier du dépôt qui adapte le chart.', pattern: PATH, patternMessage: pathMessage },
    { id: 'create', label: "Créer l'espace de noms s'il n'existe pas", kind: 'toggle', default: true },
    { id: 'wait', label: 'Attendre que tout soit prêt', kind: 'toggle', default: true, hint: 'Le job échoue si la release ne devient pas prête en cinq minutes.' },
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

export const DEPLOY_TILES: readonly JobTile[] = [dockerBuild, sshRun, sshCopy, kubeApply, kubeImage, helmUpgrade];
