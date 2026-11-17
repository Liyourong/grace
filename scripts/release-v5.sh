#!/usr/bin/env bash
# One release rehearsal for the existing Grace site. Invoke from the tag workflow.
set -euo pipefail
[[ "$GITHUB_REF" == refs/tags/cd-test-v5 ]]
: "${DIGITALOCEAN_ACCESS_TOKEN:?Missing ci-cd-pipeline environment secret}"
base=https://146-190-191-107.sslip.io
prior_digest=sha256:3895b734e3e85ffff0904552df0b8c51a64c9bd4412f3e9a152f19e183ddbb69
k=(kubectl -n grace)
check_head() {
  local head
  head=$(gh api "repos/$GITHUB_REPOSITORY/git/ref/heads/ci/cd-v5" --jq .object.sha)
  [[ "$head" == "$GITHUB_SHA" ]]
}

doctl registry login --expiry-seconds 1800
doctl kubernetes cluster kubeconfig save grace --expiry-seconds 1800
check_head
# Require a zero diff against the v4 baseline, including the DB, gateway and storage.
baseline="$RUNNER_TEMP/baseline.yaml"
live=$("${k[@]}" get deployment/app -o jsonpath='{.spec.template.spec.containers[0].image}')
case "$live" in
  "$REGISTRY_IMAGE:v4") kubectl kustomize k8s/doks > "$baseline" ;;
  "$REGISTRY_IMAGE@$prior_digest") bash scripts/render-release.sh "$live" > "$baseline" ;;
  *) echo "Expected the verified v4 baseline, found $live" >&2; exit 1 ;;
esac
"${k[@]}" diff -f "$baseline"
"${k[@]}" get pods -l app=grace -o json | jq -e --arg ref "$REGISTRY_IMAGE@$prior_digest" '
  (.items | length) > 0 and all(.items[]; any(.status.containerStatuses[]; .name == "grace" and .imageID == $ref and .ready))'
curl -fsS --max-time 15 "$base/" -o "$RUNNER_TEMP/v4.html"

tag="$REGISTRY_IMAGE:$GITHUB_SHA"
docker load -i "$RUNNER_TEMP/release/image.tar"
docker push "$tag"
# Resolve the immutable digest in the remote registry, including on fresh runners.
registry_tags() { doctl registry repository list-tags grace -o json; }
# The DO tag listing can lag behind a successful docker push.
wait_for_tag() {
  local name=$1 expected=${2:-} tags found attempt
  for ((attempt=1; attempt<=12; attempt++)); do
    tags=$(registry_tags) || return 1
    found=$(jq -r --arg tag "$name" '.[] | select(.tag == $tag) | .manifest_digest' <<< "$tags") || return 1
    if [[ "$found" =~ ^sha256:[0-9a-f]{64}$ && ( -z "$expected" || "$found" == "$expected" ) ]]; then
      printf '%s\n' "$found"
      return 0
    fi
    if ((attempt < 12)); then sleep 5; fi
  done
  echo "Registry did not confirm tag $name within the verification window" >&2
  return 1
}
digest=$(wait_for_tag "$GITHUB_SHA")
image="$REGISTRY_IMAGE@$digest"
existing=$(registry_tags | jq -r '.[] | select(.tag == "v5") | .manifest_digest')
[[ -z "$existing" || "$existing" == "$digest" ]] || { echo "v5 is already in use"; exit 1; }
docker tag "$tag" "$REGISTRY_IMAGE:v5"
docker push "$REGISTRY_IMAGE:v5"
wait_for_tag v5 "$digest" > /dev/null

manifest="$RUNNER_TEMP/release.yaml"
bash scripts/render-release.sh "$image" > "$manifest"
"${k[@]}" apply --dry-run=server -f "$manifest"
check_head
"${k[@]}" diff -f "$baseline"
echo "Commit: $GITHUB_SHA | Target: $image | Previous: $REGISTRY_IMAGE@$prior_digest" >> "$GITHUB_STEP_SUMMARY"

# Existing proxy connections can still serve the retiring pod after rollout status succeeds.
wait_for_public() {
  local attempt
  for ((attempt=1; attempt<=12; attempt++)); do
    if "$@"; then return 0; fi
    if ((attempt < 12)); then sleep 5; fi
  done
  return 1
}
v4_homepage() {
  curl -fsS --max-time 15 "$base/" | cmp - "$RUNNER_TEMP/v4.html"
}

recover() {
  rc=$?
  trap - ERR
  live=$("${k[@]}" get deployment/app -o jsonpath='{.spec.template.spec.containers[0].image}')
  if [[ "$live" == "$image" ]]; then
    "${k[@]}" set image deployment/app "grace=$REGISTRY_IMAGE@$prior_digest"
    "${k[@]}" rollout status deployment/app --timeout=4m
    curl -fsS --retry 3 --max-time 15 "$base/health/ready"
    wait_for_public v4_homepage
    echo "V5 failed acceptance. Restored the previous v4 image and verified the public homepage." >> "$GITHUB_STEP_SUMMARY"
  fi
  exit "$rc"
}
trap recover ERR
"${k[@]}" apply -f "$manifest"
for resource in statefulset/db deployment/app deployment/caddy; do
  "${k[@]}" rollout status "$resource" --timeout=4m
done
"${k[@]}" get deployment/app -o json | jq -e --arg image "$image" '
  any(.spec.template.spec.containers[]; .name == "grace" and .image == $image)
  and (.status.readyReplicas > 0 and .status.readyReplicas == .spec.replicas)'
"${k[@]}" wait --for=jsonpath='{.status.phase}'=Bound pvc/data-db-0 --timeout=60s
wait_for_public python3 tests/check_site.py "$base" "$RUNNER_TEMP/release/site"
python3 tests/smoke.py "$base"
echo "Verified v5 at $base: live image, rollout, PVC, health, assets and business smoke tests." >> "$GITHUB_STEP_SUMMARY"
