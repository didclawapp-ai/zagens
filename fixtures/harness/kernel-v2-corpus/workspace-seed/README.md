# minilang

A tiny tree-walking interpreter used as a **fixed corpus workspace** for
Zagens kernel-v2 benchmarks. The code is intentionally small, self-contained,
and never compiled by the harness — scenarios only read, search, and edit it.

## Layout

| Path | Purpose |
|------|---------|
| `src/lib.rs` | Crate root, public API surface |
| `src/lexer.rs` | Tokenizer for minilang source text |
| `src/parser.rs` | Recursive-descent parser producing an AST |
| `src/eval.rs` | Tree-walking evaluator |
| `src/util.rs` | Shared helpers (string interning, spans) |
| `docs/design.md` | Architecture notes |

## Language sketch

minilang supports integer arithmetic, `let` bindings, `if`/`else`,
functions, and a handful of built-ins:

```text
let add = fn(a, b) { a + b };
let result = add(2, 3);
print(result); // 5
```

## Pipeline

1. `lexer::tokenize` turns source text into a `Vec<Token>`.
2. `parser::parse_program` builds an `Ast` from the token stream.
3. `eval::eval_program` walks the AST with an `Env` chain.

Errors at any stage carry a `Span` (byte offsets) so the caller can render
a caret diagnostic via `util::render_span`.

## Known limitations

- No garbage collection: environments leak in long-running REPL sessions.
- Parser recovers poorly from missing closing braces.
- Numeric overflow wraps silently (see `eval.rs`).
