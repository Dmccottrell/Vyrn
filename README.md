# Vyrn

A gradually typed, garbage-collector-free language for machine learning, web
services and WebAssembly, and games and systems code.

Status: design drafts. There is no lexer, parser or compiler yet.

- [`docs/architecture.md`](docs/architecture.md): compiler tiers, memory model, GPU bridge and concurrency design
- [`docs/syntax-v0.2.md`](docs/syntax-v0.2.md): proposed surface syntax built on progressive disclosure, with four examples
- [`docs/grammar.ebnf`](docs/grammar.ebnf): the complete EBNF grammar (v0.1 draft, not yet updated to v0.2)
- [`examples/api.vyrn`](examples/api.vyrn): HTTP routes returning JSON
- [`examples/inference.vyrn`](examples/inference.vyrn): tensor kernels across several GPUs
- [`examples/game.vyrn`](examples/game.vyrn): a statically sized entity pool updated in a tight loop
