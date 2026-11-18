# Grace

## Commit Message Rules

### Recommended Format

In this course project, we recommend a lightweight [**Conventional Commits–style**](https://www.conventionalcommits.org/) format:

```text
<type>(optional scope)[!]: <short imperative summary>
```

### Guidelines

- Use **imperative mood** (e.g., “add”, “fix”, “remove”).
- Keep the entire subject line **≤ 50 characters**, including the type and scope.
- `scope` and `!` are optional — use them when helpful. Use `!` to indicate a breaking change.

In real projects, always follow the commit message convention used by your team or organization — even if it differs from this one.

### Common Types

- `feat`: new functionality
- `fix`: bug fixes
- `docs`: documentation changes
- `refactor`: code changes without behavior change
- `chore`: cleanup, configuration, maintenance
- `ci`: GitHub Actions / automation

### Examples

```text
feat: add WebSocket updates for task status
feat(storage): mount a postgres volume
fix(compose)!: rename db service to postgres
docs: add local dev setup instructions
refactor(api): simplify request validation
```

## Email verification

New users register in two steps: `POST /api/auth/register` (username, password, email) emails a six-digit
code, and `POST /api/auth/verify` (email, code) creates the account and its first session. Nothing exists in
`users` or `sessions` until the code is accepted. Existing accounts keep signing in with username and password.
Each registration allows 5 wrong codes and one email per address per 60 seconds, with at most 5 emails;
a resend repeats the same code, so earlier emails stay valid.

The backend reads these environment variables. In Kubernetes they come from the Secret `grace-sendgrid`
(optional, so pods still start without it; registration answers `503` until it exists). Never commit them.

| Variable | Purpose |
| --- | --- |
| `SENDGRID_API_KEY` | SendGrid API key |
| `SENDGRID_FROM_EMAIL` | Sender address verified in SendGrid |
| `DEMO_EMAIL`, `DEMO_CODE` | Optional. Together they let that one address register with the fixed code, once |

Create the Secret in the cluster before merging to `dev` (the key file stays on your machine):

```sh
kubectl -n grace create secret generic grace-sendgrid \
  --from-file=SENDGRID_API_KEY=sendgrid-api-key.txt \
  --from-literal=SENDGRID_FROM_EMAIL='verified-sender@example.com' \
  --from-literal=DEMO_EMAIL='demo@example.com' --from-literal=DEMO_CODE='123456'
kubectl -n grace rollout restart deployment/app   # env from a Secret is read at start-up
```

After deploying, register a real address on the live site and confirm the email arrives. Locally, put the same
variables in an untracked `.env` next to `compose.yaml`.

The smoke tests (`tests/smoke.py`) skip registration. They create their test users and sessions directly in the
database through the psql command in `SMOKE_PSQL` (set by CI and `scripts/deploy-dev.sh`) and delete them afterwards.

## CI and deployment

A PR to `dev` runs the `verify` check on an isolated Linux runner. It runs Rust tests,
frontend checks, an offline render of the DOKS configuration, and the HTTP smoke tests
against the production image and a temporary PostgreSQL 17 database. For the static frontend,
CI checks JavaScript syntax. For the npm frontend, CI runs lint and the Docker build
type-checks the application. The check compares the served homepage, JavaScript and
CSS with the files from the image. It does not exercise browser interactions.

### Enable automatic releases

In GitHub Settings → Environments → `ci-cd-pipeline`, allow the **branch**
`dev` to deploy. The existing `DIGITALOCEAN_ACCESS_TOKEN` secret is used by
the deploy job. Its DigitalOcean scopes are `registry:read`, `registry:update`,
`kubernetes:read` and `kubernetes:access_cluster`. Pull requests run CI only.
To require a green PR before merging, make `verify` a required check on `dev`.

Merge the pipeline PR first. Its V6 release promotes the published V4 image
by digest and tests it against a fresh PostgreSQL 17 database. Wait for V6
to pass the public checks, then merge the application PR. V7 builds and ships
the application from that merged commit. Use squash or merge commits for the
two PRs, with no intervening pushes to `dev`.

### Routine releases

Each push to `dev` verifies the merged commit. Release numbers start at V6,
counting first-parent commits after `bcffa6b`. A skipped commit can leave
a gap in the numbers. The commit SHA and the version label identify the
image in the Grace registry. Retries reuse a previously published SHA.
The workflow deploys by digest and records the old and new images in Actions.

The job checks the exact registry image in Compose, dry-runs the Kubernetes
manifest and checks the current head of `dev` before applying it. It verifies
the rollouts, database PVC, public health, frontend files, grading and sign-in
recovery. Each public business check creates two test accounts and private
sample papers. Inspect the Actions summary for the release number and result.

If acceptance fails after switching the app, the job restores its previous
image and checks the public homepage and readiness. This rollback covers
the application image. For a release that changes database schema or other
Kubernetes resources, plan their recovery separately. Retry failed jobs
to reuse the one-day CI artifact, or rerun all jobs if it has expired.

## Database backups

DOKS runs a daily `CronJob` (`postgres-backup` at 05:00 AM `America/Toronto`) that
exports PostgreSQL with `pg_dump -Fc` and uploads private archives to DigitalOcean
Spaces with 7-day retention. See [the DOKS backup and restore runbook](k8s/doks/README.md)
for manual trigger, verification, and `pg_restore` recovery commands.
