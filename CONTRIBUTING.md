# Contributing

Issues and pull requests are welcome. For anything larger than a fix or a new rule, open an issue first and say what
you want to change, so the work is not wasted if it does not fit.

## Build and test

You need Rust (edition 2024) and a Unix shell with `tar` and `gzip`: the tests run offline, against the sample
repositories in `examples/` and a fake platform. The tests of `install.sh` (in `crates/exodus/tests/platform.rs`) pack
the build as a release in Cargo's test scratch folder and run the script with a stand-in `curl`.

```sh
cargo build
cargo run -q -- --help
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same three checks on Linux, macOS and Windows for every push and pull request; a pull request is merged when they
pass.

## What to keep in mind

- **No secret values.** The audit reads the names of environment variables, never their values, and never opens
  `.env` or other files that hold values. A line copied into the findings is redacted (`crates/exodus-audit/src/redact.rs`).
  A key is never put on argv, printed, logged or shown in an error. A change that touches any of this needs a test.
- **Only the binary talks to the network.** `exodus-schema`, `exodus-audit`, `exodus-cost` and `exodus-report` never
  do; `crates/exodus` talks only to the platform's API.
- **`unwrap` and `expect` only in tests.** Elsewhere an error goes up to the command, which prints one line and exits
  non-zero.
- **The report is a contract.** The platform builds reports with the same crates, so a change to `exodus-report` or
  `exodus-cost` changes what the cabinet shows. The snapshots in `crates/exodus-report/tests/snapshots/` pin it: when
  a change is meant, `UPDATE_SNAPSHOTS=1 cargo test -p exodus-report` rewrites them, and the diff goes in the same
  pull request.
- **Prices are dated.** `crates/exodus-cost/prices.json` holds Google's list prices with the date they were read; tests
  pin each free tier to Google's published numbers. A price change updates the date and says where it was read.
- **Plain text.** Messages, rules and templates are short, factual sentences in English.

## Adding a pattern rule

A rule is a TOML file in `crates/exodus-audit/rules/<language>/<id>.toml`. The fields, how hits become findings, and
the tests a new rule needs are in [rules/README.md](crates/exodus-audit/rules/README.md). Keep the regex narrow: a
finding asks someone to change code, and a false one costs their trust.

## Sample repositories

`examples/` holds four made-up apps and the findings expected of each (`expected.json`, written by hand). If your
change makes the audit find something new in a sample, update its `expected.json` and say why in the pull request.
[examples/README.md](examples/README.md) has the comparison rules.

## Releases

A tag `v*` builds the binaries (Linux musl and macOS, x86_64 and arm64), `SHA256SUMS` and `install.sh` into a GitHub
release (`.github/workflows/release.yml`). A tag with a hyphen (`v0.2.0-rc`) makes a prerelease, which the installer's
default skips. The version in `Cargo.toml` stays `0.0.0`; the tag sets it at build time.

## License

By contributing you agree that your contribution is licensed under the [Apache License 2.0](LICENSE), as section 5 of
the license says.
