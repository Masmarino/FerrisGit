# Éditeur visuel

L'éditeur visuel permet de modifier la pipeline d'un dépôt de deux façons qui restent synchronisées : en déplaçant des cartes entre des colonnes, sans écrire de YAML, ou en écrivant le YAML lui-même. Le fichier `.ferrisgit-ci.yml` reste la seule source de vérité. L'éditeur le lit, le réécrit, et ce qu'il affiche est ce que le serveur exécuterait, parce que c'est l'analyseur du serveur qui le lit et l'écrit.

Une modification n'est jamais écrite directement sur la branche par défaut : elle part sur une nouvelle branche, avec une merge request, comme n'importe quelle autre modification.

## Ouvrir l'éditeur

Depuis la liste des pipelines d'un dépôt, le bouton **Éditer la pipeline** (que propose aussi la recherche rapide, ⌘K) ouvre l'éditeur sur le fichier de la branche par défaut, là où le dépôt le range (`.ferrisgit-ci.yml` sauf si le [réglage du dépôt](/docs/utilisation/depots-et-groupes) dit autre chose). Il est réservé aux contributeurs et aux rôles au-dessus : un lecteur voit un message qui le lui explique. Sans fichier dans le dépôt, l'éditeur démarre d'une pipeline vide avec les étapes `build` et `test`.

## Modifier avec les cartes

Le sélecteur **Cartes / YAML**, en haut de l'éditeur, choisit la vue. Chaque notion de la vue Cartes a sa bulle d'aide : le « ? » à côté d'une étape, d'un champ ou d'une section s'ouvre d'un clic (ou d'un appui sur un écran tactile) et explique à quoi elle sert, sans quitter la page. Échap la referme.

- **Partir d'un modèle.** Tant que la pipeline est vide, l'éditeur propose des pipelines complètes : projet Rust, projet Node ou Angular, projet Go, application publiée en image Docker. Un clic pose les étapes et les jobs, avec leurs dépendances, que vous modifiez ensuite.
- **Ajouter un job avec une tuile.** Le bouton **Ajouter un job** d'une étape ouvre le catalogue des tuiles, rangées par usage : compiler, tester, vérifier le code, empaqueter, déployer et prévenir, ou un job vide. Une tuile arrive avec une image et des commandes prêtes et s'ouvre aussitôt pour être réglée. Chaque tuile a sa bulle d'aide qui dit ce qu'elle suppose, et un badge signale les secrets qu'elle lit et que le dépôt n'a pas encore.
- **Régler un job.** Un clic sur une carte ouvre son tiroir : nom, image (avec des images courantes à cliquer), commandes, variables, jobs à attendre (`needs`), étiquettes de runner et caches. Les commandes sont une liste : on en ajoute, retire et réordonne chacune, sans écrire de bloc de texte.
- **Lire le tableau.** Une carte montre le nom du job, son image et sa première commande, suivie du nombre de commandes qui viennent après. Survoler une carte, ou y placer le focus, met en évidence les jobs qu'elle attend et ceux qui l'attendent ; la mention « Après … » d'une carte dit dans quel sens.
- **Le menu ⋮ d'une carte** permet de modifier le job, de le **dupliquer**, de le déplacer vers une autre étape ou de le supprimer. La copie se place juste après l'original, sous le nom `<nom>-2`, attend les mêmes jobs que lui et s'ouvre aussitôt : c'est le moyen le plus court de tester sur deux versions, ou de déployer vers deux cibles.
- **Déplacer un job** se fait en le glissant vers une autre étape ou à un autre rang. Sans souris, le menu ⋮ de la carte propose les étapes de destination, et le déplacement est annoncé aux lecteurs d'écran. Sur un écran tactile, un appui prolongé saisit la carte, ce qui laisse le balayage faire défiler le tableau.
- **Les étapes** se renomment sur place, se déplacent avec le menu ⋮ de leur colonne et ne se suppriment que vides. Renommer un job met à jour les jobs qui l'attendent, renommer une étape met à jour les jobs qu'elle contient.

Le fichier produit s'affiche sous le tableau, avec un bouton pour le copier. Il est régénéré après chaque modification.

## Annuler, rétablir, quitter

Chaque modification faite avec les cartes peut être annulée : les flèches de la barre d'outils, ou **⌘Z** (Ctrl+Z) pour annuler et **⇧⌘Z** (Ctrl+Y) pour rétablir. L'infobulle des flèches dit ce qu'elles vont défaire, par exemple « Annuler : suppression du job lint ». Ce que l'on tape dans un même champ forme une seule étape, et tant que le curseur est dans un champ, ces raccourcis restent ceux du champ. C'est pourquoi supprimer un job ne demande pas de confirmation. Dans la vue YAML, c'est la zone de texte qui annule ce que vous tapez.

Quitter l'éditeur avec des modifications qui n'ont pas été proposées, pour une autre page de FerrisGit, demande une confirmation. Fermer l'onglet ou recharger la page aussi, par la question du navigateur. Une fois la merge request ouverte, plus rien n'est demandé.

## Construire une image et déployer

Les tuiles de déploiement posent des questions avant de créer le job : on y répond avec quelques mots, et l'éditeur écrit les commandes, que la fenêtre montre au fur et à mesure. Rien n'est créé avant le bouton **Ajouter le job** ; chaque réponse qui finit dans une commande est contrôlée (un nom de machine ne peut pas contenir d'espace ni de point-virgule), de sorte que rien ne puisse en sortir.

- **Construire et publier une image Docker.** Registre, nom de l'image, version, Dockerfile, dossier de construction, connexion au registre et publication. Le runner ne donne pas accès au Docker de sa machine : la construction passe par un **démon Docker distant** que vous exploitez, désigné par le secret `DOCKER_HOST`. La connexion au registre utilise `REGISTRY_USER` et `REGISTRY_PASSWORD`. FerrisGit ne fournit ni numéro de commit ni nom de branche : la version est un nom fixe que vous choisissez.
- **Déployer sur une VM (SSH).** Serveur, utilisateur, port, dossier, commandes à lancer, et vérification de l'identité du serveur (recommandée). Secrets : `SSH_PRIVATE_KEY`, et `SSH_KNOWN_HOSTS` quand l'identité est vérifiée. La préparation des clés est détaillée dans les [exemples](/docs/ci-cd/exemples#déploiement-sur-une-machine-par-ssh).
- **Copier des fichiers sur une VM (SSH).** Envoie un dossier du dépôt avec `rsync`, avec en option la suppression de ce qui n'y est plus et des commandes à lancer ensuite.
- **Déployer des manifestes sur Kubernetes** (`kubectl apply`), **mettre à jour un Deployment** (changer l'image, ou redémarrer, puis attendre qu'il soit prêt) et **déployer avec Helm**. Secret : `KUBE_CONFIG`, un kubeconfig.

Les secrets dont la tuile a besoin sont listés dans la fenêtre, avec ceux que le dépôt a déjà et ceux qui restent à créer (quand vous avez le rôle Mainteneur) : le panneau **Variables et secrets** permet ensuite de les saisir. Des modèles de pipeline complets enchaînent les tests, l'image et le déploiement : « Image Docker déployée sur une VM » et « Image Docker déployée sur Kubernetes ».

Ces tuiles lisent des secrets, qui ne sont transmis qu'aux runners Docker : si votre instance exécute les jobs avec Kubernetes, elles sont grisées avec l'explication. Cela ne concerne pas la cible du déploiement : des runners Docker peuvent très bien déployer sur un cluster Kubernetes.

## Variables et secrets

Un job reçoit des **variables** de deux sources, que l'éditeur sépare pour qu'on ne les confonde pas (voir [Variables et secrets](/docs/ci-cd/variables-et-secrets)).

- Les **variables du job** s'écrivent dans le tiroir du job. Elles finissent dans `.ferrisgit-ci.yml` : tous ceux qui lisent le dépôt les lisent. Un nom invalide (il doit être fait de lettres, de chiffres et de `_`, sans commencer par un chiffre) est signalé.
- Les **secrets** sont les variables CI du dépôt : chiffrées, masquées dans les journaux, jamais réaffichées. Le bouton **Variables et secrets** de la barre d'outils ouvre leur panneau, qui reprend le formulaire des réglages du dépôt.

L'éditeur aide à les utiliser :

- **Insérer une variable.** Sous les commandes, ce menu liste les variables du job puis les secrets du dépôt, et écrit `$NOM` là où se trouvait le curseur.
- **Un secret qui se cache dans une variable.** Une variable dont le nom évoque un secret (`TOKEN`, `PASSWORD`, `API_KEY`…) et qui a une valeur déclenche un avertissement, avec un bouton **En faire un secret** : la valeur part chiffrée dans le dépôt et quitte le fichier.
- **Les secrets qui manquent.** Les noms que les commandes lisent et que rien ne fournit sont listés dans le tiroir du job, avec un bouton pour créer le secret quand il s'agit d'un secret. Le panneau des secrets donne la liste de ceux que vos jobs lisent sans qu'ils existent (« À créer ») et, pour chaque secret existant, les jobs qui le lisent, ou qu'aucun job ne le lit.
- **Le moteur d'exécution.** Les secrets ne sont transmis qu'aux runners Docker : si votre instance exécute les jobs avec Kubernetes, le panneau le dit. De même, les caches n'ont d'effet qu'avec Kubernetes, et le tiroir le signale quand l'instance utilise des runners Docker.

Il faut le rôle **Mainteneur** pour voir et créer les secrets. Un contributeur peut utiliser `$NOM` dans ses commandes, mais l'éditeur ne peut pas lui lister les secrets : il lui indique de s'adresser à un mainteneur, et les contrôles qui dépendent de cette liste (secrets manquants, noms inconnus) ne sont pas affichés.

## Modifier le YAML

La vue **YAML** montre le fichier dans une zone de texte. Tant que les cartes n'ont pas été touchées, c'est le texte du dépôt, commentaires compris ; après des changements faits avec les cartes, c'est le YAML que le serveur en écrit.

Ce que vous tapez est vérifié par le serveur après une courte pause, avec les mêmes règles que pour une pipeline poussée. **Ce texte est enregistré tel quel**, commentaires et mise en forme compris : rien n'est réécrit tant que vous restez dans cette vue.

Si le YAML ne peut pas être lu, la vue **Cartes** est désactivée tant qu'il n'est pas corrigé, et l'erreur est affichée avec sa ligne. Repasser aux cartes une fois le YAML valide remplace les cartes par ce que vous avez écrit. Cela réécrit le fichier : l'éditeur dit alors ce qui a disparu (voir plus bas).

## Ce que dit le serveur

Les erreurs de [validation](/docs/ci-cd/reference-yaml#erreurs-de-validation) s'affichent au-dessus du fichier, **toutes à la fois**, dans une phrase qui désigne le job fautif ; dans la vue Cartes, un lien mène à la carte concernée, qui est entourée en rouge. Tant que cette liste n'est pas vide, le serveur refuserait le fichier et la modification ne peut pas être proposée. Les avertissements (job sans commande, image vide, étape déclarée deux fois) sont signalés à part : ils n'empêchent rien.

## Proposer la modification

**Proposer la modification** n'est possible que lorsque le fichier diffère de celui du dépôt et que le serveur ne lui reproche rien. Une fenêtre demande un titre et, si l'on veut, une description. Un clic sur **Proposer la modification** :

1. écrit le fichier sur une nouvelle branche `pipeline-editor/<identifiant>`, dans un commit à votre nom, bâti sur la pointe actuelle de la branche par défaut ;
2. ouvre une merge request de cette branche vers la branche par défaut ;
3. vous y emmène.

La modification est ensuite relue, discutée et fusionnée comme n'importe quelle merge request : voir [Merge requests](/docs/utilisation/merge-requests). Les pipelines démarrent sur un push ou sur une fusion : la branche créée par l'éditeur n'en lance pas à sa création, et la fusion en lance une sur le nouveau commit, avec le nouveau fichier.

Si le fichier a changé sur la branche par défaut depuis l'ouverture de l'éditeur, l'enregistrement est refusé et un bouton permet de rouvrir l'éditeur : il vaut mieux repartir de la dernière version que d'écraser le travail d'un autre. Un changement sur d'autres fichiers du dépôt n'y fait rien.

Dans un dépôt qui n'a encore aucun commit, il n'y a pas de branche vers laquelle ouvrir une merge request : l'éditeur reste utilisable pour composer le fichier, que l'on copie ensuite dans le dépôt.

## Ce qu'une réécriture ne conserve pas

Passer des cartes à l'enregistrement, ou du YAML aux cartes, réécrit le fichier. Deux choses ne survivent pas :

- les **commentaires** ;
- les **clés que FerrisGit ne connaît pas**, que l'analyseur ignore déjà à l'exécution.

À l'ouverture, un encadré liste ce que le fichier du dépôt contient de ce genre ; au passage du YAML aux cartes, un autre dit ce qui vient d'être perdu. Le bouton **Revenir au fichier du dépôt** abandonne les modifications faites dans l'éditeur, après une confirmation, car cela ne s'annule pas. Pour garder les commentaires, modifiez le fichier dans la vue YAML sans repasser par les cartes.

Si le fichier du dépôt est illisible (YAML mal formé), l'éditeur le dit et part d'une pipeline vide plutôt que de deviner.

Les routes qui servent l'éditeur sont décrites dans la [référence de l'API](/docs/api/ci-cd#lire-et-écrire-un-fichier-de-pipeline).
