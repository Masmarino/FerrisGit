# Premiers pas

Une pipeline FerrisGit est décrite par un fichier `.ferrisgit-ci.yml` à la racine du dépôt. Quand vous poussez, FerrisGit lit ce fichier, crée une pipeline et confie ses jobs à un moteur d'exécution : des runners Docker ou des Pods Kubernetes. Cette page vous mène du fichier minimal jusqu'aux journaux d'un job.

## Ce qu'il faut avoir avant

- Un dépôt sur lequel vous avez au moins le rôle Contributeur (pour pousser).
- La CI activée sur le dépôt. Elle l'est par défaut : l'option **CI activée pour ce dépôt** se trouve dans **Réglages > Pipeline** du dépôt.
- Un moteur d'exécution prêt, choisi par un administrateur dans **Admin > Réglages > Exécution** :
  - **Docker / runners** : au moins un runner enregistré et en ligne, voir [Runners Docker](/docs/ci-cd/runners-docker) ;
  - **Kubernetes** : un cluster configuré, voir [Moteur Kubernetes](/docs/ci-cd/kubernetes).

> **Note** : par défaut, un fichier de pipeline n'est lu que s'il est à la racine du dépôt sous le nom `.ferrisgit-ci.yml`. Un Mainteneur peut changer ce chemin dans **Réglages > Pipeline > Chemin du fichier pipeline** (chemin relatif à la racine du dépôt).

## Écrire une pipeline minimale

Créez `.ferrisgit-ci.yml` à la racine du dépôt :

```yaml ferrisgit-ci
stages: [test]

jobs:
  hello:
    stage: test
    image: alpine:3.20
    script:
      - echo "Bonjour depuis FerrisGit"
      - uname -a
```

Ce fichier déclare une étape (`test`) et un job (`hello`) qui s'exécute dans l'image `alpine:3.20`. Les lignes de `script` sont enchaînées avec `&&` : la première qui échoue fait échouer le job. Toutes les clés sont décrites dans [la référence](/docs/ci-cd/reference-yaml).

## Le pousser

```bash
git add .ferrisgit-ci.yml
git commit -m "Ajoute la pipeline"
git push
```

Le push s'authentifie comme d'habitude avec un jeton Git, voir [Cloner et pousser](/docs/utilisation/cloner-et-pousser). FerrisGit crée la pipeline avant de répondre au `git push`.

## Quand une pipeline se déclenche

FerrisGit crée une pipeline dans deux cas :

- **Après chaque push réussi** vers le dépôt, quelle que soit la branche ou le tag poussé. FerrisGit ne regarde pas ce qui a été poussé : il prend le commit sur lequel pointe `HEAD` dans le dépôt (la branche par défaut du dépôt) et y lit le fichier de pipeline.
- **Après la fusion d'une demande de fusion**, sur le nouveau commit de la branche cible. Voir [Demandes de fusion](/docs/utilisation/merge-requests).

Aucune pipeline n'est créée, sans message, quand :

- la CI est désactivée sur le dépôt ;
- le fichier de pipeline n'existe pas dans le commit lu.

Si le fichier existe mais qu'il est invalide, le push réussit et FerrisGit crée une pipeline **Échouée**, sans job, qui porte le message d'erreur : vous le lisez dans l'interface (voir [Dépannage](/docs/ci-cd/depannage#le-fichier-est-invalide)).

> **Attention** : puisque c'est le commit de `HEAD` qui est lu, pousser une branche de travail ne teste pas cette branche : la pipeline créée porte sur la pointe de la branche par défaut. Pour tester du code, fusionnez-le (la pipeline de la fusion tourne sur le résultat) ou poussez-le sur la branche par défaut. Un push vers un wiki ne déclenche rien.

## Voir la pipeline dans l'interface

Ouvrez le dépôt, puis l'entrée **Pipelines** du menu.

- La liste affiche chaque pipeline avec son numéro (`#` suivi des 8 premiers caractères de son identifiant), le message du commit, le SHA court, l'auteur du déclenchement et la durée. Les onglets **Toutes**, **En cours**, **Réussies** et **Échouées** filtrent par statut ; le champ **Rechercher une pipeline** cherche par SHA ou message ; le bouton **Actualiser** recharge la liste.
- Un clic sur une pipeline ouvre **Pipeline #xxxxxxxx**. Le menu de gauche liste le **Résumé**, puis chaque étape avec ses jobs et leur durée.
- Le statut d'une pipeline est l'un de : **En attente**, **En cours**, **Réussie**, **Échouée**, **Annulée**. Celui d'un job peut aussi être **Ignoré** : le job n'a pas démarré parce qu'un job dont il dépend a échoué.
- Une pipeline dont le fichier est invalide s'affiche **Échouée**, avec le libellé « Fichier de pipeline invalide » dans la liste. Son détail montre le message d'erreur à la place des jobs.

La page d'une pipeline se rafraîchit toute seule toutes les 3 secondes tant qu'elle n'est pas terminée.

Une pipeline est **En attente** à sa création, passe à **En cours** dès qu'un de ses jobs démarre, puis se termine quand tous ses jobs sont terminés. Elle est **Échouée** si un job a échoué ou n'a pas pu démarrer, sinon **Annulée** si un job a été annulé, sinon **Réussie** (voir [Statuts](/docs/ci-cd/reference-yaml#statuts)).

## Étapes et ordre des jobs

Les étapes (`stages`) s'exécutent l'une après l'autre : les jobs d'une étape ne démarrent que lorsque tous les jobs des étapes précédentes ont réussi. Les jobs d'une même étape tournent en parallèle. Un job qui déclare `needs` n'attend que les jobs qu'il liste. Si un job échoue, les jobs qui dépendent de lui (étapes suivantes comprises) ne démarrent pas : ils s'affichent **Ignoré** et la pipeline se termine **Échoué**. Voir [Ordre de lancement](/docs/ci-cd/reference-yaml#ordre-de-lancement).

À l'échec, la personne qui a poussé reçoit une notification (« La pipeline sur *sha* a échoué ») et les webhooks abonnés à l'événement `pipeline_failed` sont appelés. Voir [Webhooks](/docs/utilisation/webhooks).

## Lire les journaux d'un job

Cliquez sur un job dans le menu de gauche. La page affiche son nom, son statut, sa durée, les jobs dont il dépend (**Dépend de**) et ses journaux. Tant que rien n'est arrivé, la zone affiche « (pas encore de logs) ».

- Avec les **runners Docker**, les lignes arrivent au fil de l'exécution. Le dépôt est d'abord cloné par le runner : les erreurs de clonage apparaissent dans ce même journal.
- Avec **Kubernetes**, le journal complet n'est récupéré qu'à la fin du job, quand son Pod s'arrête.

Un job qui ne démarre pas reste **En attente** : voir [Dépannage](/docs/ci-cd/depannage).

## Annuler, relancer

- **Annuler** : sur une pipeline en attente, le bouton **Annuler la pipeline** apparaît en haut de la page. Il est réservé aux rôles Contributeur et supérieurs (et au propriétaire). Les jobs en attente ou en cours passent à **Annulé**. Une pipeline déjà terminée n'est pas modifiée.
- **Relancer** : il n'existe ni bouton ni route pour relancer une pipeline existante. Poussez un nouveau commit (par exemple `git commit --allow-empty -m "Relance la CI"` sur la branche par défaut, puis `git push`).

> **Attention** : avec les runners Docker, annuler une pipeline marque les jobs comme annulés mais n'arrête pas un conteneur déjà lancé : il va au bout, et son résultat est ignoré. Avec Kubernetes, le Pod du job est supprimé.

## Pour aller plus loin

- [Référence de .ferrisgit-ci.yml](/docs/ci-cd/reference-yaml) : toutes les clés et règles de validation.
- [Variables et secrets](/docs/ci-cd/variables-et-secrets).
- [Exemples de pipelines](/docs/ci-cd/exemples) : Rust, Node et Angular, image Docker, déploiement.
- [Pipelines et runners (API)](/docs/api/ci-cd).
