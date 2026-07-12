# CI — zero GitHub-hosted-runner spend

Two independent gates; neither consumes paid GitHub minutes:

1. **Local pre-push gate** (works today, no infra):
   ```bash
   git config core.hooksPath .githooks
   ```
   Every `git push` then runs the compile check + lib unit tests and blocks on failure
   (`git push --no-verify` bypasses in an emergency).

2. **GitHub Actions on a self-hosted runner** (`.github/workflows/ci.yml`, `runs-on: [self-hosted]`
   — GitHub bills $0 for self-hosted runners). One-paste runner install (any always-on box or
   this machine): repo → Settings → Actions → Runners → "New self-hosted runner" and follow the
   three commands GitHub shows (download, `./config.sh --url <repo> --token <token>`, `./run.sh`;
   use `./svc.sh install && ./svc.sh start` for a persistent service). Until a runner is
   registered, workflow runs simply queue — they never bill.
