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
kubectl -n grace diff -k k8s/doks
"${k[@]}" get pods -l app=grace -o json | jq -e --arg ref "$REGISTRY_IMAGE@$prior_digest" '
  (.items | length) > 0 and all(.items[]; any(.status.containerStatuses[]; .name == "grace" and .imageID == $ref and .ready))'
curl -fsS --max-time 15 "$base/" -o "$RUNNER_TEMP/v4.html"

tag="$REGISTRY_IMAGE:$GITHUB_SHA"
docker load -i "$RUNNER_TEMP/release/image.tar"
docker push "$tag"
# Resolve the immutable digest in the remote registry, including on fresh runners.
registry_tags() { doctl registry repository list-tags grace -o json; }
digest=$(registry_tags | jq -er --arg tag "$GITHUB_SHA" '.[] | select(.tag == $tag) | .manifest_digest')
image="$REGISTRY_IMAGE@$digest"
existing=$(registry_tags | jq -r '.[] | select(.tag == "v5") | .manifest_digest')
[[ -z "$existing" || "$existing" == "$digest" ]] || { echo "v5 is already in use"; exit 1; }
docker tag "$tag" "$REGISTRY_IMAGE:v5"
docker push "$REGISTRY_IMAGE:v5"
registry_tags | jq -e --arg d "$digest" 'any(.[]; .tag == "v5" and .manifest_digest == $d)'

manifest="$RUNNER_TEMP/release.yaml"
bash scripts/render-release.sh "$image" > "$manifest"
"${k[@]}" apply --dry-run=server -f "$manifest"
check_head
"${k[@]}" diff -k k8s/doks
echo "Commit: $GITHUB_SHA | Target: $image | Previous: $REGISTRY_IMAGE@$prior_digest" >> "$GITHUB_STEP_SUMMARY"

recover() {
  rc=$?
  trap - ERR
  live=$("${k[@]}" get deployment/app -o jsonpath='{.spec.template.spec.containers[0].image}')
  if [[ "$live" == "$image" ]]; then
    "${k[@]}" set image deployment/app "grace=$REGISTRY_IMAGE@$prior_digest"
    "${k[@]}" rollout status deployment/app --timeout=4m
    curl -fsS --retry 3 --max-time 15 "$base/health/ready"
    curl -fsS --max-time 15 "$base/" | cmp - "$RUNNER_TEMP/v4.html"
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
python3 tests/check_site.py "$base" "$RUNNER_TEMP/release/site"
python3 tests/smoke.py "$base"
echo "Verified v5 at $base: live image, rollout, PVC, health, assets and business smoke tests." >> "$GITHUB_STEP_SUMMARY"
