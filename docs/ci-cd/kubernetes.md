# Moteur Kubernetes

Avec le moteur Kubernetes, FerrisGit exécute chaque job de pipeline dans un Pod de votre cluster, sans runner à installer. Le Pod reçoit le dépôt au commit de la pipeline et les variables CI du dépôt, comme un job de runner Docker, et, si un administrateur l'a activé, son propre démon Docker. Cette page explique comment activer le moteur, les droits à donner, ce que FerrisGit crée dans le cluster, et ce qui le distingue encore des runners Docker (voir [Limites](#limites)).

## Activer le moteur

Un administrateur ouvre **Admin > Réglages**, section **Exécution** :

- carte **Moteur d'exécution** : choisissez **Kubernetes** (l'autre choix est **Docker / runners**). Chaque réglage est enregistré dès que vous le modifiez ;
- carte **Kubernetes** : **Namespace Kubernetes** et **StorageClass pour le cache (RWX requis)**.

Le changement de moteur s'applique aux prochaines pipelines. Une pipeline garde le moteur avec lequel elle a été créée, même si vous rebasculez ensuite ; ses jobs en attente sont lancés par ce moteur-là. Quand le moteur actif n'est pas Docker, l'entrée **Runners** disparaît du menu.

## Accès au cluster

FerrisGit cherche la configuration du cluster une seule fois, au démarrage du serveur :

- **Serveur dans le cluster** : il utilise le compte de service monté dans son Pod (comportement par défaut de Kubernetes). Rien d'autre à configurer que les droits ci-dessous.
- **Serveur hors du cluster** (Docker Compose, par exemple) : montez un fichier kubeconfig dans le conteneur et indiquez-le avec la variable `KUBECONFIG`.

Si aucune configuration n'est trouvée, le serveur démarre quand même, et les runners Docker continuent de fonctionner. En revanche, avec le moteur Kubernetes sélectionné, chaque job échoue au lancement : le serveur journalise « Kubernetes execution engine is not available on this server (no cluster was reachable at startup) » et le job passe à **Échoué** sans journal. Corrigez l'accès puis **redémarrez le serveur**.

## Namespace

Les Pods sont créés dans le namespace choisi ainsi, par ordre de priorité :

1. le champ **Namespace Kubernetes** des réglages, s'il est rempli ;
2. sinon le namespace détecté au démarrage (celui du Pod du serveur, ou celui du contexte du kubeconfig) ; l'interface l'affiche (« Détecté automatiquement depuis le cluster ») ;
3. sinon `ferrisgit-jobs`.

FerrisGit ne crée pas le namespace : il doit exister, avec les droits ci-dessous.

> **Attention** : si vous modifiez le namespace dans les réglages, **redémarrez le serveur**. L'observateur de Pods lit le namespace une seule fois au démarrage, alors que les nouveaux Pods utilisent la valeur à jour : sans redémarrage, les jobs restent **En cours** sans journal, car personne ne surveille leurs Pods.

## Droits RBAC

FerrisGit n'agit que dans un namespace. Il lui faut, dans ce namespace, un `Role` limité aux Pods, à leurs journaux, aux PVC de cache et à la création du Secret de chaque job, lié au compte de service qu'utilise le serveur. Voici le contenu exact du `Role` que crée le chart Helm de FerrisGit (`helm/ferrisgit/templates/rbac.yaml`), pour le namespace `ferrisgit` :

```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: ferrisgit-ci
  namespace: ferrisgit
---
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: ferrisgit-ci
  namespace: ferrisgit
rules:
  - apiGroups: [""]
    resources: ["pods", "pods/log"]
    verbs: ["get", "list", "watch", "create", "delete"]
  - apiGroups: [""]
    resources: ["persistentvolumeclaims"]
    verbs: ["get", "list", "create"]
  - apiGroups: [""]
    resources: ["secrets"]
    verbs: ["create"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata:
  name: ferrisgit-ci
  namespace: ferrisgit
subjects:
  - kind: ServiceAccount
    name: ferrisgit-ci
    namespace: ferrisgit
roleRef:
  kind: Role
  name: ferrisgit-ci
  apiGroup: rbac.authorization.k8s.io
```

Le Pod du serveur doit tourner avec ce compte de service (`serviceAccountName: ferrisgit-ci`). Pour les Secrets, ce rôle ne permet que d'en créer : FerrisGit ne peut ni les lire ni les lister, et Kubernetes supprime le Secret d'un job avec son Pod. Le chart Helm fourni avec FerrisGit le crée déjà (`kubernetesExecutor.enabled`, activé par défaut) : voir [Déploiement sur Kubernetes](/docs/administration/deploiement-kubernetes).

Si les Pods de jobs doivent tourner dans un autre namespace que celui du serveur, créez le `Role` et le `RoleBinding` dans **ce** namespace (en gardant le compte de service du serveur comme sujet), et renseignez ce namespace dans les réglages.

Un droit facultatif : lire les `StorageClass` (ressource à l'échelle du cluster) permet à FerrisGit de proposer la classe par défaut dans l'interface. Il faut pour cela un `ClusterRole` avec `get` et `list` sur `storage.k8s.io/storageclasses`, lié au même compte de service. Sans lui, le champ n'est simplement pas prérempli.

## Un Pod par job

Quand un job est disponible (voir [Ordre de lancement](/docs/ci-cd/reference-yaml#ordre-de-lancement) : première étape, ou étapes précédentes et `needs` réussis), FerrisGit crée un Pod, puis un Secret du même nom qui lui appartient :

| Propriété | Valeur |
|---|---|
| Nom | `ferrisgit-job-<identifiant du job>` |
| Étiquette | `ferrisgit.io/job-id: <identifiant du job>` |
| Redémarrage | `restartPolicy: Never` |
| Clonage | un conteneur d'initialisation `clone` (image `alpine/git`) copie le dépôt au commit de la pipeline dans `/workspace`, puis supprime `.git` |
| Conteneur | `job`, avec l'image du job, lancé depuis `/workspace` |
| Commande | `/bin/sh -c "<script>"` (lignes de `script` jointes par ` && `) |
| Variables | les `variables` du job, puis les variables CI du dépôt, lues dans le Secret ; une variable CI l'emporte sur une variable du fichier de même nom |
| Volumes | `/workspace`, et un volume monté sur `/ferrisgit-cache/<clé>` par clé de `cache` |
| Compte de service | aucun jeton monté : le job n'a pas accès à l'API Kubernetes |

Le conteneur `clone` s'authentifie avec un **jeton de job** : il ne lit que le dépôt de ce job, et seulement tant que le job n'est pas terminé. Le jeton est dans le Secret du job, jamais dans la description du Pod, et le conteneur `job` ne le reçoit pas. Le Pod joint le serveur à l'adresse `JOB_SERVER_URL` (par défaut `PUBLIC_URL` ; le chart Helm y met l'adresse du Service dans le cluster) : voir [Configuration](/docs/administration/configuration).

Le job passe à **En cours** dès que le Pod est créé. Il devient **Réussi** si le Pod finit en phase `Succeeded`, **Échoué** s'il finit en `Failed`. L'image doit contenir `/bin/sh`.

Si la création du Pod est refusée par le cluster (quota de Pods dépassé, namespace absent, droits insuffisants, image refusée par une règle d'admission), le job passe directement à **Échoué**, sans journal ; la cause est dans les journaux du serveur. Voir [Dépannage](/docs/ci-cd/depannage#pod-kubernetes-bloqué).

## Ressources

FerrisGit ne fixe aucune demande ni limite de CPU ou de mémoire, ne définit ni `imagePullSecrets`, ni sélecteur de nœud, et le conteneur du job n'a pas de `securityContext` (seul le démon Docker, quand il est activé, est privilégié). Les Pods prennent les valeurs par défaut du namespace :

- un `LimitRange` du namespace peut fournir des demandes et des limites par défaut ;
- si un `ResourceQuota` du namespace exige des demandes et des limites et qu'aucun `LimitRange` n'en fournit, le cluster refuse les Pods ;
- une image privée n'est téléchargeable que si le compte de service par défaut du namespace a un `imagePullSecret`.

## Docker dans les jobs

Quand un administrateur active **Donner Docker à chaque job** (**Admin > Réglages > Exécution**, carte **Docker dans les jobs**), chaque Pod reçoit en plus un conteneur `docker` : un démon Docker (`docker:dind`) lancé comme *sidecar* natif, avant le job, qui attend qu'il réponde. Le démon écoute sur une socket dans un volume partagé avec le job, jamais sur le réseau, et y copie son client avec les extensions buildx et compose. Le job reçoit `DOCKER_HOST` et `DOCKER_CONFIG`, et trouve `docker` en fin de `PATH` : `docker build`, `docker run`, `docker push` et `docker compose` fonctionnent quelle que soit l'image du job. Tout disparaît avec le Pod, images comprises : rien n'est gardé d'un job à l'autre.

Ce démon a besoin d'un conteneur **privilégié**, qui a la main sur le nœud qui l'exécute. Le namespace des jobs doit l'accepter (Pod Security Admission au niveau `privileged`, ou aucune restriction), et le cluster doit gérer les sidecars natifs (Kubernetes 1.29 ou plus récent). N'activez ce réglage que si vous faites confiance à toutes les personnes qui peuvent modifier un fichier de pipeline.

Les volumes que le job monte dans un `docker run` sont ceux du démon, pas les siens : pour donner des fichiers à un conteneur, construisez une image avec eux, ou copiez-les avec `docker cp`.

## Journaux

FerrisGit lit les journaux du Pod une fois, quand il est terminé (`Succeeded` ou `Failed`) : ceux du clonage s'il a échoué, puis ceux du job, et les enregistre dans le job. Pendant l'exécution, la page du job affiche « (pas encore de logs) ». Le journal mélange sortie standard et sortie d'erreur. Les valeurs des variables CI masquées y sont remplacées par `***` avant l'enregistrement.

## Nettoyage

- Quand un Pod est terminé, FerrisGit récupère ses journaux, enregistre le résultat, puis supprime le Pod.
- Annuler un job ou une pipeline supprime immédiatement son Pod.
- Les PVC de cache ne sont jamais supprimées par FerrisGit : voir [Caches](/docs/ci-cd/caches#invalidation).

Un Pod terminé dont la suppression échoue est laissé dans le cluster (avertissement dans les journaux du serveur) : cherchez les Pods `ferrisgit-job-*` avec `kubectl get pods -n <namespace>`.

## Limites

- **`tags` ignoré.** Les étiquettes n'ont d'effet qu'avec les runners Docker.
- **Journaux à la fin seulement.**
- **Le serveur doit être joignable depuis les Pods**, à l'adresse `JOB_SERVER_URL`, pour le clonage. Une `NetworkPolicy` qui filtre l'accès au serveur doit laisser passer les Pods des jobs (le chart Helm le fait).
- **Pas de délai maximal.** Un Pod qui n'atteint jamais l'état terminal (non planifiable, image introuvable en `ImagePullBackOff`, volume qui ne peut pas être lié) laisse son job **En cours** indéfiniment. Si vous supprimez un Pod à la main pendant qu'il s'exécute, FerrisGit ne s'en aperçoit pas non plus.
- **Pas de plafond de jobs simultanés** : le réglage **Jobs simultanés (runners Docker)** de **Admin > Réglages > Exécution** ne s'applique qu'aux runners Docker. FerrisGit crée un Pod pour chaque job dès qu'il est prêt, sans limite : ce plafond ne retient pas les jobs d'une pipeline Kubernetes.
- **Un seul namespace**, lu au démarrage pour la surveillance.
- **Une PVC `ReadWriteMany`** par clé de cache : la `StorageClass` doit le permettre. Voir [Caches](/docs/ci-cd/caches).
