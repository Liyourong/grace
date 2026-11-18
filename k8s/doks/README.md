# DOKS PostgreSQL Backup & Restore Runbook

## Overview

- **CronJob:** `postgres-backup` in namespace `grace` ([backup.yaml](./backup.yaml))
- **Schedule:** `0 5 * * *` in `America/Toronto` (daily at 05:00 AM Toronto time)
- **Concurrency:** `concurrencyPolicy: Forbid` (prevents overlapping scheduled runs)
- **Backup format:** PostgreSQL custom archive (`pg_dump -Fc --no-owner --no-acl`)
- **Destination:** DigitalOcean Spaces bucket `grace-postgres-backups` (`tor1`, endpoint `https://tor1.digitaloceanspaces.com`) under `postgres/grace-<YYYYMMDDTHHMMSSZ>.dump` with `--acl private`
- **Retention:** After an upload is verified via `head-object`, [backup.sh](./backup.sh) deletes `.dump` objects under `postgres/` older than **7 days**. If `pg_dump` or the upload fails, the Pod exits nonzero and skips cleanup so existing backups are preserved.
- **Secrets:** Database password is read from `Secret/grace-db` (`password`), and Spaces credentials are read from `Secret/grace-backup-spaces` (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`). Neither credentials nor backup data are logged or stored in Git.

## 1. Trigger a Backup Manually & Check Job Result

Create a one-off Job from the deployed CronJob and wait for completion:

```bash
JOB_NAME="manual-backup-$(date -u +%Y%m%d%H%M%S)"
kubectl -n grace create job --from=cronjob/postgres-backup "$JOB_NAME"
kubectl -n grace wait --for=condition=complete "job/$JOB_NAME" --timeout=180s
kubectl -n grace get "job/$JOB_NAME"
kubectl -n grace logs "job/$JOB_NAME" --all-containers=true
```

If the Job fails, inspect Pod status and events:

```bash
kubectl -n grace get pods -l "job-name=$JOB_NAME"
kubectl -n grace describe "job/$JOB_NAME"
```

## 2. List Backups & Verify Private Access

Export `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` into your local shell (for example, by reading `Secret/grace-backup-spaces` from the cluster—never paste keys into files tracked by Git):

```bash
export AWS_ACCESS_KEY_ID="$(kubectl -n grace get secret grace-backup-spaces -o jsonpath='{.data.AWS_ACCESS_KEY_ID}' | base64 --decode)"
export AWS_SECRET_ACCESS_KEY="$(kubectl -n grace get secret grace-backup-spaces -o jsonpath='{.data.AWS_SECRET_ACCESS_KEY}' | base64 --decode)"
```

List available backup archives:

```bash
docker run --rm --entrypoint aws \
  -e AWS_ACCESS_KEY_ID \
  -e AWS_SECRET_ACCESS_KEY \
  -e AWS_EC2_METADATA_DISABLED=true \
  amazon/aws-cli:2.41.0 \
  s3 ls s3://grace-postgres-backups/postgres/ \
  --endpoint-url https://tor1.digitaloceanspaces.com \
  --region tor1
```

Confirm anonymous requests cannot download the archive (`HTTP/1.1 403 Forbidden` or `403`):

```bash
ARCHIVE="grace-YYYYMMDDTHHMMSSZ.dump"
curl -sS -o /dev/null -w "%{http_code}\n" \
  "https://grace-postgres-backups.tor1.digitaloceanspaces.com/postgres/${ARCHIVE}"
```

## 3. Download & Restore Into a Separate Empty Database

1. Download the selected archive from Spaces into the current directory:

```bash
ARCHIVE="grace-YYYYMMDDTHHMMSSZ.dump"
docker run --rm --entrypoint aws \
  -e AWS_ACCESS_KEY_ID \
  -e AWS_SECRET_ACCESS_KEY \
  -e AWS_EC2_METADATA_DISABLED=true \
  -v "$PWD:/work" -w /work \
  amazon/aws-cli:2.41.0 \
  s3 cp "s3://grace-postgres-backups/postgres/${ARCHIVE}" "./${ARCHIVE}" \
  --endpoint-url https://tor1.digitaloceanspaces.com \
  --region tor1
```

2. Create a separate empty test database (`grace_restore_test`) on a target PostgreSQL 17 instance and restore the archive with `pg_restore`. To test against a local disposable PostgreSQL container (recommended so you do not risk touching production):

```bash
docker run -d --name grace-restore-pg \
  -e POSTGRES_USER=grace \
  -e POSTGRES_PASSWORD=restore_test_only \
  -e POSTGRES_DB=grace \
  -p 127.0.0.1:15432:5432 \
  postgres:17

docker exec -e PGPASSWORD=restore_test_only grace-restore-pg \
  psql -U grace -d grace -v ON_ERROR_STOP=1 -c "CREATE DATABASE grace_restore_test;"

docker run --rm --network container:grace-restore-pg \
  -e PGPASSWORD=restore_test_only \
  -v "$PWD:/work" -w /work \
  postgres:17 \
  pg_restore \
    --clean \
    --if-exists \
    --no-owner \
    --no-acl \
    --single-transaction \
    --exit-on-error \
    -Fc \
    -h 127.0.0.1 \
    -U grace \
    -d grace_restore_test \
    "./${ARCHIVE}"
```

Or, if using local `psql` and `pg_restore` against an existing PostgreSQL instance:

```bash
psql "$ADMIN_DATABASE_URL" -v ON_ERROR_STOP=1 -c "CREATE DATABASE grace_restore_test;"
pg_restore \
  --clean \
  --if-exists \
  --no-owner \
  --no-acl \
  --single-transaction \
  --exit-on-error \
  -Fc \
  -d "$RESTORE_DATABASE_URL" \
  "./${ARCHIVE}"
```

3. Start a test Grace instance with `DATABASE_URL` pointing to `grace_restore_test` (for example, `postgres://grace:restore_test_only@127.0.0.1:15432/grace_restore_test`), sign in with an existing test account, and verify its papers, saved answers, and practice position. Clean up the disposable container afterward with `docker rm -f grace-restore-pg`.
