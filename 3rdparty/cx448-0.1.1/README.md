# cx448

> Implementation of ed448, curve448 and x448.


**Note** This code is based on

- https://github.com/crate-crypto/x448/
- https://github.com/RustCrypto/elliptic-curves/pull/1121

ported to the current _stable_ releases of the rustcrypto ecoysystem. Once upstream updates have landed it will be retired again.


---


THIS CODE HAS NOT BEEN AUDITED OR REVIEWED. USE AT YOUR OWN RISK.

## Field Choice

The field size is a Solinas trinomial prime `2^448 - 2^224 -1`. This prime is called the Goldilocks prime.

## Curves

This repository implements three curves explicitly and another curve implicitly.

The three explicitly implemented curves are:

- Ed448-Goldilocks
- Curve448
- Twisted-Goldilocks
