#!/usr/bin/env bash
# Run from the repository root after releasing the instrumented Grace app.
set -euo pipefail
context=${1:?usage: bash scripts/dashboard-deploy.sh CONTEXT [--enable-hpa-demo]}
[[ $# == 1 || ( $# == 2 && $2 == --enable-hpa-demo ) ]] || exit 2
k=(kubectl --context "$context" -n grace)
demo=false
if [[ "$context" == grace-dash-demo || ${2:-} == --enable-hpa-demo ]]; then demo=true; fi

# Read the effective Pod spec: registry credentials may have been injected at admission.
"${k[@]}" rollout status deployment/app --timeout=4m
app=$("${k[@]}" get deployment/app -o json)
image=$(jq -er '.spec.template.spec.containers[] | select(.name == "grace")
  | select(any(.env[]?; .name == "METRICS_PORT" and .value == "9101")) | .image' <<< "$app") || {
  echo "Release the instrumented Grace app with METRICS_PORT=9101 first." >&2
  exit 1
}
pull_secrets=$("${k[@]}" get pods -l app=grace -o json | jq -ce --arg image "$image" '
  first(.items[] | select(.metadata.deletionTimestamp == null)
    | select(any(.status.conditions[]?; .type == "Ready" and .status == "True"))
    | select(any(.spec.containers[]; .name == "grace" and .image == $image))
    | (.spec.imagePullSecrets // []))')

dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT
cp -R k8s/monitoring "$dir/monitoring"
cat > "$dir/kustomization.yaml" <<'YAML'
resources: [monitoring]
patches:
  - path: dashboard-patch.json
YAML
jq -n --arg image "$image" --argjson secrets "$pull_secrets" --argjson demo "$demo" '
  {apiVersion:"apps/v1",kind:"Deployment",metadata:{name:"grace-dashboard",namespace:"grace"},
   spec:{template:{spec:{imagePullSecrets:$secrets,containers:[{name:"dashboard",image:$image,
     env:[{name:"GRACE_HPA_DEMO",value:(if $demo then "1" else "0" end)}]}]}}}}' > "$dir/dashboard-patch.json"
"${k[@]}" apply -k "$dir"
"${k[@]}" rollout status deployment/grace-dashboard --timeout=180s

# Verify the Service path and allow newly installed Prometheus time to scrape two samples.
proxy='/api/v1/namespaces/grace/services/http:grace-dashboard:http/proxy'
for attempt in {1..12}; do
  if "${k[@]}" get --request-timeout=10s --raw "$proxy/health/ready" >/dev/null &&
     "${k[@]}" get --request-timeout=12s --raw "$proxy/api/metrics" | jq -e '
    .end as $end | .series as $s | ["podIn","podOut"] | all(.[]; . as $key |
      any($s[$key][]?; .values[-1] | .[0] == $end and (.[1] | tonumber? | isfinite)))' >/dev/null; then
    break
  fi
  if [[ $attempt == 12 ]]; then
    echo "Dashboard readiness or current Pod traffic unavailable via the Service. Check the dashboard watches and Prometheus cAdvisor targets." >&2
    exit 1
  fi
  sleep 10
done

if [[ $demo == true ]]; then
  "${k[@]}" set env deployment/app --containers=grace GRACE_HPA_DEMO=1
elif jq -e '.spec.template.spec.containers[] | select(.name=="grace") | any(.env[]?; .name=="GRACE_HPA_DEMO")' <<< "$app" >/dev/null; then
  "${k[@]}" set env deployment/app --containers=grace GRACE_HPA_DEMO-
fi
"${k[@]}" rollout status deployment/app --timeout=4m
echo "Dashboard ready; inbound and outbound traffic verified through svc/grace-dashboard."
