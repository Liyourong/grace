#!/usr/bin/env bash
# Render the tracked DOKS overlay with the tested image. Run from the repo root.
set -euo pipefail
image=${1:?usage: render-release.sh registry/repository@sha256:digest}
repository=registry.digitalocean.com/grace-dev/grace
digest=${image#*@}
[[ "$image" == "$repository@$digest" && "$digest" =~ ^sha256:[0-9a-f]{64}$ ]] || {
  echo "Expected a Grace registry image pinned by SHA-256" >&2
  exit 1
}

dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT
cp -R k8s "$dir/"
cat > "$dir/kustomization.yaml" <<YAML
resources:
  - k8s/doks
images:
  - name: $repository
    newName: $repository
    digest: $digest
YAML
kubectl kustomize "$dir" > "$dir/release.yaml"
# Validate the generated objects before passing the manifest to kubectl apply.
ruby -ryaml -e '
  docs = YAML.load_stream(File.read(ARGV[0]))
  abort "Unexpected namespace" unless docs.all? { |d| d.dig("metadata", "namespace") == "grace" }
  ids = docs.to_h { |d| [[d["kind"], d.dig("metadata", "name")].join("/"), d] }
  required = %w[Deployment/app Deployment/caddy Service/app Service/db Service/caddy StatefulSet/db HorizontalPodAutoscaler/app PersistentVolumeClaim/caddy-data CronJob/postgres-backup]
  missing = required - ids.keys
  abort "Missing: #{missing.join(", ")}" unless missing.empty?
  containers = ids.fetch("Deployment/app").dig("spec", "template", "spec", "containers")
  abort "Grace image mismatch" unless containers.any? { |c| c["name"] == "grace" && c["image"] == ARGV[1] }
' "$dir/release.yaml" "$image"
cat "$dir/release.yaml"
