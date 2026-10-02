# Réglages de l'instance

Les réglages de l'instance se trouvent dans **Admin**, puis **Réglages** (adresse `/admin/settings`). Ils sont réservés aux super-administrateurs et s'appliquent à tous les utilisateurs. La page est organisée en trois onglets : **Exécution**, **Sécurité** et **E-mail**. On peut ouvrir directement un onglet avec `/admin/settings?section=security` ou `?section=email`.

Les réglages des onglets **Exécution** et **Sécurité** s'enregistrent un par un, dès que vous les modifiez : un interrupteur ou le moteur d'exécution tout de suite, un champ texte quand vous le quittez, le curseur de taille de push une fraction de seconde après que vous le lâchez. À côté de chaque réglage, un indicateur affiche **Enregistrement…**, puis **Enregistré**, ou **Non enregistré** en cas d'échec (la valeur précédente est alors rétablie et une notification **Échec de l'enregistrement. Réessayez.** apparaît). L'onglet **E-mail** a un bouton **Enregistrer**.

Ces réglages sont stockés dans la base de données. Ils ne dépendent pas des variables d'environnement, décrites dans [Configuration](/docs/administration/configuration).

## Quand un réglage prend effet

| Réglage | Effet |
|---|---|
| Moteur d'exécution | Pour les pipelines créés ensuite. Un pipeline déjà créé garde le moteur avec lequel il a démarré. |
| Jeton d'enregistrement des runners | Tout de suite : la route d'auto-enregistrement lit le réglage à chaque appel. |
| Jobs simultanés (runners Docker) | Tout de suite, à la prochaine demande de job d'un runner Docker. |
| Namespace Kubernetes | Pour les nouveaux Pods, tout de suite ; mais le suivi des Pods ne lit la valeur qu'au démarrage du serveur : redémarrez le serveur après un changement. |
| StorageClass pour le cache | Au prochain job qui déclare un cache. |
| Durée de vie du JWT | Aux prochaines connexions ; les sessions ouvertes gardent leur durée. |
| Inscription libre | Tout de suite. |
| Pages publiques, référencement | Tout de suite. |
| Taille maximale d'un push | Au prochain accès Git. |
| Durée de conservation des journaux | Au prochain balayage, qui a lieu au démarrage du serveur puis toutes les 24 heures (pas au moment de l'enregistrement). |
| Serveur SMTP | À l'envoi du prochain e-mail. |

## Onglet Exécution

### Moteur d'exécution

Choisit où s'exécutent les jobs des pipelines. Deux valeurs : **Docker / runners** (par défaut) et **Kubernetes**.

- **Docker / runners** : les jobs sont confiés aux runners enregistrés, qui les lancent dans des conteneurs Docker. L'entrée **Runners** du menu n'apparaît que dans ce mode. Voir [Runners Docker](/docs/ci-cd/runners-docker).
- **Kubernetes** : chaque job est lancé dans un Pod du cluster, dans le namespace indiqué plus bas. FerrisGit doit pour cela trouver une configuration Kubernetes (jeton de `ServiceAccount` dans le cluster, ou `KUBECONFIG`). Sans elle, le serveur démarre quand même, mais choisir Kubernetes fait échouer les jobs avec une erreur explicite. Voir [Moteur Kubernetes](/docs/ci-cd/kubernetes).

Vous pouvez changer de moteur à tout moment. Les pipelines déjà créés continuent sur le moteur avec lequel ils ont été créés, y compris pour l'annulation.

### Runners Docker : jeton d'enregistrement

La carte **Runners Docker** regroupe deux réglages qui ne concernent que ces runners. Ils sont sans effet avec le moteur **Kubernetes**.

Le jeton d'enregistrement laisse des machines s'enregistrer elles-mêmes comme runners, sans passer par un administrateur : un runner qui le présente à `POST /api/runner/register` reçoit son propre jeton (voir [Runners Docker](/docs/ci-cd/runners-docker)). Tant qu'aucun jeton n'est défini, cette route est désactivée.

La ligne **Jeton d'enregistrement** affiche **Configuré** ou **Non configuré**. Le jeton lui-même n'est jamais réaffiché : FerrisGit n'en conserve que l'empreinte (SHA-256).

- **Définir un jeton** (ou **Remplacer le jeton**, quand il y en a déjà un) : saisissez le jeton dans le champ masqué. Il est enregistré quand vous quittez le champ, puis le champ se vide. Un champ laissé vide n'enregistre rien.
- **Générer un jeton** : le navigateur tire un jeton aléatoire de 256 bits (64 caractères hexadécimaux), l'enregistre et l'affiche **une seule fois**, avec un bouton de copie. Copiez-le tout de suite, puis cliquez sur **J'ai copié le jeton**. Quand un jeton existe déjà, le bouton s'appelle **Générer un nouveau jeton** et demande confirmation : l'ancien jeton ne permet plus d'enregistrer de nouveaux runners.
- **Supprimer le jeton** (visible quand un jeton est configuré) : demande confirmation, puis désactive l'auto-enregistrement.

Remplacer ou supprimer le jeton ne touche pas aux runners déjà enregistrés : chacun garde son propre jeton et continue de fonctionner. Pour en révoquer un, utilisez **Runners**.

> **Attention** : ce jeton donne le pouvoir d'enregistrer un runner, qui peut ensuite prendre des jobs et lire les dépôts et les variables CI. Traitez-le comme un secret et ne le laissez pas dans un script versionné.

### Runners Docker : jobs simultanés

Nombre entier de 1 ou plus : le plafond de jobs « en cours » en même temps. Vide (le défaut), il n'y a pas de plafond. Quand le plafond est atteint, les runners Docker qui demandent un job reçoivent « aucun job » jusqu'à ce qu'un job se termine ; les jobs en attente restent en attente. Le champ refuse un zéro, un nombre négatif ou décimal.

Le plafond ne s'applique qu'aux runners Docker : il n'est pas vérifié quand le moteur Kubernetes crée un Pod. Le compte, lui, porte sur tous les jobs « en cours » de l'instance, quel que soit le moteur avec lequel ils ont démarré.

### Namespace Kubernetes

Champ **Namespace Kubernetes** (exemple affiché : `ferrisgit-ci`). C'est le namespace dans lequel les Pods des jobs sont créés. Le réglage est sans effet tant que le moteur est **Docker / runners**.

Quand le champ est vide, FerrisGit utilise le namespace détecté au démarrage (celui de son propre Pod, ou celui du contexte du kubeconfig) ; si rien n'est détecté, `ferrisgit-jobs`. Si une valeur est détectée, l'interface l'affiche (« Détecté automatiquement depuis le cluster : … »).

L'enregistrement a lieu quand vous quittez le champ après l'avoir modifié. Vider le champ revient à la valeur détectée.

> **Attention** : le suivi des Pods lit le namespace une seule fois, au démarrage du serveur, alors que les nouveaux Pods utilisent la valeur à jour. Si vous changez le namespace sans redémarrer le serveur, les nouveaux jobs restent à l'état « en cours » sans logs.

### StorageClass pour le cache (RWX requis)

Champ **StorageClass pour le cache (RWX requis)** (exemple : `standard-rwx`). C'est la `StorageClass` qui sert à créer les volumes de cache, un par dépôt et par clé de cache (voir [Caches](/docs/ci-cd/caches)). Elle doit gérer l'accès `ReadWriteMany`.

Un job qui déclare des clés `cache:` échoue explicitement tant qu'aucune `StorageClass` n'est enregistrée ici. Les jobs sans cache n'en ont pas besoin.

Si FerrisGit détecte la classe par défaut du cluster, il l'affiche dans le champ, avec un avertissement : vérifiez qu'elle supporte `ReadWriteMany` avant de l'utiliser. **Cette valeur détectée n'est jamais appliquée tant que vous ne l'avez pas enregistrée.** Pour la confirmer, modifiez le champ (par exemple en retapant le nom) puis quittez-le.

### Journaux des jobs

Champ **Durée de conservation des journaux (jours)** : un nombre entier de 1 ou plus. Vide (le défaut), les journaux sont conservés sans limite.

Quand une durée est définie, un balayage efface le texte du journal des jobs **terminés** (réussis, en échec, annulés ou ignorés) dont la fin remonte à plus de cette durée. Les pipelines, les jobs, leurs statuts, leurs durées et leurs dates sont conservés. Dans la page du pipeline, le job concerné affiche alors « Journal supprimé le *date* (rétention des journaux de l'instance). » à la place de son journal. Les jobs en cours ou en attente ne sont jamais touchés, ni les jobs qui n'ont jamais écrit de journal.

Le balayage s'exécute au démarrage du serveur, puis toutes les 24 heures. Il n'a pas lieu au moment où vous enregistrez le champ : un journal trop ancien est supprimé au plus tard 24 heures plus tard. Il est borné (500 jobs à la fois) et s'interrompt proprement à l'arrêt du serveur. La suppression est **définitive** : raccourcir la durée efface les journaux plus anciens au prochain balayage, et allonger la durée ne restaure rien.

## Onglet Sécurité

### Sessions : Durée de vie du JWT (heures)

Durée pendant laquelle une connexion reste valide avant que l'utilisateur doive se reconnecter (mot de passe et second facteur). Nombre entier d'heures, de **1 à 720** (30 jours). Valeur par défaut : **12**. Le champ refuse tout ce qui n'est pas un entier de 1 ou plus (« Entrez un nombre entier d'heures, 1 ou plus ») ; le serveur refuse en plus une valeur supérieure à 720, et l'enregistrement échoue.

La valeur est lue au démarrage du serveur et mise à jour quand vous l'enregistrez. Elle s'applique aux **jetons émis ensuite** : les sessions déjà ouvertes gardent leur échéance d'origine. Une session peut aussi s'arrêter plus tôt : elle est révoquée quand l'utilisateur change son mot de passe (une nouvelle session est alors émise pour lui) ou quand un administrateur réinitialise son mot de passe ou sa double authentification, et à la suppression du compte.

### Inscription : Autoriser l'inscription libre

Interrupteur, **désactivé par défaut**. Désactivé, seuls les administrateurs créent des comptes, par invitation (voir [Utilisateurs et invitations](/docs/administration/utilisateurs)) ; la page `/register` répond « Les inscriptions sont fermées ».

Activé, toute personne qui peut joindre l'instance peut créer un compte sur `/register` en choisissant un nom d'utilisateur, une adresse e-mail et un mot de passe. Le compte est créé tout de suite, **sans vérification de l'adresse e-mail**, et n'est pas administrateur. Il passe obligatoirement par la configuration de la double authentification à sa première connexion. N'activez ce réglage que sur une instance que vous acceptez d'ouvrir. L'inscription est limitée à 10 tentatives par adresse IP et par période de 5 minutes.

### Pages publiques : Pages publiques

Interrupteur, **activé par défaut**. Activé, toute personne sans compte peut parcourir les dépôts publics en lecture seule : catalogue, README, fichiers, commits, branches, tags et releases (avec leurs fichiers joints). Désactivé, ces pages et l'API publique (`/api/public/…`) répondent comme si les dépôts n'existaient pas, et la page d'accueil `/` redirige vers la connexion. Voir [Pages publiques](/docs/utilisation/pages-publiques) pour ce que voient les visiteurs.

Le réglage vaut aussi pour Git. Activé, un dépôt public se clone sans identifiants, ainsi que son wiki (`.wiki.git`). Désactivé, le clonage anonyme est refusé exactement comme pour un dépôt privé : le serveur répond 401 et demande une authentification, comme pour un dépôt qui n'existe pas. Un utilisateur qui s'authentifie avec son nom et un jeton Git continue de lire les dépôts publics, comme dans l'interface. Si le serveur ne parvient pas à lire le réglage, il refuse les lectures anonymes.

### Pages publiques : Référencement par les moteurs de recherche

Interrupteur, **désactivé par défaut**, et grisé tant que **Pages publiques** est désactivé.

- Désactivé : toutes les réponses du serveur portent l'en-tête `X-Robots-Tag: noindex, nofollow` et `/robots.txt` interdit tout (`Disallow: /`).
- Activé (et pages publiques activées) : l'en-tête n'est plus ajouté et `/robots.txt` n'interdit plus que `/api/`, `/account` et `/admin/`.

### Pushs : Taille maximale d'un push (Mio)

Curseur de **1 à 600 Mio**, valeur par défaut **500 Mio**. C'est la taille maximale de la requête qu'un client Git envoie à FerrisGit, sur tous les dépôts. Au-delà, le serveur répond « 413 Payload Too Large » et le push est refusé. La valeur est vérifiée à chaque requête Git, sans redémarrage.

Quelle que soit la valeur, le serveur applique en plus un plafond fixe de 600 Mio.

> **Attention** : pour contrôler la taille, FerrisGit reçoit tout le corps de la requête en mémoire avant de l'examiner. Jusqu'à 8 requêtes de ce type peuvent être en cours en même temps. Prévoyez la mémoire du conteneur en conséquence, et la limite de taille de votre reverse proxy (voir [Installation](/docs/administration/installation)). Dans le chart Helm du dépôt, la limite mémoire par défaut est de 2 Gio (`resources.limits.memory`).

## Onglet E-mail

FerrisGit envoie des e-mails pour inviter un utilisateur, réinitialiser un mot de passe, et prévenir un utilisateur quand son mot de passe est changé, quand une méthode de double authentification est ajoutée ou quand son administrateur la réinitialise. Il n'envoie pas d'e-mails de notification d'activité. Tant qu'aucun serveur n'est configuré, **aucun e-mail n'est envoyé** ; l'interface indique « Aucun serveur configuré : FerrisGit n'envoie pas d'e-mails. ». Les liens d'invitation et de réinitialisation sont alors affichés à l'administrateur, qui les transmet lui-même (voir [Utilisateurs et invitations](/docs/administration/utilisateurs)).

### Serveur SMTP

Remplissez le formulaire puis cliquez sur **Enregistrer**. Le bouton n'est actif que s'il y a des modifications.

| Champ | Détail |
|---|---|
| **Hôte (serveur SMTP)** | Nom d'hôte ou adresse IP, 253 caractères au plus, sans espace. Obligatoire. |
| **Port** | Entier de 1 à 65535. Obligatoire. Valeur initiale : 587. |
| **Sécurité** | **Aucune** : connexion en clair, généralement port 25. **STARTTLS** : la connexion démarre en clair puis passe en chiffré, généralement port 587. **TLS** : connexion chiffrée dès le départ, généralement port 465. Valeur initiale : STARTTLS. |
| **Identifiant** | Laissez vide si le serveur n'exige pas d'authentification. |
| **Mot de passe** | Obligatoire si un identifiant est renseigné. Stocké chiffré, jamais réaffiché ; laissez le champ vide pour conserver le mot de passe enregistré. Sans identifiant, aucun mot de passe n'est conservé. |
| **Adresse d'expédition** | Adresse e-mail de l'expéditeur. Obligatoire, avec un domaine pointé (`ferrisgit@example.com`). |
| **Nom d'expéditeur** | 100 caractères au plus, sans caractère de contrôle. Vide, il vaut `FerrisGit`. |

Avec la sécurité **Aucune**, l'interface avertit que l'identifiant et le mot de passe transitent en clair ; réservez ce mode à un relais interne de confiance. Avec **STARTTLS** et **TLS**, le certificat du serveur est vérifié : FerrisGit n'a pas d'option pour accepter un certificat invalide. Chaque connexion SMTP a un délai maximal de 15 secondes.

Les réglages sont relus à chaque envoi : pas de redémarrage nécessaire.

### Tester l'envoi

La carte **Tester l'envoi** envoie un e-mail d'objet « E-mail de test FerrisGit » au **Destinataire du test**, avec les réglages **enregistrés** : enregistrez avant de tester. Le bouton **Envoyer un e-mail de test** reste inactif tant que le formulaire contient des modifications non enregistrées, qu'aucun serveur n'est configuré ou que l'adresse n'est pas valide.

Le résultat s'affiche sous le champ : « E-mail de test envoyé à … » ou « Échec de l'envoi : … » suivi du motif donné par le serveur (identifiants refusés, serveur injoignable…).

Les e-mails de sécurité (mot de passe changé, double authentification) partent en tâche de fond : un échec d'envoi est journalisé et n'empêche jamais l'action. Pour une invitation ou une réinitialisation de mot de passe, l'administrateur apprend tout de suite si l'envoi a échoué.
