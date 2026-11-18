#!/usr/bin/env bash
set -euo pipefail

# Never enable shell tracing (`set -x`) so database and Spaces credentials never appear in logs.

BACKUP_DIR="${BACKUP_DIR:-/backup}"
PGHOST="${PGHOST:-db}"
PGPORT="${PGPORT:-5432}"
PGDATABASE="${PGDATABASE:-grace}"
PGUSER="${PGUSER:-grace}"
SPACES_ENDPOINT="${SPACES_ENDPOINT:-https://tor1.digitaloceanspaces.com}"
SPACES_BUCKET="${SPACES_BUCKET:-grace-postgres-backups}"
AWS_DEFAULT_REGION="${AWS_DEFAULT_REGION:-tor1}"
BACKUP_PREFIX="${BACKUP_PREFIX:-postgres/}"
RETENTION_DAYS="${RETENTION_DAYS:-7}"

file_size() {
  wc -c < "$1" | tr -d '[:space:]'
}

spaces_aws() {
  aws --endpoint-url "$SPACES_ENDPOINT" --region "$AWS_DEFAULT_REGION" "$@"
}

normalize_prefix() {
  local prefix="$1"
  prefix="${prefix#/}"
  if [[ -n "$prefix" && "$prefix" != */ ]]; then
    prefix="${prefix}/"
  fi
  printf '%s' "$prefix"
}

run_dump() {
  : "${PGPASSWORD:?PGPASSWORD must be set}"

  mkdir -p "$BACKUP_DIR"
  local timestamp filename dump_path tmp_path size_bytes
  timestamp="$(date -u '+%Y%m%dT%H%M%SZ')"
  filename="grace-${timestamp}.dump"
  dump_path="${BACKUP_DIR}/${filename}"
  tmp_path="${dump_path}.tmp"

  rm -f "$tmp_path"
  echo "Starting pg_dump for database '${PGDATABASE}' on '${PGHOST}:${PGPORT}'..."
  pg_dump -Fc --no-owner --no-acl -w \
    -h "$PGHOST" \
    -p "$PGPORT" \
    -U "$PGUSER" \
    -d "$PGDATABASE" \
    -f "$tmp_path"

  if [[ ! -s "$tmp_path" ]]; then
    echo "pg_dump produced an empty archive at ${tmp_path}" >&2
    rm -f "$tmp_path"
    exit 1
  fi

  # Validate the custom-format table of contents before passing the archive to upload.
  pg_restore -l "$tmp_path" >/dev/null

  mv "$tmp_path" "$dump_path"
  printf '%s\n' "$filename" > "${BACKUP_DIR}/latest-filename"

  size_bytes="$(file_size "$dump_path")"
  echo "Local dump completed: ${filename} (${size_bytes} bytes)"
}

cleanup_old_backups() {
  local prefix="$1"
  local current_key="$2"

  if [[ ! "$RETENTION_DAYS" =~ ^[0-9]+$ ]]; then
    echo "Invalid RETENTION_DAYS '${RETENTION_DAYS}'; expected non-negative integer" >&2
    exit 1
  fi

  local cutoff_epoch objects
  cutoff_epoch=$(( $(date -u +%s) - RETENTION_DAYS * 86400 ))
  objects="$(spaces_aws s3api list-objects-v2 \
    --bucket "$SPACES_BUCKET" \
    --prefix "$prefix" \
    --query 'Contents[].[Key,LastModified]' \
    --output text)"

  if [[ -z "$objects" || "$objects" == "None" ]]; then
    echo "No existing objects found under s3://${SPACES_BUCKET}/${prefix} for retention check."
    return 0
  fi

  local deleted_count=0
  local key last_modified obj_epoch
  while IFS=$'\t' read -r key last_modified; do
    [[ -z "$key" || "$key" == "None" ]] && continue
    [[ "$key" == */ || "$key" != *.dump ]] && continue
    [[ "$key" == "$current_key" ]] && continue
    [[ -z "$last_modified" || "$last_modified" == "None" ]] && continue

    obj_epoch="$(date -u -d "$last_modified" +%s)"
    if (( obj_epoch < cutoff_epoch )); then
      spaces_aws s3api delete-object --bucket "$SPACES_BUCKET" --key "$key" >/dev/null
      echo "Deleted expired backup (> ${RETENTION_DAYS}d): s3://${SPACES_BUCKET}/${key}"
      deleted_count=$((deleted_count + 1))
    fi
  done <<< "$objects"

  echo "Retention check complete (${deleted_count} expired backup(s) deleted)."
}

run_upload() {
  : "${AWS_ACCESS_KEY_ID:?AWS_ACCESS_KEY_ID must be set}"
  : "${AWS_SECRET_ACCESS_KEY:?AWS_SECRET_ACCESS_KEY must be set}"

  local marker="${BACKUP_DIR}/latest-filename"
  if [[ ! -s "$marker" ]]; then
    echo "Missing dump filename marker at ${marker}; skipping upload and cleanup." >&2
    exit 1
  fi

  local filename dump_path prefix object_key local_size remote_size
  filename="$(tr -d '[:space:]' < "$marker")"
  dump_path="${BACKUP_DIR}/${filename}"
  if [[ ! -s "$dump_path" ]]; then
    echo "Backup archive not found or empty at ${dump_path}; skipping upload and cleanup." >&2
    exit 1
  fi

  prefix="$(normalize_prefix "$BACKUP_PREFIX")"
  object_key="${prefix}${filename}"
  local_size="$(file_size "$dump_path")"

  echo "Uploading ${filename} (${local_size} bytes) to s3://${SPACES_BUCKET}/${object_key}..."
  spaces_aws s3 cp "$dump_path" "s3://${SPACES_BUCKET}/${object_key}" \
    --acl private \
    --only-show-errors

  remote_size="$(spaces_aws s3api head-object \
    --bucket "$SPACES_BUCKET" \
    --key "$object_key" \
    --query 'ContentLength' \
    --output text | tr -d '[:space:]')"

  if [[ -z "$remote_size" || "$remote_size" == "None" || "$remote_size" != "$local_size" ]]; then
    echo "Upload verification failed for s3://${SPACES_BUCKET}/${object_key} (local=${local_size}, remote=${remote_size})" >&2
    exit 1
  fi

  # Only run retention cleanup after the new archive upload is verified.
  cleanup_old_backups "$prefix" "$object_key"

  echo "Backup succeeded: s3://${SPACES_BUCKET}/${object_key} (${remote_size} bytes, private)"
}

case "${1:-}" in
  dump)
    run_dump
    ;;
  upload)
    run_upload
    ;;
  *)
    echo "Usage: $0 {dump|upload}" >&2
    exit 1
    ;;
esac
