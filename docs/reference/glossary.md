# Glossary

## Base Interval

The execution clock declared by `interval <...>`. Unqualified market series refer to this interval.

## Declared Interval

Any interval explicitly listed in `use <...>` so the strategy may reference it.

## No Lookahead

The guarantee that higher-interval values appear only after their candles fully close.

## Output Series

A named per-bar result emitted by `export` or `trigger`.

## Raw Interval

The source interval inferred from the CSV data file before roll-up.

## Roll-Up

The deterministic aggregation of a finer interval feed into a coarser interval feed.
