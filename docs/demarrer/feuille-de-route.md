# Feuille de route

Cette page décrit la direction du projet, version par version. Rien de ce qui suit n'existe encore : le contenu
changera à mesure que chaque fonction sera conçue. Pour ce qui est disponible aujourd'hui, voir la
[présentation](/docs/demarrer/presentation) et le journal des versions du projet.

## Vue d'ensemble

| Version | Thème |
|---|---|
| 0.2 | Le socle CI des vérifications (branches protégées, pipelines sur les demandes de fusion, artefacts, jetons sur l'API), l'interface en cinq langues et le choix du thème |
| 0.3 | Le moteur d'analyse, la détection de secrets et l'audit des dépendances |
| 0.4 | Les portes qualité, le tableau de bord et le traitement des anomalies |
| 0.5 | La qualité du code : complexité, duplication, mauvaises pratiques, couverture de tests |
| 0.6 | L'analyse de sécurité et le lien complet avec ArtiFerris |

## 0.2 : un socle pour les vérifications, cinq langues et le choix du thème

Un scan de code n'a de sens que si la plateforme sait qu'un résultat appartient à un commit, qu'il s'affiche sur une
demande de fusion et qu'il peut en bloquer la fusion. Cette version construit ce socle, traduit l'interface et laisse choisir son thème :

- les branches protégées (pas de push direct, fusion par demande de fusion, approbations et vérifications obligatoires) ;
- les pipelines sur les demandes de fusion, avec leur statut affiché et exigé avant la fusion ;
- les variables CI prédéfinies (commit, branche, demande de fusion, pipeline) et des règles pour décider quand un job
  s'exécute ;
- les artefacts de pipeline et l'envoi de rapports ;
- la relance d'un pipeline depuis l'interface et les pipelines planifiés ;
- les jetons d'accès utilisables sur l'API REST, avec des portées, et une description OpenAPI générée depuis le code ;
- un premier lien avec ArtiFerris : publier depuis un pipeline un artefact vers une instance ArtiFerris ;
- l'internationalisation avec Transloco : l'interface en anglais, français, italien, espagnol et allemand. La langue suit
  le navigateur à la première visite, se change à tout moment et est enregistrée sur le compte ; les e-mails sont envoyés
  dans la langue du destinataire, et les pages publiques suivent la langue du lecteur. La documentation sera traduite
  ensuite ;
- le mode sombre comme réglage : système, clair ou sombre, enregistré sur le compte et appliqué sans clignotement.
  L'interface suit aujourd'hui le réglage du système, sans bouton. Le logo du shell connecté suit aussi le thème.

## 0.3 à 0.6 : le scan de code

FerrisGit aura son propre moteur d'analyse, au lieu d'encapsuler des outils tiers. Il s'exécute comme un job de
pipeline : l'analyse suit donc la capacité des runners et le serveur reste léger. Les rapports d'autres outils (format
SARIF) pourront être importés à côté des résultats du moteur.

Le dépôt entier est analysé à chaque push sur la branche par défaut et à chaque demande de fusion, mais les portes
qualité ne bloquent que sur ce que la demande de fusion introduit : un projet existant reste adoptable.

- **0.3 :** le moteur et le stockage des anomalies (une empreinte stable par anomalie, une référence par branche), la
  détection de secrets dans le diff et dans tout l'historique, l'audit des dépendances à partir des fichiers de
  verrouillage, et l'affichage des anomalies directement dans le diff.
- **0.4 :** des portes qualité configurables par dépôt (par exemple aucune nouvelle anomalie bloquante, une couverture
  minimale sur le nouveau code), un tableau de bord par dépôt (anomalies ouvertes, ancienneté, tendance, dette technique), le traitement des anomalies (faux positif, ne sera pas
  corrigé, assignation) et un fichier `.ferrisgit/scan.yml` pour régler le tout.
- **0.5 :** l'analyse du code lui-même, pour Rust et TypeScript d'abord : complexité, duplication, mauvaises pratiques,
  et suivi de la couverture de tests.
- **0.6 :** l'analyse de sécurité (règles de motifs, puis flux de données dans une fonction), l'analyse des images de
  conteneur et de l'infrastructure décrite en code, un résumé de sécurité et un tableau de bord de sécurité par dépôt,
  la traçabilité de chaque artefact publié dans ArtiFerris jusqu'au commit, à la demande de fusion et au pipeline qui
  l'ont produit, et le retour des résultats d'analyse d'ArtiFerris dans les vérifications. Une identité partagée entre
  les deux plateformes reste à explorer.

## À l'étude, sans version

Des idées dont le contenu reste à définir :

- des agents d'IA sur un dépôt (assistant de relecture des demandes de fusion, résumés, explication d'un pipeline en
  échec, tri des tickets). Pour une plateforme auto-hébergée, ils utiliseraient un modèle choisi par l'administrateur
  (un modèle local compris), activé dépôt par dépôt, avec des identifiants limités et chaque action enregistrée ;
- un graphe des commits dans la vue d'un dépôt : branches, fusions et tags dessinés ensemble, à côté de l'arborescence
  et de la liste des commits qui existent déjà ;
- des photos de profil, stockées sur un espace compatible S3. Les initiales restent en solution de repli, et le même
  stockage pourrait accueillir plus tard les fichiers de releases et les artefacts.

## Et la plateforme ?

Sans ordre fixé, parmi ce qui est envisagé : l'accès Git par SSH et les clés de déploiement, les forks, le
renommage et le transfert d'un dépôt avec redirections, les fusions en squash ou en rebase, la réouverture et les
brouillons de demandes de fusion, `CODEOWNERS`, les mentions `@nom` et les références de tickets, la recherche dans le
code, l'import depuis GitHub et GitLab, la réinitialisation de mot de passe en libre-service, les notifications par
e-mail, l'authentification unique (OIDC, LDAP), une page de journal d'audit, les métriques Prometheus, des outils de
sauvegarde et de restauration, et des webhooks plus complets (nouvelles tentatives, journal des envois).
