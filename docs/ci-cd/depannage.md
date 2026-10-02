# Dépannage

Cette page suit les symptômes : un pipeline n'apparaît pas, un job reste en attente, un job échoue, un cache manque, un runner ou Kubernetes ne répond pas. Les messages cités sont ceux du code ; beaucoup ne sont visibles que dans les journaux du serveur ou du runner, c'est précisé à chaque fois.

> **Note** : les journaux du serveur sont ceux du conteneur ou du processus `ferrisgit-api` (par exemple `docker compose logs ferrisgit-server`, ou `kubectl logs` sur le Pod du serveur). Les erreurs de création de pipeline sont au niveau `ERROR`.

## Aucun pipeline n'apparaît après un push

Le push réussit toujours, même quand aucun pipeline n'est créé. Vérifiez dans l'ordre :

1. **La CI est-elle activée ?** **Réglages > Pipeline > CI activée pour ce dépôt** (rôle Mainteneur).
2. **Le fichier existe-t-il où FerrisGit le cherche ?** Par défaut `.ferrisgit-ci.yml` à la racine ; le chemin se règle dans **Chemin du fichier pipeline**. Un fichier absent ne produit aucun message.
3. **Le fichier est-il présent sur le commit lu ?** FerrisGit lit le fichier sur le commit de `HEAD` (la branche par défaut du dépôt), pas sur la branche que vous venez de pousser. Un fichier ajouté seulement sur une branche de travail n'est pas vu avant la fusion.
4. **Le fichier est-il valide ?** Un fichier invalide donne un pipeline **Échoué** visible dans la liste, pas une absence de pipeline : voir la section suivante.
5. Le push visait-il le wiki du dépôt ? Un push vers un wiki ne déclenche rien.

## Le fichier est invalide

Un fichier de pipeline présent mais invalide ne bloque pas le push : FerrisGit crée un pipeline **Échoué**, sans job, rattaché au commit. Ouvrez-le dans **Pipelines** : la liste le marque « Fichier de pipeline invalide », et son détail affiche l'encadré **Fichier de pipeline invalide** avec le message de l'analyseur, en police à chasse fixe. Vous êtes aussi notifié comme pour tout pipeline échoué. L'erreur n'apparaît pas dans la sortie de `git push`, qui ne la connaît pas : elle est écrite dans l'interface (et renvoyée par l'API dans le champ `error`, voir [Pipelines et runners (API)](/docs/api/ci-cd)).

Corrigez le fichier et poussez à nouveau : un nouveau pipeline est créé. Les messages possibles et leur remède :

| Message | Cause et remède |
|---|---|
| `invalid YAML: …` | YAML mal formé, clé obligatoire absente (`stages`, `jobs`, `stage`, `image`, `script`) ou valeur du mauvais type. Le message donne le chemin de la clé et la position. Un deux-points suivi d'un espace dans une ligne de script non quotée est une cause fréquente : mettez la ligne entre guillemets simples. |
| `job '<job>' references stage '<étape>' which is not declared in `stages`` | Ajoutez l'étape à `stages` ou corrigez `stage`. |
| `job '<job>' has a `needs` entry '<dépendance>' which is not a declared job` | Le nom dans `needs` n'est pas le nom exact d'un job. |
| `job '<job>' needs '<dépendance>', but that job's stage does not come before '<job>''s own stage` | La dépendance est dans une étape ultérieure. Déplacez l'un des deux jobs ou réordonnez `stages`. |
| `job '<job>' declares cache key '<clé>', which is not valid: …` | Une clé de cache doit respecter `^[a-z0-9-]+$` (minuscules, chiffres, tirets). |
| `` `needs` form a cycle: a -> b -> a `` | Des jobs s'attendent mutuellement (ou un job se déclare lui-même). Retirez une des dépendances du cycle, dont les jobs sont cités dans le message. |
| `pipeline file is not valid UTF-8: …` | Réenregistrez le fichier en UTF-8. |

FerrisGit n'a pas d'outil de vérification séparé : relisez le fichier avec [la référence](/docs/ci-cd/reference-yaml#erreurs-de-validation) avant de pousser. Seule la première erreur est rapportée à chaque fois.

Le pipeline de la fusion d'une demande de fusion se comporte de la même façon : il apparaît **Échoué** avec son message.

## Le pipeline reste « En attente » ou un job ne démarre pas

Un pipeline est **En attente** jusqu'à ce que son premier job démarre, puis **En cours**. Un pipeline qui reste **En attente** n'a donc encore démarré aucun job : regardez l'état des jobs. Un job **En attente** peut avoir plusieurs causes.

### Aucun runner ne convient (runners Docker)

- **Aucun runner en ligne.** Ouvrez **Runners** : l'état doit être **En ligne** (signal de moins de 2 minutes). **Hors ligne** ou **Jamais connecté** : le processus `ferrisgit-runner` ne tourne pas ou ne joint pas le serveur (voir plus bas).
- **Le job déclare `tags` et aucun runner ne les porte tous.** Comparez les `tags` du job aux étiquettes affichées sur chaque runner : toutes celles du job doivent être présentes, orthographe et casse exactes. Un runner enregistré sans étiquette ne prend aucun job qui en déclare. Rappel : les étiquettes d'un runner sont celles de son enregistrement, pas celles de `FERRISGIT_RUNNER_TAGS`.
- **Le moteur de l'instance n'est pas Docker / runners.** Dans ce cas, le menu **Runners** n'apparaît pas. Un administrateur règle le moteur dans **Admin > Réglages > Exécution**.
- **Le plafond de jobs simultanés est atteint.** Un administrateur peut limiter le nombre de jobs en cours sur toute l'instance (réglage **Jobs simultanés (runners Docker)** dans **Admin > Réglages > Exécution**) ; les runners reçoivent alors « aucun job » jusqu'à ce qu'un job se termine.
- **Tous les runners sont occupés.** Un runner exécute un seul job à la fois.

### Le job attend les jobs qui le précèdent

- Un job avec `needs` attend que tous les jobs listés soient **Réussis**.
- Un job sans `needs` placé après la première étape attend que **tous** les jobs de **toutes** les étapes précédentes soient **Réussis**. Un job d'une étape ultérieure qui reste **En attente** alors que ses voisins ont démarré attend donc probablement un job lent ou en attente d'un runner à une étape précédente.
- Si un job dont il dépend échoue, il passe à **Ignoré** : il ne restera pas en attente, et le pipeline se termine **Échoué**.
- Les cycles dans `needs` (deux jobs qui s'attendent mutuellement, ou un job qui se déclare lui-même) sont refusés à la création : le pipeline est alors un pipeline invalide, voir [Le fichier est invalide](#le-fichier-est-invalide).

### Un pipeline sans job

Un pipeline dont `jobs` est vide n'a rien à exécuter et reste **En attente**.

## Un job échoue

Ouvrez le job : la cause est presque toujours dans son journal.

- **`command not found` ou `not found`** : la commande n'existe pas dans l'image (par exemple `bash` dans une image Alpine : le script s'exécute avec `sh`). Utilisez une autre image ou installez l'outil dans le script.
- **Le script s'arrête à la première erreur** : les lignes sont enchaînées avec `&&`. Une commande qui retourne un code non nul arrête tout, y compris `grep` sans résultat ou `diff` avec différences. Écrivez `commande || true` pour tolérer son échec.
- **`syntax error near unexpected token '&&'`** : une ligne se termine par `&` ou `;`. Retirez-le.
- **Le dépôt est absent** (Kubernetes) : le Pod ne contient pas le code. Voir [Moteur Kubernetes](/docs/ci-cd/kubernetes#limites).
- **`git` ne fonctionne pas dans le job** (Docker) : le dossier `.git` est supprimé du clone. Il y a les fichiers, pas l'historique.
- **Une variable CI est vide** (Kubernetes) : le moteur ne transmet pas les variables CI du dépôt. Voir [Variables et secrets](/docs/ci-cd/variables-et-secrets).
- **Un secret apparaît comme `***`** : c'est le masquage des variables CI dans les journaux ; ce n'est pas une erreur.
- **L'image ne se télécharge pas** : le message vient de Docker (runner) ou du cluster (Pod) ; l'image est introuvable, privée, ou le registre est injoignable depuis la machine ou le cluster.
- **Le job s'arrête sur une erreur du runner**, par exemple `failed to create workdir: …` ou `failed to strip git metadata from the workdir: …` : problème de droits ou d'espace disque dans `FERRISGIT_WORKDIR_ROOT`.

## Le cache semble absent

- **Avec les runners Docker**, `cache` est ignoré : rien n'est conservé. Aucun message n'est affiché.
- **Sur Kubernetes**, le cache est monté dans `/ferrisgit-cache/<clé>`. Un outil qui écrit ailleurs ne l'utilise pas : vérifiez les variables (`CARGO_HOME`, `npm_config_cache`…) et les chemins.
- **Le job échoue sans journal** alors qu'il déclare `cache` : la `StorageClass` n'est pas réglée. Le serveur journalise `validation error: job declares cache: keys but k8s_cache_storage_class is not configured`. Un administrateur la renseigne dans **Admin > Réglages > Exécution**, voir [Caches](/docs/ci-cd/caches#régler-la-storageclass).
- **Le job reste « En cours » sans journal** : la PVC de cache ne se lie pas. Lancez `kubectl get pvc -n <namespace>` : une PVC `Pending` indique que la `StorageClass` ne fournit pas de `ReadWriteMany`. Voir plus bas.
- **Le cache est vide après un renommage de clé** : une nouvelle clé crée un nouveau volume. Les caches sont par dépôt : un autre dépôt ne partage rien.
- **La clé est refusée** : seules les minuscules, chiffres et tirets sont acceptés.

## Docker est inaccessible depuis le runner

Les messages arrivent dans le journal du job (c'est lui qui s'affiche dans l'interface) :

- `failed to start docker: No such file or directory (os error 2)` : la commande `docker` n'est pas installée ou pas dans le `PATH` du runner. Même forme pour `failed to start git: …`.
- `permission denied while trying to connect to the Docker daemon socket …` (message de Docker) : l'utilisateur du runner n'a pas accès au démon, par exemple parce qu'il n'est pas membre du groupe `docker`.
- `Cannot connect to the Docker daemon …` : le démon n'est pas démarré.

Le job passe alors à **Échoué**. Corrigez la machine du runner, puis poussez à nouveau.

## Le jeton du runner est invalide

Dans les journaux du runner, une ligne `ERROR failed to claim job` dont l'erreur contient `401 Unauthorized` signifie que le serveur refuse le jeton (le corps de la réponse est `{"error":"invalid runner token"}`). Causes habituelles :

- le jeton a été mal copié (il commence par `fgr_` et fait 68 caractères) ;
- le runner a été révoqué (supprimé) ;
- vous avez donné un autre type de jeton : un jeton Git ou le jeton d'enregistrement ne conviennent pas ;
- `FERRISGIT_SERVER_URL` pointe vers une autre instance.

Le jeton d'un runner n'est affiché qu'une fois : si vous l'avez perdu, enregistrez un nouveau runner et révoquez l'ancien. Voir [Runners Docker](/docs/ci-cd/runners-docker#enregistrer-le-runner).

Autres messages du runner :

- `FERRISGIT_SERVER_URL must be set` ou `FERRISGIT_RUNNER_TOKEN must be set` : variable manquante, le processus s'arrête.
- `failed to claim job` avec `error sending request for url (…)` : le serveur est injoignable (adresse, réseau, certificat).
- `failed to report job result` : le job s'est exécuté mais son résultat n'a pas pu être envoyé ; il reste **En cours**.
- Un clonage qui échoue produit, dans le journal du job, l'erreur de `git` (le jeton y est remplacé par `***`) : voir si le jeton est valide et si l'adresse du serveur est bonne.

## Un job reste « En cours » indéfiniment

FerrisGit n'applique aucun délai. Les causes :

- **Runner arrêté ou redémarré en plein job** : le job n'est pas repris. Annulez le pipeline, ou révoquez le runner (`DELETE /api/admin/runners/{id}`) : ses jobs en cours repassent alors à **En attente** et un autre runner les prend.
- **Script qui ne rend jamais la main** : annulez le pipeline. Avec les runners Docker, le conteneur n'est pas arrêté pour autant : arrêtez-le sur la machine du runner (`docker ps`, `docker stop`).
- **Variables CI illisibles** : si `SETTINGS_ENCRYPTION_KEY` a été changée, la lecture des variables du dépôt échoue au moment de confier le job au runner, qui reçoit une erreur du serveur alors que le job est déjà marqué **En cours**. Remettez l'ancienne clé, ou recréez les variables.

## Kubernetes

### Le moteur n'est pas disponible

Les jobs échouent sans journal et le serveur écrit `Kubernetes execution engine is not available on this server (no cluster was reachable at startup)` : aucune configuration de cluster n'a été trouvée au démarrage. Voir [Moteur Kubernetes](/docs/ci-cd/kubernetes#accès-au-cluster) ; redémarrez le serveur après correction.

### Pod Kubernetes bloqué

Un Pod qui n'atteint jamais l'état terminal (`Succeeded` ou `Failed`) laisse son job **En cours** sans limite de temps : FerrisGit n'a pas de délai pour cela. Cherchez le Pod :

```bash
kubectl get pods -n <namespace>
kubectl describe pod ferrisgit-job-<identifiant du job> -n <namespace>
```

Les causes courantes sont visibles dans `describe` :

- `ImagePullBackOff` ou `ErrImagePull` : image introuvable ou privée (le Pod n'a pas de `imagePullSecret` sauf celui du compte de service par défaut du namespace) ;
- `Pending` avec `Unschedulable` : pas de nœud avec assez de ressources ;
- `Pending` à cause d'un volume : la PVC de cache ne se lie pas (la `StorageClass` ne fournit pas `ReadWriteMany`, ou n'existe pas).

Annulez le pipeline : le Pod est alors supprimé. Corrigez la cause, puis poussez à nouveau.

### Quota de Pods ou refus du cluster

Quand le cluster refuse de créer le Pod (quota de Pods ou de ressources atteint, namespace inexistant, droits manquants), le job passe immédiatement à **Échoué** sans aucun journal. L'interface n'affiche pas la cause ; elle est dans les journaux du serveur :

```text
ERROR failed to submit job to execution engine; marking it failed
ERROR failed to submit newly-runnable job to execution engine; marking it failed
```

(le premier pour un job sans `needs`, créé avec le pipeline ; le second pour un job lancé plus tard), suivie du message de l'API Kubernetes. Vérifiez :

```bash
kubectl get resourcequota -n <namespace>
kubectl describe resourcequota -n <namespace>
```

Si un `ResourceQuota` exige des demandes et des limites de CPU ou de mémoire, ajoutez un `LimitRange` au namespace qui les fournit par défaut : FerrisGit n'en fixe aucune. Si le message parle de droits (`forbidden`), comparez avec le `Role` de [Moteur Kubernetes](/docs/ci-cd/kubernetes#droits-rbac).

### Après un changement de namespace, rien ne se termine

Les jobs restent **En cours** et sans journal : le namespace a été modifié dans les réglages mais le serveur n'a pas été redémarré, donc l'observateur de Pods regarde encore l'ancien namespace. Redémarrez le serveur.

### Journal incomplet ou Pod laissé

Si FerrisGit ne peut pas lire les journaux d'un Pod terminé, il écrit `failed to fetch logs from a terminal Pod; the job's logs may be incomplete` dans ses journaux, et le job a un journal vide ou partiel. S'il ne peut pas supprimer un Pod terminé, il écrit `failed to delete a terminal Pod; it may be left behind in the cluster` : supprimez-le à la main.
