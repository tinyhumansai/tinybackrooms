# 4. Jev is an optional judge, not part of generation

- **Status:** Accepted
- **Date:** 2026-10-04

## Context

The brief pointed at "Jav TypeSafe AI's conditional model" and fast typed
output. The best match is TypeSafe AI's **Jev** (launched 2026-09-15; about 90%
confidence in the identification). It is a non-autoregressive "System One"
model: one query carries state plus typed questions (Choice, Score, Noul) and
returns typed answers with calibrated confidence in a claimed 70–500 ms. It
cannot generate text, and TypeSafe has published no technical paper or weights;
the speed and cost figures are the vendor's and unverified. The papers that
actually explain fast typed generation are the constrained-decoding ones in
[ADR 0003](0003-free-text-then-structure.md).

## Decision

Jev is not in the generation path, because creative generation needs text.
It is a candidate backend for the `judge` scorer, whose job (score novelty and
usefulness, label a Boden class) is a classification. That integration, behind
a `jev` Cargo feature using the `tinyjevclient` crate, is deferred until the
core ships and until the vendor claims can be checked against our own labelled
set.

## Consequences

- No dependency on an unpublished model in v1.
- The `judge` scorer is already isolated behind `Conversant::structured`, so a
  Jev-backed judge replaces the LLM judge without engine changes.
