# entropy-rs

A Rust compiler for [Entropy](https://entropy-lang.org/) ([esolangs](https://esolangs.org/wiki/Entropy)).
Status: in design.

## Principles

The written spec is the reference. Anything it leaves open gets an explicit rule.

entropy-rs is a strict superset: every spec-valid program is valid here with the described
behaviour. Extensions such as extra operators and `extern` only use syntax the spec never does.

Every read of a value takes one persistent decay step. This covers variables, literals,
temporaries and the right-hand side of an assignment.

Decay is deterministic given a seed. With ChaCha8 and a fixed evaluation and draw order, the
same seed always produces byte-identical output.

There is one code generator, Cranelift, which JIT-compiles for `run` and emits native executables
for `build`. A small reference evaluator in the test suite checks it on generated programs.

`extern` calls into the C ABI. Values decay while they're in Entropy and arrive in C exactly as
they were when passed.
