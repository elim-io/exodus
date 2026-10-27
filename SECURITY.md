# Security

## Reporting a vulnerability

Please report a vulnerability privately, through GitHub: the **Security** tab of this repository, then **Report a
vulnerability**. Do not open a public issue for it. Say what you found, how to reproduce it, and what it lets someone
do. We answer in the advisory, and credit you when the fix is released unless you ask us not to.

The same form takes reports about the hosted platform the CLI talks to (`exodus.elim.io`, `app.exodus.elim.io`).

## Supported versions

Fixes go into the next release. Only the latest release is supported; `curl -fsSL https://exodus.elim.io/install | sh`
replaces an older binary with it.

## What exodus promises

These are the properties a report against this repository usually concerns:

- The audit reads the names of environment variables, never their values. It does not open `.env`, `.env.*` (except
  `.env.example`, `.env.sample`, `.env.template`, `.env.dist`, `example.env`), `*.env`, `.envrc`, `.dev.vars`,
  `*.tfvars`, `*.pem`, `*.key`, `*.p12` or `*.pfx`. A command or source line copied into the findings loses
  `NAME=value` assignments, secret-named flags, `user:password@` in URLs and the string after a secret-named key.
- Without a linked account the CLI sends nothing to Exodus. With one, it sends the findings and answers, the audit's
  progress lines, the plan's progress, the results of `exodus check` in CI and, when you run `exodus bill`, the billing
  rows it read; linking sends the repository's name, the machine's hostname and its OS. The findings include the
  agent's description of the project (`exodus describe`), three paragraphs in its own words, when it wrote one. It
  never sends a source file: a finding carries at most one line of code, 200 characters long.
- The project key is kept in `~/.config/exodus/credentials` with mode 0600 (its folder 0700); on Windows in `%APPDATA%\exodus\credentials`, which the profile's permissions keep to your account. It is never put on a
  command line, printed, logged or included in an error; `exodus status` shows only its id.
- The one-time code that links an account works once, for 15 minutes.
- Only the `exodus` binary opens network connections, and only to the platform's API: `https://app.exodus.elim.io`, or
  the address given with `--api` (https, or http to `localhost` only). `exodus bill` runs your own `gcloud` and `bq`,
  which talk to Google Cloud with your sign-in.
- `exodus init` writes only text: `.exodus/`, the `.gitignore` line and the agent instructions. It installs nothing.
  The instructions name Google's own Agent Skills (`google/skills`) and the Developer Knowledge MCP server and tell
  the agent to give you the commands to run; it does not run installers itself. That server's API key is meant to
  live in the environment variable `DEVELOPERKNOWLEDGE_API_KEY`, set by you; the instructions forbid putting it on a
  command line, in a file of the repository or in the chat.
- `install.sh` checks the downloaded archive against the release's `SHA256SUMS` before installing it. Both come from
  the same release, so this catches a broken or swapped download, not a compromised release. The binaries are not
  signed yet.
