# AGENTS.md

entropy-rs is a Rust compiler for [Entropy](https://entropy-lang.org/). The design spec is in
`.docs/nc-superpowers/specs/`.

## Simplicity

Do the simplest thing that works. Add complexity when a real need appears, not an imagined one.

### Code

- Plain data, small functions, obvious control flow.
- No abstraction with a single user. Traits, generics and macros wait for a second real case.
- No dependency for what std already does.
- Delete dead code. Don't comment it out.
- Name things for what they are.
- Comments explain why, not what. Most code needs none.

### Prose

This covers docs, comments, commit messages, PRs and replies.

- Short sentences, common words.
- Say it once. Cut what the reader doesn't need.
- No filler, hedging or sales tone.
- Little formatting. Bold rarely. Lists only for real lists.
- An example beats an explanation.

## Project rules

- The written spec (entropy-lang.org, esolangs.org) is the reference. Every program valid there
  must work here.
- All decay logic lives in `entropy-runtime`.
- User input never causes a panic.
- Every behaviour change comes with a golden test.
- Before calling work done: `scripts/check.sh` passes (static checks and tests).
