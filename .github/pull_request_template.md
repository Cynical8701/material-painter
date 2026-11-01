## What this changes

<!-- One or two sentences. Link the issue: "Fixes #123". -->

## How I tested it

<!-- What you ran and what you tried in the app. Screenshots or a clip for anything visible. -->

## AI assistance

<!-- Which tools, if any (Claude, Copilot, Cursor, ...), and roughly how much of the code they wrote. No judgment; it helps review. -->

## Checklist

- [ ] I've read the whole diff and understand every change in it.
- [ ] `cargo build` has no warnings and `cargo test` passes.
- [ ] GPU tests pass (`cargo test --bin material_painter -- --ignored --test-threads 1`), if I touched rendering, baking, painting, nodes or export.
- [ ] I tried the change in the running app.
- [ ] The PR contains only this change (no unrelated reformatting or refactors).
