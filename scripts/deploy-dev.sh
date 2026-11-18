#!/usr/bin/env bash
set -euo pipefail
[[ "$GITHUB_REF" == refs/heads/dev && "$GITHUB_EVENT_NAME" == push ]]
: "${DIGITALOCEAN_ACCESS_TOKEN:?Missing ci-cd-pipeline environment secret}"
version=$(bash scripts/release-version.sh "$GITHUB_SHA")
k=(kubectl -n grace)
retry() {
  local attempt
  for ((attempt=1; attempt<=12; attempt++)); do
    if "$@"; then return 0; fi
    if ((attempt < 12)); then sleep 5; fi
  done
  return 1
}
# Only an explicitly missing manifest is an unused tag. Auth and network errors stop the release.
digest_of() {
  local ref=$1 result
  if result=$(docker buildx imagetools inspect "$ref" --format '{{.Manifest.Digest}}' 2>&1); then
    [[ "$result" =~ ^sha256:[0-9a-f]{64}$ ]] || return 1
    printf '%s\n' "$result"
  elif [[ "$result" == "ERROR: $ref: not found" ]]; then
    return 0
  else
    echo "$result" >&2
    return 1
  fi
}
published_digest() {
  local digest
  digest=$(digest_of "$1") && [[ -n "$digest" ]] && printf '%s\n' "$digest"
}
tag_matches() {
  local digest
  digest=$(digest_of "$1") && [[ -n "$digest" && "$digest" == "$2" ]]
}
publish_alias() {
  local ref=$1 existing
  existing=$(digest_of "$ref")
  if [[ -z "$existing" ]]; then
    docker buildx imagetools create --prefer-index=false -t "$ref" "$image"
  elif [[ "$existing" != "${image##*@}" ]]; then
    echo "$ref already points to $existing" >&2
    return 1
  fi
  retry tag_matches "$ref" "${image##*@}"
}
check_head() {
  local head
  head=$(gh api "repos/$GITHUB_REPOSITORY/git/ref/heads/dev" --jq .object.sha)
  if [[ "$head" != "$GITHUB_SHA" ]]; then
    echo "Skipped superseded release $version. dev is now $head." >> "$GITHUB_STEP_SUMMARY"
    exit 0
  fi
}

doctl registry login --expiry-seconds 1800
doctl kubernetes cluster kubeconfig save grace --expiry-seconds 1800
check_head
tag="$REGISTRY_IMAGE:$GITHUB_SHA"
if [[ "$version" == v6 ]]; then
  # The original Chinese React V4 frontend is present in this published image.
  image="$REGISTRY_IMAGE@sha256:3895b734e3e85ffff0904552df0b8c51a64c9bd4412f3e9a152f19e183ddbb69"
  publish_alias "$tag"
else
  digest=$(digest_of "$tag")
  if [[ -z "$digest" ]]; then
    docker load -i "$RUNNER_TEMP/release/image.tar"
    docker push "$tag"
    digest=$(retry published_digest "$tag")
  fi
  image="$REGISTRY_IMAGE@$digest"
fi

# Test the exact registry image, including the V4 promotion and retries of a published SHA.
docker pull "$image"
export COMPOSE_PROJECT_NAME=grace-release
docker tag "$image" grace-release-app
docker compose up -d --no-build --wait --wait-timeout 120
site="$RUNNER_TEMP/registry-site"
mkdir -p "$site"
docker compose cp app:/app/frontend/. "$site/"
python3 tests/check_site.py http://localhost:8090 "$site"
python3 tests/smoke.py http://localhost:8090
docker compose down --volumes
check_head
publish_alias "$REGISTRY_IMAGE:$version"

manifest="$RUNNER_TEMP/release.yaml"
bash scripts/render-release.sh "$image" > "$manifest"
"${k[@]}" apply --dry-run=server -f "$manifest"
config=$("${k[@]}" get deployment/app -o jsonpath='{.spec.template.spec.containers[0].envFrom[0].configMapRef.name}')
host=$("${k[@]}" get configmap "$config" -o jsonpath='{.data.SITE_HOST}')
base="https://$host"
previous=$("${k[@]}" get deployment/app -o jsonpath='{.spec.template.spec.containers[0].image}')
[[ "${previous%@*}" == "$REGISTRY_IMAGE" && "${previous##*@}" =~ ^sha256:[0-9a-f]{64}$ ]]
curl -fsS --max-time 15 "$base/" -o "$RUNNER_TEMP/previous.html"
previous_homepage() {
  curl -fsS --max-time 15 "$base/" | cmp - "$RUNNER_TEMP/previous.html"
}
rollback() {
  local rc=$? live
  trap - ERR
  live=$("${k[@]}" get deployment/app -o jsonpath='{.spec.template.spec.containers[0].image}')
  if [[ "$live" == "$image" && "$previous" != "$image" ]]; then
    "${k[@]}" set image deployment/app "grace=$previous"
    "${k[@]}" rollout status deployment/app --timeout=4m
    retry curl -fsS --max-time 15 "$base/health/ready"
    retry previous_homepage
    echo "Restored the previous app image $previous and verified its public homepage." >> "$GITHUB_STEP_SUMMARY"
  fi
  exit "$rc"
}
check_head
echo "Release: $version | Commit: $GITHUB_SHA | Previous: $previous | Target: $image" >> "$GITHUB_STEP_SUMMARY"
trap rollback ERR
"${k[@]}" apply -f "$manifest"
for resource in statefulset/db deployment/app deployment/caddy; do
  "${k[@]}" rollout status "$resource" --timeout=4m
done
"${k[@]}" get deployment/app -o json | jq -e --arg image "$image" '
  any(.spec.template.spec.containers[]; .name == "grace" and .image == $image)
  and (.status.readyReplicas > 0 and .status.readyReplicas == .spec.replicas)'
"${k[@]}" wait --for=jsonpath='{.status.phase}'=Bound pvc/data-db-0 --timeout=60s
retry python3 tests/check_site.py "$base" "$site"
python3 tests/smoke.py "$base"
echo "Verified $version at $base (health, assets and business checks)." >> "$GITHUB_STEP_SUMMARY"
