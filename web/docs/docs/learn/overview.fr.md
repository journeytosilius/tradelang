# Apprendre PalmScript

La documentation publique de PalmScript s'organise autour de :

- le langage pour ecrire des strategies
- des exemples qui montrent comment les scripts sont ecrits et utilises

## Ce Que Vous Faites Avec PalmScript

Flux typique :

1. ecrire un script `.ps`
2. declarer un `interval` de base
3. declarer une ou plusieurs liaisons `source`
4. le valider dans l'IDE navigateur
5. l'executer sur des donnees historiques dans l'application

## Optimisations Longues

Pour les longues recherches de tuning en CLI :

- utilisez `palmscript run optimize ...` quand vous voulez le resultat au premier plan
- utilisez `palmscript run optimize ...` pour optimiser directement depuis la CLI
- enregistrez les candidats utiles avec `--preset-out best.json` afin de les rejouer avec `run backtest` ou `run walk-forward`
- laissez le holdout final intact actif par defaut, sauf si vous voulez desactiver cette protection volontairement

## Que Lire Ensuite

- Premier flux executable : [Demarrage Rapide](quickstart.md)
- Premiere presentation complete d'une strategie : [Premiere Strategie](first-strategy.md)
- Vue d'ensemble du langage : [Vue d'ensemble du langage](language-overview.md)
- Regles et semantique exactes : [Vue d'ensemble de la Reference](../reference/overview.md)

## Roles De La Documentation

- `Apprendre` explique comment utiliser PalmScript efficacement.
- `Reference` definit ce que signifie PalmScript.

## Latest Diagnostics Additions

PalmScript now exposes richer machine-readable backtest diagnostics in every public locale build:

- `run backtest`, `run walk-forward`, and `run optimize` accept `--diagnostics summary|full-trace`
- summary mode keeps cohort, drawdown-path, baseline-comparison, source-alignment, holdout-drift, robustness, overfitting-risk, validation-constraint, and hint data, and top-level backtests also add bounded date-perturbation reruns
- full-trace mode adds one typed per-bar decision trace per execution bar
- optimize output now includes top-candidate holdout checks plus validation-constraint, holdout-pass-rate, parameter stability, baseline-comparison, and overfitting-risk summaries

## Execution Paper Locale

PalmScript inclut maintenant aussi un daemon local d'execution paper :

- `palmscript run paper ...` cree une session paper persistante
- `palmscript execution serve` traite ces sessions avec des donnees d'exchange reelles sur des bougies fermees
- la session reutilise la meme VM compilee, la meme simulation d'ordres et les memes regles de portefeuille que le backtest
- les snapshots paper montrent maintenant aussi le bid/ask top-of-book, le prix median derive et les prix last/mark lorsqu'ils existent
- la v1 utilise uniquement de l'argent fictif et n'envoie jamais d'ordres reels
