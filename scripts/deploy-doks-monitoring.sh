#!/usr/bin/env bash
set -euo pipefail
context=${1:?usage: bash scripts/deploy-doks-monitoring.sh CONTEXT}
helm upgrade --install grace-monitor prometheus \
  --repo https://prometheus-community.github.io/helm-charts --version 29.47.2 \
  --kube-context "$context" -n grace -f k8s/monitoring/prometheus-values.yaml \
  --atomic --timeout 5m
# Preserve a rehearsal explicitly enabled by an operator. Fresh installs are read-only.
dashboard=$(kubectl --context "$context" -n grace get deployment/grace-dashboard --ignore-not-found -o json)
if [[ -n "$dashboard" ]] && jq -e 'any(.spec.template.spec.containers[].env[]?; .name=="GRACE_HPA_DEMO" and .value=="1")' <<< "$dashboard" >/dev/null; then
  bash scripts/dashboard-deploy.sh "$context" --enable-hpa-demo
else
  bash scripts/dashboard-deploy.sh "$context"
fi
