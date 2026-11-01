# Contributing to Material Painter

Thanks for helping! Bug reports, testing on your hardware, new nodes, smart
materials and code are all welcome.

- **Questions and chat:** [Discord](https://discord.gg/TqKD5xKp7Y)
- **Bugs and feature requests:** [GitHub issues](https://github.com/alla2001/material-painter/issues/new/choose)
- **Where it's going:** the `#roadmap` channel on Discord

## Building

Install [Rust](https://rustup.rs) (stable), then:

```sh
git clone https://github.com/alla2001/material-painter
cd material-painter
cargo run --release
```

On Linux, install the windowing dev packages first:

```sh
sudo apt install pkg-config libxkbcommon-dev libwayland-dev libxcb-render0-dev \
    libxcb-shape0-dev libxcb-xfixes0-dev libgtk-3-dev
```

## Testing

```sh
cargo test                                   # unit tests (what CI runs)
cargo test --bin material_painter -- --ignored --test-threads 1   # GPU tests
```

The GPU tests render offscreen (no window) on your graphics card and write
images to `target/bake_test/` so you can look at the result. Run them when
you touch rendering, baking, painting, nodes or export. CI builds every pull
request with `-D warnings` on Windows, Linux and macOS, so keep the build
warning-free.

## Pull requests

1. **Open an issue or ask on Discord before anything big**, so we agree on the
   approach before you spend time on it. Small fixes can go straight to a PR.
2. **One change per PR.** A focused 100-line PR gets reviewed; a 3,000-line
   one that also reformats unrelated files does not.
3. **Run `cargo test`** (and the GPU tests if you touched GPU code), and say
   what you tested in the PR.
4. **Match the existing style:** `cargo fmt` is not enforced on purpose (the
   code uses its own wrapping), so format like the surrounding code. Doc
   comments explain *why*, not what.
5. **Screenshots or a short clip** for anything visible.

## Using AI tools

Most contributions will be written with AI help, and that's fine. Material
Painter itself was built with heavy AI assistance. What matters is the result
and that a human stands behind it:

- **Say so in the PR** (the template asks). No judgment; it helps review.
- **You must understand and have run what you submit.** Read the diff before
  you open the PR, build it, try the feature in the app, and run the tests.
  "The AI said it works" isn't testing.
- **Keep AI PRs small and focused.** Agents love to "improve" nearby code;
  revert anything that isn't part of your change.
- **Point your agent at [AGENTS.md](AGENTS.md).** It explains the
  architecture, the conventions and the mistakes agents commonly make here.
- **No generated filler:** no unneeded abstractions, no comments restating the
  code, no new dependencies without a reason.

PRs that clearly weren't read or run by their author will be closed, kindly.

## Other ways to help

- **Test on your hardware**, especially AMD and Intel GPUs, Linux
  distributions and Macs, and report how it runs (Help → About shows the
  version and GPU).
- **Write nodes:** Material Designer nodes are single WGSL files; see the
  Shader Maker tab and `src/graph/nodes/` for examples.
- **Make smart materials** and share them in Discord.
- **Support development** on [Ko-fi](https://ko-fi.com/allaeddinerakik).

## License

By contributing you agree that your contribution is licensed under the
project's license, [GPL-3.0-or-later](LICENSE).
