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

## CI and deployment

A PR to `dev` runs the `verify` check on an isolated Linux runner. It runs Rust tests,
frontend lint, an offline render of the DOKS configuration, and the HTTP smoke tests
against the production image and a temporary PostgreSQL 17 database. The Docker build
also type-checks the frontend. The check compares the served homepage, JavaScript and
CSS with the files from the image. It does not exercise browser interactions.

### Enable the first release

1. Land the intended application version on `dev` before merging the workflow.
   Only committed files reach GitHub. Record the current release configuration and
   image digest as the first rollback point; a tag such as `v4` may be reassigned.
2. In GitHub **Settings → Environments**, create `production` and restrict deployments
   to the `dev` branch. Add the environment secret `DIGITALOCEAN_ACCESS_TOKEN`.
   Its DigitalOcean custom scopes are `registry:read`, `registry:update`,
   `kubernetes:read` and `kubernetes:access_cluster`. The identity must also be able
   to read and apply the resources in the `grace` namespace and watch rollouts.
   The existing namespace and `grace-db` Secret must already be present.
3. Open a PR containing the workflow. After `verify` appears, configure the `dev`
   branch rule to require a PR and a passing `verify` check. During setup, confirm
   that a failing check blocks the PR and a corrected run passes.
4. After the first successful deployment, run the full business smoke test once
   against the public site. This creates two test accounts and private sample papers.
   Run it from the checkout matching the deployed commit:

   ```sh
   python3 tests/smoke.py https://146-190-191-107.sslip.io
   ```

### Routine releases

A push to `dev` verifies the merged commit again. The deploy job loads that exact
tested image, publishes it under the full Git SHA and renders the tracked DOKS overlay
with the registry digest. The tracked `newTag` is the manual build default.
Use the digest renderer for automated releases and recovery so a plain overlay apply
does not switch the app back to that tag.

The job checks the entire manifest with a server-side dry run, then applies the same
file. It waits for the database, app and gateway rollouts, checks ready replicas and
the bound database PVC, and compares the public homepage and assets with the release.
Only one `dev` run deploys at a time. A superseded commit skips the cluster update.
The Actions summary records the commit, old image, target digest and verification result.

### Failed release and recovery

Check the failed step first. A failure before the real apply leaves the cluster
unchanged. An apply or rollout can fail after some resources have changed. Inspect
`kubectl --context do-tor1-grace -n grace get deploy,sts,pods,pvc,hpa` and inspect
the events or startup logs for the affected workload. For a check that failed after
deployment, compare the live image and health with the run summary before deciding
to roll back. Re-run the current `dev` workflow to retry a transient failure; run all
jobs if the one-day image artifact has expired.

For a rollback, use an isolated checkout of the last successful deployment commit
and its recorded image digest. Confirm the old application works with the current
database schema: application startup may already have applied a migration. In a
release checkout containing the renderer, run:

```sh
# Set IMAGE to the full registry.digitalocean.com/grace-dev/grace@sha256:... from the successful run.
bash scripts/render-release.sh "$IMAGE" > /tmp/grace-rollback.yaml &&
  kubectl --context do-tor1-grace -n grace apply --dry-run=server -f /tmp/grace-rollback.yaml &&
  kubectl --context do-tor1-grace -n grace apply -f /tmp/grace-rollback.yaml
```

For the pre-pipeline release, render the saved baseline configuration with its recorded
digest using the same renderer. Check all three rollouts, the live app image, bound
database PVC and the public health and frontend at the recovered version. Apply restores
the resources in the manifest; it does not prune resources added later or reverse
database migrations. Record the recovery and correct `dev` before the next release.
