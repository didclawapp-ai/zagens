# minilang design notes

## Goals

Small, readable, dependency-free. The interpreter exists to give the
kernel-v2 corpus a realistic multi-module codebase to read, search,
and edit — not to be a usable language.

## Module dependency graph

```text
lib.rs ──> lexer.rs ──> util.rs (Span)
   │  ──> parser.rs ──> lexer.rs (Token), util.rs (Span)
   └─ ──> eval.rs  ──> parser.rs (Ast)
```

`util.rs` sits at the bottom of the graph; nothing in `util.rs` may
import from the other modules.

## Evaluation strategy

The evaluator is a straightforward tree walk over `Ast`. Environments
are flat `HashMap`s today; the design doc for closures (unwritten)
calls for a parent-pointer chain.

## Error handling

All stages return `Result<_, String>` with human-readable messages.
Spans are produced by the lexer and threaded through the parser, but
the evaluator drops them — a known gap that makes runtime errors hard
to locate in the source.

## Open questions

1. Should `%` follow Rust semantics (sign of dividend) or Euclidean?
2. Do we want a bytecode VM eventually, or is a tree walk enough?
3. String values: interned (`util::intern`) or plain `String`?
