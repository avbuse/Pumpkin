# 06 — Dependency and secret handling

## Secrets

- Never commit, print, log, paste into an issue or comment, or place in a worker prompt any
  secret value: API keys, tokens, passwords, private keys, `.env` contents.
- Secrets live in Paperclip's secret store or the host's credential store and reach processes as
  environment variables.
- If a secret is exposed, stop, report it, and treat it as compromised.
- `.env` files stay untracked. Commit `.env.example` with placeholder values only.
- Before staging, check the diff for credential-shaped strings. Prefer adding files by name over
  `git add -A`.

## Dependencies

- Add a dependency only when it is justified by the task. State why in the PR.
- Prefer what the project already uses. Respect existing lockfiles and commit them with the change.
- Do not upgrade unrelated dependencies in a feature PR.
- Check licence compatibility and maintenance status for anything new.
- Do not run install scripts or fetch code from unvetted sources.
