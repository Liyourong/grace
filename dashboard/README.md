# Grace operations dashboard

The dashboard runs in its own Pod in the Grace cluster. Open the dashboard on the presenter's computer. The browser receives live Kubernetes snapshots and login completions. On an idle stream the server sends a heartbeat every five seconds. **Last contact** shows when this browser last received a snapshot or heartbeat. Hover over it for the last Kubernetes sync. The test client sends its requests through the normal Grace entry point.

## Open the dashboard

From the repository root, select the cluster used for the demo.

```bash
CTX=grace-dash-demo
kubectl --context "$CTX" -n grace port-forward deployment/grace-dashboard 9090:9090 9091:9091
```

Open **http://localhost:9090**. The link opens the full Operations dashboard on the Grace application origin. Sign in as `grace_test` if prompted. Direct links to Topology, Metrics, Readiness and Logs keep the selected tab through sign-in. The Grace home page opens the same dashboard. For local port-forwards, set `GRACE_APP_ORIGIN` when running the dashboard installer if the application URL differs from the first configured application origin. Port 9091 accepts the demo client’s timings on Pod loopback. Restart the forward if the dashboard Pod is replaced.

## Operations views

**Topology** shows the number of ready replicas, time to the first HPA increase and time to five ready replicas beside the load test button. The Minikube rehearsal samples CPU every 15 seconds and uses a 60-second scale-down stabilization window. DOKS uses its installed metrics provider.

The right sidebar charts average application CPU as a percentage of the Pods’ CPU requests. The dashed line shows the configured HPA target. During a load test the first observed reading above target is marked in amber. The HPA replica request and Pod creation events have their own timestamps.

**Logs** reads seven days of application operations from PostgreSQL. Filter by Pod, status or Trace ID and page through matching operations. Select an answer submission or login to inspect its processing steps. The application returns the same ID in the `X-Trace-Id` header. Prompts, passwords and answer text are excluded from the log.

**Readiness** includes the presenter probe and **Database persistence**. Submit an answer as the test account, select the environment, then copy and run each step separately. Compare the PostgreSQL start time, question ID and saved answer before and after the restart. Reload the Grace paper to show the saved answer. The cloud command briefly interrupts database service.

The public status site still serves read-only cluster information from port 9092. Privileged data and controls use the test account’s Grace session.

## One-click HPA test

On the **Topology** tab click **Start load test**. Each Grace app Pod requests a lease, runs an 800 ms CPU sample, and pauses for 200 ms. Set `GRACE_HPA_FAST=0` on the app to reproduce the previous 300 ms sample and 700 ms pause. The dashboard stops issuing samples when five replicas are available, after three minutes, or when you click **Stop load test**. Closing the tab leaves the server-side time limit in effect. Watch the node boxes for new Pods and the **Metrics** view for replica counts. After Kubernetes scales back down you can run the test again.

The install script enables this control for the local `grace-dash-demo` profile by default. On DOKS, an operator can enable it for a rehearsal as described below. Open the dashboard through its localhost port-forward on port 9090 (minikube) or 19090 (DOKS). Both the app and dashboard must use the image that includes the CPU test. The button exercises CPU-based autoscaling independently of the login timeline.

The right-hand **Live traffic** panel shows total inbound and outbound bandwidth on the Grace application Pods’ non-loopback network interfaces, including traffic to PostgreSQL and monitoring traffic. The chart covers five minutes and polls every 15 seconds. Each rate uses the last two available cAdvisor counter samples. **Pod activity** keeps the latest 100 observed creations and deletions for the app, Caddy and database Pods. The initial inventory is a baseline. Refreshing or reconnecting restores recent events from the dashboard, and the browser keeps notifications it has already received across dashboard restarts. Changes that occur entirely while the watcher is offline can be missed.

## Run the two scenes

Prepare a private JSON file on the presentation machine. It must contain an array of ten distinct test accounts that can already log in to the selected Grace installation. Each entry has `username` and `password` strings. Store the file outside Git and restrict it to its owner.

```bash
export GRACE_DEMO_ACCOUNTS=/path/to/private/accounts.json
GRACE_ORIGIN=http://localhost:8090
python3 scripts/dashboard-demo.py burst --url "$GRACE_ORIGIN"
```

On DOKS, set `GRACE_ORIGIN` to the public HTTPS origin. On minikube, forward `svc/caddy` to port 8090 as described below. Select a node in Explorer or on the topology map to filter its metrics. Use the toolbar to switch between Topology, Metrics and Readiness. The login inspector stays visible on desktop. Each timeline bar uses the start and end times recorded by the client. Hover over a bar to see its Pod and duration. The yellow dots show confirmed completions. The animation takes about one second so the audience can follow it. The bars retain the measured duration.

In another terminal, start the presenter's readiness probe. It samples every ten seconds.

```bash
python3 scripts/dashboard-demo.py probe --url "$GRACE_ORIGIN"
```

Run the second scene with the settings measured in the local rehearsal.

```bash
python3 scripts/dashboard-demo.py scale --url "$GRACE_ORIGIN" --seconds 120 --pause 0.1
python3 scripts/dashboard-demo.py burst --url "$GRACE_ORIGIN"
```

The load uses two login workers. Each worker logs out and pauses between logins. Ctrl+C stops scheduling work. Watch the HPA CPU target, replica count, new Pod state and endpoint arrows. A new Pod can receive a request after it joins the Service. Inspect the per-Pod counts after the second burst. The HPA can take several minutes to return to its minimum after load stops. Check errors with `burst --wrong-password`. The expected result is nine successful logins and one HTTP 401.

## Install on minikube or DOKS

Use a cluster that already runs the instrumented version of Grace from this checkout. Its image contains both the Grace and dashboard binaries. The monitoring install uses Helm 3, kubectl, jq, and an operator account with permission to create read roles. The existing application release remains a separate operation.

For a new local rehearsal, use the existing minikube application setup with a dedicated profile. The initial steps below prepare a fresh two-node profile.

```bash
CTX=grace-dash-demo
minikube start -p "$CTX" --driver=docker --nodes=2 --cpus=2 --memory=2200mb --keep-context --addons=metrics-server
docker build -t grace:local .
minikube -p "$CTX" image load grace:local
kubectl --context "$CTX" apply -f k8s/base/namespace.yaml
openssl rand -hex 24 | tr -d '\n' | kubectl --context "$CTX" -n grace create secret generic grace-db --from-file=password=/dev/stdin
kubectl --context "$CTX" apply -k k8s/minikube
kubectl --context "$CTX" apply -f k8s/monitoring/minikube-entry.yaml
kubectl --context "$CTX" -n grace rollout status deploy/app
```

On an existing installation, keep the current database Secret and PVC. The local Caddy deployment gives the presentation machine a route through the actual app Service. Run the following forward in a separate terminal during the rehearsal.

```bash
kubectl --context "$CTX" -n grace port-forward svc/caddy 8090:8090
```

Install the metrics sources and the dashboard. For Docker-backed minikube, add the second values file to include its overlay root filesystem.

```bash
helm upgrade --install grace-monitor prometheus --repo https://prometheus-community.github.io/helm-charts \
  --version 29.47.2 --kube-context "$CTX" -n grace \
  -f k8s/monitoring/prometheus-values.yaml -f k8s/monitoring/minikube-prometheus-values.yaml
bash scripts/dashboard-deploy.sh "$CTX"
```

On DOKS, the dev release job installs or updates Prometheus and the Dashboard after the Grace app passes its public health and business checks. A monitoring failure fails the job and leaves the verified learner app running. To install or retry monitoring independently, run the same command from the repository root with Helm 3, kubectl and jq:

```bash
CTX=do-tor1-grace
bash scripts/deploy-doks-monitoring.sh "$CTX"
```

The script waits for the app rollout and uses the released image and the registry pull-secret references from a Ready app Pod. The Dashboard uses its own read-only Kubernetes role. Installation checks topology readiness and current inbound and outbound network samples through the Dashboard Service. Allow two Prometheus scrapes after installation. If the traffic check fails, inspect the `kubernetes-nodes-cadvisor` scrape targets and the `container_network_receive_bytes_total` and `container_network_transmit_bytes_total` series for the Grace app Pods, then rerun the installer. The deployer needs permission to read `services/proxy` in the Grace namespace for these checks.

After installation, team members with Pod port-forward access can view the private ClusterIP Service from their own computer:

```bash
kubectl --context "$CTX" -n grace port-forward svc/grace-dashboard 19090:9090
```

Open **http://127.0.0.1:19090**. Keep the terminal open. Reconnect the forward when a release replaces the Dashboard Pod. Use the two-port Deployment forward in **Open the dashboard** when also sending login results or presenter probes.

For a scheduled DOKS rehearsal, run `bash scripts/dashboard-deploy.sh "$CTX" --enable-hpa-demo` and reconnect the Service forward on port 19090 after the rollout. Click **Start load test** on the same page. This enables bounded CPU load on the application Pods when the presenter clicks the button. Changing demo mode rolls the app Pods. After the rehearsal run `bash scripts/dashboard-deploy.sh "$CTX"` to return to monitoring-only mode. Subsequent dev releases preserve the current rehearsal setting. After a manual app rollback, rerun the installer to align the Dashboard image with Grace.
To link a specific existing DigitalOcean Uptime check, set `UPTIME_URL` on the dashboard Deployment to its DigitalOcean console URL.

## Read the signals

- **Topology.** Node boxes contain scheduled Grace workloads. Service arrows follow ready EndpointSlice targets. Dashed lines identify mounted claims. Select a node to filter CPU, memory, root disk and network charts. Unscheduled Pods have their own group.
- **Login results.** The dashboard joins random per-request IDs to structured completion events from the Grace Pods. Timeouts, missing Pod attribution and client results remain independent. Replayed log records count once. The dashboard stores the current batch and recent probe samples in memory.
- **Metrics.** Prometheus scrapes every 15 seconds. The default chart window is 15 minutes. Node, kube-state-metrics and kubelet series supply infrastructure metrics. Grace exposes answer counters on an internal metrics port. The application charts show submissions per minute, HTTP failure percentage and mean handler latency. Idle windows have no latency or failure percentage. The demo Prometheus keeps six hours of samples in ephemeral storage.
- **PostgreSQL storage.** The free-space and capacity series use kubelet statistics for the mounted `data-db-0` claim. The default minikube hostPath provisioner supplied no volume statistics in the rehearsal. The UI marks them unavailable. On DOKS, check both series against the bound volume before using them for alerting.
- **Outside the cluster.** The probe chart measures the presentation machine's route to Grace. DigitalOcean Uptime has its own probes and alert history. A full cluster outage also takes the in-cluster dashboard offline.

The [rehearsal evidence](evidence/) contains real request timings, HTTP statuses, Pod counts, HPA observations, metric ranges and screenshots from the local cluster.

The [learning workflow load test](evidence/practice-hpa.md) compares fixed replicas with HPA at the same request rate. Its chart aligns answer latency, workflow failures, available replicas and PostgreSQL activity by elapsed time.

The [higher-load comparison](evidence/practice-pressure.md) adds 60 and 120 scheduled workflows per second and tracks how many the ten clients could actually start and complete.
