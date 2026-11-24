# Grace presentation demo

Run the live demo on **DigitalOcean Kubernetes (DOKS)**. Keep **Minikube** signed in and ready in a second browser window. The presenter cues each switch from the slides. The developer controls the browser and terminal. Budget about two minutes of the six-minute presentation for the core demo, plus 15 seconds for Explain on its slide.

## Set up cloud access on the developer’s laptop

Save the supplied file as `~/Downloads/token.txt`. It contains the DigitalOcean API token on one line. The Grace website uses the separate `grace_test` login.

**Prepare one cloud terminal.** Install `doctl` and `kubectl` if needed. On macOS with Homebrew, run `brew install doctl kubectl`. Run the block below in Bash or Zsh. It reads the token from the file and downloads a two-hour cluster credential. Keep this terminal for the demo. If the credential expires between rehearsal and class, run the block again.

```bash
export KUBECONFIG="$HOME/.kube/grace-class-demo.yaml"
(
  set -e
  umask 077
  mkdir -p "$HOME/.kube"
  DIGITALOCEAN_ACCESS_TOKEN=$(tr -d '\r\n' < "$HOME/Downloads/token.txt")
  test -n "$DIGITALOCEAN_ACCESS_TOKEN"
  export DIGITALOCEAN_ACCESS_TOKEN
  demo_config=$(mktemp "$KUBECONFIG.XXXXXX")
  trap 'rm -f "$demo_config"' EXIT
  doctl kubernetes cluster kubeconfig show grace --expiry-seconds 7200 > "$demo_config"
  mv "$demo_config" "$KUBECONFIG"
) && kubectl config get-contexts
```

Expect `do-tor1-grace`. If the downloaded context has a different name, run `kubectl config rename-context "$(kubectl config current-context)" do-tor1-grace` in this cloud terminal. Keep Minikube on the computer that hosts the existing backup profile, in its own terminal with its usual kubeconfig. [DigitalOcean kubeconfig command](https://docs.digitalocean.com/reference/doctl/reference/kubernetes/cluster/kubeconfig/show/)

**Developer · test access before class.** These checks cover the two commands in **Readiness → Database persistence**. Each answer should be `yes`. Then run **Read the saved answer** once against DOKS to confirm the actual SQL connection. Save the restart for the demo.

```bash
kubectl --context do-tor1-grace -n grace auth can-i get pods/db-0
for verb in get create; do
  kubectl --context do-tor1-grace -n grace auth can-i "$verb" pods/db-0 --subresource=exec
done
for verb in get patch list watch; do
  kubectl --context do-tor1-grace -n grace auth can-i "$verb" statefulsets.apps/db
done
```

If a check returns `no`, send the failed command and its result to the cluster admin.

<details>
<summary>Cluster admin only · grant missing demo permissions</summary>

 Have the developer run `kubectl --context do-tor1-grace auth whoami -o jsonpath='{.status.userInfo.username}{"\n"}'` and send you the username. From your **admin terminal**, substitute that exact username below and apply the namespace Role and RoleBinding. Ask the developer to repeat the checks. Kubernetes permissions are additive; this binding grants the listed access and preserves any roles the identity already has. [DOKS identity and role mapping](https://docs.digitalocean.com/products/kubernetes/reference/api/)

```bash
DEMO_USER='replace-with-the-developer-kubernetes-username'
kubectl --context do-tor1-grace -n grace apply -f - <<'YAML'
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata: {name: grace-class-db-demo, namespace: grace}
rules:
  - apiGroups: [""]
    resources: ["pods"]
    resourceNames: ["db-0"]
    verbs: ["get"]
  - apiGroups: [""]
    resources: ["pods/exec"]
    resourceNames: ["db-0"]
    verbs: ["get", "create"]
  - apiGroups: ["apps"]
    resources: ["statefulsets"]
    resourceNames: ["db"]
    verbs: ["get", "patch", "list", "watch"]
YAML
kubectl --context do-tor1-grace -n grace create rolebinding grace-class-db-presenter \
  --role=grace-class-db-demo --user="$DEMO_USER" --dry-run=client -o yaml |
  kubectl --context do-tor1-grace -n grace apply -f -
```

If you created the demo binding, remove it after class with `kubectl --context do-tor1-grace -n grace delete rolebinding grace-class-db-presenter`.

</details>

## Before class

| Open and keep ready | Cloud · primary | Minikube · backup |
|---|---|---|
| Practice tab | [Grace on DOKS](https://146-190-191-107.sslip.io/) | [Grace on Minikube](http://localhost:18090/) |
| Operations tab | [Cloud DevOps](https://146-190-191-107.sslip.io/devops/#topology-view) | [Local DevOps](http://localhost:18090/devops/#topology-view) |
| Terminal target | `--context do-tor1-grace -n grace` | `--context grace-dash-demo -n grace` |

1. On the **Minikube backup computer**, check that Docker is running. Run `minikube status -p grace-dash-demo`. If the existing profile is stopped, run `minikube start -p grace-dash-demo --keep-context`. Open the local links on that computer. If the local app link is offline, run the following in its own terminal and leave it open.

   ```bash
   kubectl --context grace-dash-demo -n grace port-forward svc/caddy 18090:8090
   ```

2. Sign in to **both** sites as `grace_test`. In each practice tab, open **My papers → French Relative Pronouns · Demo → question 1**. Check that the Zhengzhou sentence and all four choices are visible. The two sites keep separate answers. Ask the team to leave this shared account free during the demo.
3. In each Operations tab, open **Readiness → Database persistence**. Select **DOKS** in the cloud window and **Minikube** in the backup window. Check the context printed in both command boxes. The environment selector changes the commands you copy. The dashboard itself continues to show the cluster that serves its URL.
4. In **Topology**, check for two ready app replicas, a numeric **Application CPU** reading, a **70%** target, and an enabled **Start load test** button. Leave the cloud Operations tab here. If the slide links to a local dashboard, switch to the prepared **cloud** tab when presenting.
5. In the **backup terminal**, verify local restart access and check that the database Pod is pinned to its data node. These checks are read-only.

   ```bash
   kubectl --context grace-dash-demo -n grace auth can-i patch statefulsets.apps/db
   kubectl --context grace-dash-demo -n grace get sts db -o jsonpath='{.spec.template.spec.nodeSelector.kubernetes\.io/hostname}{"\n"}'
   kubectl --context grace-dash-demo -n grace get pod db-0 -o wide
   ```

   Expect `yes`. For this backup profile, the selector and the Pod’s **NODE** should both be `grace-dash-demo-m02`. Its PostgreSQL volume uses a node-local directory. If the selector is empty or points elsewhere, resolve that before rehearsing a local database restart.

## At the “Live Demo” slide

Start with the **cloud** window. Keep the same environment for the answer, SQL query, restart and final refresh.

| Cue | Developer action | What the audience should see |
|---|---|---|
| **1 · Submit the answer** | In question 1, click **où**. Pause on the grading result. Switch to **DevOps → Topology** and point to an `app-…` Pod and `db-0`. | Grace grades one real submission. The application and PostgreSQL run in separate containers. The next query shows their connection. |
| **2 · Read the database** | Open **Readiness → Database persistence**. Check **DOKS**. Copy **Read the saved answer** into the terminal and run it. Leave the output visible. | PostgreSQL returns `db_started`, `question_id` and `saved_answer`. The stored choice ID is `ou`; Grace displays **où**. |
| **3 · Restart and read again** | Copy **Restart PostgreSQL** and run it. The cloud database briefly goes offline. Wait for the rollout to complete. Run the **first query** again. Then refresh Grace and reopen question 1. | The database start time changes. The question ID and answer match the first row. Grace still shows **où** selected after a fresh read. |
| **4 · Start scaling** | In **Topology**, click **Start load test** once. Point to the CPU reading and its 70% target. While the cluster samples CPU, open **Logs**, select the latest **Submit answer**, and show its status and app Pod. Return to **Topology**. | The log ties the earlier click to a specific app Pod. The CPU percentage measures average use relative to the app Pods’ requested CPU. |
| **5 · Show the scale-up** | Point to the amber CPU warning, the HPA’s requested replica count, then **Pod activity → Created** and the ready replica count. Click **Stop load test** if it is still running when you finish. | The warning marks the first sampled value above target. The HPA request, Pod creation and Pod readiness are separate observations. Report the counts actually on screen. |

If scaling is still in progress, return to the slides and revisit **Topology** at the **Observability** slide. The test stops automatically when five replicas are available or after three minutes. On **Metrics**, **Answer submissions** shows the submission rate; **Answer latency · mean** shows how long the handler takes. Use **Logs** to identify the individual request.

## At the “Explain” slide

Return to the cloud practice tab, question 1. Click **Explain** under the graded answer. Wait for **Additional explanation**, then point to why *où* refers back to *la ville*. The Rust backend loads the saved question and answer and calls the serverless function. If the function is slow or unavailable, use the result on the slide and continue. Use the prepared slides for the SendGrid email and GitHub Actions deployment evidence.

## If the cloud demo stalls

- **Cloud app or DevOps unavailable.** Switch once to the prepared Minikube window. Say, “We’ll continue on our backup Kubernetes cluster.” Click **où** there before querying. Use **Minikube** in Readiness and check `--context grace-dash-demo` on the command you paste.
- **Cloud database restart already started.** Leave its rollout terminal open. Use a separate terminal for Minikube. Show a complete before-and-after pair from the same cluster.
- **Only the HPA or Explain result is late.** Continue with the next slide. A recorded [HPA rehearsal](dashboard/evidence/hpa-button.png) and the Explain slide provide fallbacks. Identify rehearsal evidence as recorded when you show it.

After the presentation, stop any active load test and leave the test account’s paper and answers in place. On a borrowed laptop, sign out of Grace and delete the supplied `~/Downloads/token.txt` and the dedicated `~/.kube/grace-class-demo.yaml`.
