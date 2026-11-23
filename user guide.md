# Run Grace on a new computer

Grace is a language-practice app. This guide starts with an empty machine and takes you through your first saved answer. Docker builds the React frontend and Rust API and runs them alongside PostgreSQL. You need internet access for the first download and build. You can complete the local walkthrough with a demo email address and a code you choose yourself.

## 1. Install Git and Docker

Choose the instructions for your operating system. All commands after this section run in a terminal.

| System | Setup |
| --- | --- |
| macOS | Open **Terminal**, run `xcode-select --install`, and finish the installer to get Git. Download Docker Desktop for your Mac's chip (Apple silicon or Intel), install it, and open it from Applications. [Docker for Mac](https://docs.docker.com/desktop/setup/install/mac-install/) |
| Windows | In **PowerShell as Administrator**, run `wsl --install -d Ubuntu`. Restart if prompted. Open Ubuntu and create your Linux user. Install Docker Desktop for Windows, start it, and enable Ubuntu under **Settings → Resources → WSL Integration**. In the Ubuntu terminal, run `sudo apt update && sudo apt install -y git curl`. Use that terminal for the rest of the guide. [Install WSL](https://learn.microsoft.com/en-us/windows/wsl/install) · [Docker WSL setup](https://docs.docker.com/desktop/features/wsl/) |
| Linux | Install Git and curl using your distribution's package manager. On Ubuntu or Debian, run `sudo apt update && sudo apt install -y git curl`. Install and start Docker Desktop for Linux, or install Docker Engine with the Compose plugin. [Docker installation options](https://docs.docker.com/get-started/get-docker/) |

Wait for Docker to finish starting. Run these checks in the terminal you will use for development:

```sh
git --version
docker compose version
docker info
```

The first two commands should print version numbers. Look for a **Server** section in the last command's output. If it reports a connection error, start Docker and try again. The container build supplies Rust, Node.js and PostgreSQL.

## 2. Download the project

Clone the team's development branch using HTTPS:

```sh
git clone --branch dev https://github.com/Liyourong/grace.git
cd grace
git branch --show-current
```

The last command should print `dev`. Run the remaining project commands from this directory, which contains `compose.yaml`. If GitHub prompts you to sign in, use an account with access to the [Grace repository](https://github.com/Liyourong/grace).

### Shared test login

Use **Sign in** with the learner username `grace_test` and the team’s test password on the configured local preview or the cloud site. Everyone using that login in the same environment shares its papers and progress. Local and cloud practice records are independent.

The account lives in PostgreSQL. App updates reuse its user ID, password, and saved work. Keep the database volume on redeploy, or restore its backup to recover progress. For a fresh installation, supply the team’s Argon2id hash before starting Grace. In Compose, set `GRACE_TEST_PASSWORD_HASH` in the ignored `.env` file, enclosing the full hash in single quotes to preserve the dollar signs. On Kubernetes, supply the `password-hash` key in the `grace-test-account` Secret in the app namespace. Startup creates the account only when the username is free. Keep shared test work separate from personal papers.

## 3. Configure your first local account

In the directory containing `compose.yaml`, create a plain-text file named **`.env`**. Put these two lines in it. Replace `YOUR_SIX_DIGITS` with six digits of your choice before saving.

```dotenv
DEMO_EMAIL=developer@example.test
DEMO_CODE=YOUR_SIX_DIGITS
```

Grace accepts this code during registration for exactly that email address and skips email delivery for it. You will choose your username and password in the browser. Keep `.env` local. Git already ignores it.

## 4. Start Grace and check readiness

```sh
docker compose up --build -d
docker compose ps
curl -i http://localhost:8090/health/ready
```

The first build downloads dependencies and compiles the app. In the service list, look for `db` marked **healthy** and `app` marked **Up**. The readiness request should return **HTTP 200** with an empty body. If the app has just started, wait a few seconds and retry. Open [Grace on your computer](http://localhost:8090/).

## 5. Register, submit an answer and verify it survives a restart

1. Click **Sign up**. Enter `developer@example.test`, a username of 3–32 letters, digits or underscores, and a password of 8–128 characters. Click **Send verification code**. Enter the six digits from your `.env` and click **Verify and create account**.
2. Your library starts with six papers at the root. Open **French Basics** in the sidebar. On the first question, select **Bonjour**. You should see a correct result immediately. Click **Next**. For a typed answer, use the submit button beside the input to save it. On the last question, **Finish** shows your results.
3. To try an upload, choose **Upload a paper**, select the [sample JSON file](examples/french-basics.json) from your clone, then click **Upload**. Each upload creates a separate paper. Click **Start practicing** in the upload result to open that copy.
4. Recreate the containers from the same project directory:

   ```sh
   docker compose down
   docker compose up -d
   ```

5. Wait for readiness, reload Grace, and sign in if prompted. Open the original paper and return to question one. Your submitted answer and the uploaded paper should still be there. Compose keeps them in the PostgreSQL named volume. Adding `-v` to `docker compose down` deletes that volume and its data.

For daily development, run `docker compose up --build -d` after editing source files. To stop for the day, run `docker compose down`.

### Learning overview

After sign-in, **Learning overview** shows your latest practice. Click **Continue** to reopen the saved question. On a new account, **Start practicing** opens French Basics. Select **Overview** in the sidebar to return at any time. On a phone, tap **My papers** to show the library.

**Questions answered** counts each answered question in your library once, including after a correction. **Study time** counts time spent on a question in the foreground. It pauses when you switch away or after two minutes without interaction. Overlapping practice on two devices counts once. Time tracking starts with this version. Removing a paper keeps your study time and removes its answers from the overview.

Select **Upload a paper** from the overview or during practice. Choose or drop a JSON file, then click **Upload**. The new paper appears at the root of **My papers**. Select **Start practicing** on the upload result to open it.

### Organize your library

After **French Basics**, the five starter papers are **Basic Sentences**, **Questions**, **Simple Subordinate Clauses**, **Relative Clauses**, and **Complex Sentences**. Each contains ten French multiple-choice questions with English instructions and explanations.

Click **New folder** beside **My papers**, enter a name, and click **Save**. In a paper’s **…** menu, choose **Move to…**, select the folder or **My papers (root)**, and click **Move paper**. Click a folder name to expand or collapse it. Moving a paper keeps your answers and position.

Use the folder’s **…** menu to rename or delete it. Deleting a folder moves its papers back to the root. **Delete paper** permanently removes the selected paper and its answers and progress after confirmation. This also applies to starter papers. A deleted starter stays deleted when you sign in again.

## 6. Enable external services when you need them

| Feature | Configuration and check |
| --- | --- |
| Registration with real email | Obtain a SendGrid API key with mail-send access and a sender address verified in SendGrid. Set `SENDGRID_API_KEY` and `SENDGRID_FROM_EMAIL` in `.env`. Register a different email address you can receive mail at, then enter the six-digit code from the message. |
| **Generate a paper** | Use the same Function URL and token as Explain. Select **Generate a paper**, enter a topic and level, choose 1–10 questions, and select any question types you want to practice. Leave all types unchecked for automatic selection. Then select **Generate with AI**. Select **Open paper** to start practicing. |
| **Explain** on a saved answer | Obtain the team's deployed DigitalOcean Function HTTPS URL and its matching token. Set `GRACE_EXPLAIN_URL` and `GRACE_EXPLAIN_TOKEN` in `.env`. Submit an answer, then click **Explain**. Grading and saving already work with the local setup above. |

After editing `.env`, run `docker compose up -d` to apply the settings.

## 7. Run the full dashboard locally with minikube

Start with Docker running and a clone of Grace (steps 1–2). You can go straight to this section if you only want the dashboard. You will run the app, PostgreSQL, Caddy, Prometheus and the dashboard in a two-node cluster on your computer. The commands target the local `grace-dash-demo` profile explicitly. No cloud account is required.

### Install the command-line tools

**macOS.** If `brew --version` fails, install [Homebrew](https://brew.sh/) and run the shell setup command printed by its installer. In Terminal, run:

```sh
brew install minikube kubectl helm@3 jq openssl@3 python
export PATH="$(brew --prefix helm@3)/bin:$PATH"
```

**Ubuntu, Debian or Windows with Ubuntu on WSL 2.** Use your Linux terminal. On Windows, check that `docker info` works **inside Ubuntu**. The following block installs the Linux binaries for Intel/AMD or ARM machines. [minikube downloads](https://minikube.sigs.k8s.io/docs/start/) · [kubectl installation](https://kubernetes.io/docs/tasks/tools/install-kubectl-linux/) · [Helm 3 installation](https://helm.sh/docs/v3/intro/install/)

```sh
(
  set -eu
  sudo apt-get update
  sudo apt-get install -y ca-certificates curl jq openssl python3
  case "$(uname -m)" in
    x86_64) arch=amd64 ;;
    aarch64|arm64) arch=arm64 ;;
    *) echo "Choose the binary for your CPU from the download pages above."; exit 1 ;;
  esac
  tmp=$(mktemp -d)
  cd "$tmp"
  curl -fsSLo minikube "https://github.com/kubernetes/minikube/releases/latest/download/minikube-linux-$arch"
  version=$(curl -fsSL https://dl.k8s.io/release/stable.txt)
  curl -fsSLO "https://dl.k8s.io/release/$version/bin/linux/$arch/kubectl"
  curl -fsSLo kubectl.sha256 "https://dl.k8s.io/release/$version/bin/linux/$arch/kubectl.sha256"
  echo "$(cat kubectl.sha256)  kubectl" | sha256sum --check
  archive="helm-v3.19.0-linux-$arch.tar.gz"
  curl -fsSLO "https://get.helm.sh/$archive"
  curl -fsSLO "https://get.helm.sh/$archive.sha256sum"
  sha256sum --check "$archive.sha256sum"
  tar -xzf "$archive"
  sudo install -m 0755 minikube kubectl "linux-$arch/helm" /usr/local/bin/
)
```

Check the tools in the same terminal. Use Helm **v3** and Python **3.10 or later** for the demo commands.

```sh
minikube version
kubectl version --client
helm version --short
jq --version
openssl version
python3 --version
```

### Create the cluster and deploy Grace

Use **Terminal A**, in the directory containing `compose.yaml`. If you ran the Compose walkthrough, run `docker compose down` there first to release port 8090. The cluster uses its own database. Give Docker room for two nodes at 2 CPUs and 2,200 MB per node, plus space for images and the 5 GiB database volume.

```sh
CTX=grace-dash-demo
minikube start -p "$CTX" --driver=docker --nodes=2 --cpus=2 --memory=2200mb --keep-context --addons=metrics-server
kubectl --context "$CTX" wait --for=condition=Ready nodes --all --timeout=5m
kubectl --context "$CTX" get nodes
docker build -t grace:local .
minikube -p "$CTX" image load grace:local
kubectl --context "$CTX" apply -f k8s/base/namespace.yaml
```

Expect two `Ready` nodes. For a **fresh profile**, create its database password and its local registration settings once. Choose six digits, replace `YOUR_SIX_DIGITS`, and keep the code for registration. On a later startup, reuse the existing Secrets and database volume.

```sh
openssl rand -hex 24 | tr -d '\n' | kubectl --context "$CTX" -n grace create secret generic grace-db --from-file=password=/dev/stdin
kubectl --context "$CTX" -n grace create secret generic grace-sendgrid \
  --from-literal=DEMO_EMAIL=developer@example.test \
  --from-literal=DEMO_CODE=YOUR_SIX_DIGITS
kubectl --context "$CTX" apply -k k8s/minikube
kubectl --context "$CTX" apply -f k8s/monitoring/minikube-entry.yaml
kubectl --context "$CTX" -n grace rollout status statefulset/db --timeout=5m
kubectl --context "$CTX" -n grace rollout status deployment/app --timeout=5m
kubectl --context "$CTX" -n grace rollout status deployment/caddy --timeout=5m
kubectl --context "$CTX" -n grace get pvc
```

Check that `data-db-0` shows `Bound`. Install Prometheus with both values files, then install the dashboard. Keep the profile name above so the installer enables the local HPA demo.

```sh
helm upgrade --install grace-monitor prometheus \
  --repo https://prometheus-community.github.io/helm-charts --version 29.47.2 \
  --kube-context "$CTX" -n grace --wait --timeout 10m \
  -f k8s/monitoring/prometheus-values.yaml \
  -f k8s/monitoring/minikube-prometheus-values.yaml
bash scripts/dashboard-deploy.sh "$CTX"
kubectl --context "$CTX" -n grace get deploy,hpa
```

The installer waits for the dashboard and current inbound and outbound samples. At idle, expect `app` at `2/2`, `grace-dashboard` at `1/1`, and the `app` HPA with a minimum of 2 and a maximum of 5. Initial CPU and bandwidth readings can take a few scrapes.

### Open the app and dashboard

Open two additional terminals. Leave each command running. The context is spelled out because new terminals do not inherit `CTX` from Terminal A.

| Terminal | Command | Open in your browser |
| --- | --- | --- |
| B — learner app through Caddy | `kubectl --context grace-dash-demo -n grace port-forward --address 127.0.0.1 svc/caddy 8090:8090` | [http://localhost:8090](http://localhost:8090/) |
| C — dashboard and demo input | `kubectl --context grace-dash-demo -n grace port-forward --address 127.0.0.1 deployment/grace-dashboard 9090:9090 9091:9091` | [http://127.0.0.1:9090](http://127.0.0.1:9090/) |

Use `localhost:8090` for the app. That is the origin accepted by the minikube overlay. Port 9091 accepts demo events from the local CLI. Register in the cluster app using `developer@example.test` and the six digits you set above. Open **French Basics** and submit one answer.

### Verify the dashboard

In Terminal A, check the actual data coming through the dashboard:

```sh
curl -fsS -o /dev/null -w 'Grace ready: %{http_code}\n' http://localhost:8090/health/ready
curl -fsS -o /dev/null -w 'Dashboard ready: %{http_code}\n' http://127.0.0.1:9090/health/ready
curl -fsS http://127.0.0.1:9090/api/snapshot | jq '{nodes:(.resources.nodes|keys), synced, replicas:.resources.deployments.app}'
curl -fsS http://127.0.0.1:9090/api/metrics | jq '.end as $end | [.series.podIn,.series.podOut] | map(any(.[]?; .values[-1][0] == $end))'
```

Both health checks should print `200`. The snapshot should list both nodes, all sync flags should be `true`, and the traffic check should print `[true, true]`. In **Topology**, locate the app Pods and the database volume. The **Live traffic** panel shows incoming and outgoing bandwidth. In **Metrics**, look for the submission you just made after the next scrape.

For **Readiness**, run the command below from the repository in Terminal A. Open the dashboard's **Readiness** tab and wait for a sample. Use Ctrl+C to stop the probe.

```sh
python3 scripts/dashboard-demo.py probe --url http://localhost:8090
```

For **autoscaling**, start the watcher below, then click **Topology → Start load test** in the local dashboard. Watch the replica count and the **Pod activity** panel as new Pods start. The test stops at five available replicas or after three minutes. Scale-down can take several more minutes. Use Ctrl+C to exit the watcher.

```sh
kubectl --context grace-dash-demo -n grace get hpa app --watch
```

### Optional: show ten concurrent logins

The login timeline uses real accounts. To prepare ten disposable accounts in this **local rehearsal database**, install the Argon2 CLI (`brew install argon2` on macOS or `sudo apt-get install -y argon2` on Ubuntu). Run the following once in Terminal A. It creates unique usernames and passwords, stores password hashes in PostgreSQL, and writes the login file outside the repository with owner-only permissions.

```sh
export GRACE_DEMO_ACCOUNTS="$(mktemp)"
python3 - <<'PY'
import json, os, secrets, subprocess

accounts = []
statements = ["BEGIN;"]
prefix = "demo_" + secrets.token_hex(4)
for i in range(10):
    name, password = f"{prefix}_{i}", secrets.token_hex(16)
    hashed = subprocess.check_output(
        ["argon2", secrets.token_hex(12), "-id", "-e"],
        input=password, text=True,
    ).strip()
    assert hashed.startswith("$argon2id$") and "'" not in hashed
    statements.append(
        f"WITH u AS (INSERT INTO users(username,password_hash) VALUES ('{name}','') RETURNING id) "
        f"INSERT INTO auth_credentials(user_id,username,password_hash) SELECT id,'{name}','{hashed}' FROM u;"
    )
    accounts.append({"username": name, "password": password})
statements.append("COMMIT;")
subprocess.run(
    ["kubectl", "--context", "grace-dash-demo", "-n", "grace", "exec", "-i", "db-0",
     "--", "psql", "-Xq", "-U", "grace", "-d", "grace", "-v", "ON_ERROR_STOP=1"],
    input="\n".join(statements), text=True, check=True,
)
with open(os.environ["GRACE_DEMO_ACCOUNTS"], "w") as f:
    json.dump(accounts, f)
PY
python3 scripts/dashboard-demo.py burst --url http://localhost:8090
```

Expect ten HTTP 200 results. Open the **Login burst** panel and look for ten requests with Pod attribution. To see a failed request, repeat the last command with `--wrong-password`. Expect nine successes and one HTTP 401. The CPU load test and login burst exercise different parts of the dashboard.

### Optional: inspect private Logs and Trace on your own cluster

Check whether your checkout includes `dashboard/logs.js`. The published `dev` revision used for this walkthrough includes the topology, metrics, readiness and login views. Private Logs requires a revision that also contains that file. If you already have a running profile and have just switched to such a checkout, update the local image before continuing:

```sh
docker build -t grace:local-traces .
minikube -p grace-dash-demo image load grace:local-traces
kubectl --context grace-dash-demo -n grace set image deployment/app grace=grace:local-traces
bash scripts/dashboard-deploy.sh grace-dash-demo
```

Use this block **once on your new local profile**. It generates a private presenter link in a temporary file and configures its verifier on the local dashboard. Keep that file until you finish the rehearsal.

```sh
(
  set -eu
  umask 077
  key=$(openssl rand -hex 32)
  presenter_file=$(mktemp)
  printf 'http://127.0.0.1:19090/#presenter=%s\n' "$key" > "$presenter_file"
  verifier=$(printf %s "$key" | openssl dgst -sha256 -r | cut -d' ' -f1)
  kubectl --context grace-dash-demo -n grace set env deployment/grace-dashboard GRACE_PRESENTER_SHA256="$verifier"
  kubectl --context grace-dash-demo -n grace rollout status deployment/grace-dashboard --timeout=3m
  printf 'Open the private URL stored in %s in your local rehearsal browser profile.\n' "$presenter_file"
)
```

The rollout replaces the dashboard Pod. Stop Terminal C's old forward, then run the command below there. Reserve port 19090 for this local forward. If you also present the cloud deployment, use a separate browser profile for the local key.

```sh
kubectl --context grace-dash-demo -n grace port-forward --address 127.0.0.1 deployment/grace-dashboard 19090:9090 9091:9091
```

Open the URL saved in the temporary file. Sign in and submit an answer at [the local cluster app](http://localhost:8090/), then open [local presenter Logs](http://127.0.0.1:19090/#logs-view). Select the latest `PUT …/answers/…` row to see the session check, question lookup, grading and database transaction. The private control tab on 19090 now also handles the load-test button. The public cloud viewer has a different data source.

### Stop, resume or diagnose the local cluster

Stop both port-forwards with Ctrl+C and run `minikube stop -p grace-dash-demo`. To resume, run `minikube start -p grace-dash-demo --keep-context`, wait for the app and dashboard Deployments to be ready, and restart the two forwards. The stopped profile keeps the database.

| Symptom | Local check |
| --- | --- |
| A Pod stays `Pending`, or shows an image error | Run `kubectl --context grace-dash-demo -n grace get pods -o wide` and `kubectl --context grace-dash-demo -n grace get events --sort-by=.lastTimestamp`. Check Docker resources, the database PVC and the `grace:local` image load. |
| HPA shows `<unknown>` | Run `kubectl --context grace-dash-demo top nodes`. If samples remain unavailable, check `kubectl --context grace-dash-demo -n kube-system rollout status deploy/metrics-server`. |
| Traffic stays blank | Check `helm status grace-monitor --kube-context grace-dash-demo -n grace`, allow two more scrapes, and repeat the traffic check. Confirm that Helm used both values files. |
| A forwarded URL stops responding | Read the error in Terminal B or C. If its Pod was replaced, rerun that terminal's port-forward command. |

## 8. Open the team's cloud dashboard

Anyone can open the [public dashboard](https://status.146-190-191-107.sslip.io/) to watch topology and Pod traffic. To inspect private request logs, get an authorized kubeconfig and the presenter link from the project maintainer. Confirm that `kubectl config get-contexts` lists `do-tor1-grace`, then run:

```sh
kubectl --context do-tor1-grace -n grace port-forward --address 127.0.0.1 svc/grace-dashboard 19090:9090
```

Keep that terminal open. Open the private presenter link once in the browser you will use. Submit an answer in the [cloud app](https://146-190-191-107.sslip.io/), open [private Logs](http://127.0.0.1:19090/#logs-view), and select the `PUT …/answers/…` row to see its trace. This dashboard shows requests sent to the cloud app. For an autoscaling demonstration, **Topology → Start load test** generates CPU load. A login or answer submission generates a business request trace.

## If a step fails

| Symptom | What to check |
| --- | --- |
| Docker says port 8090 is already allocated | Check `docker ps` and any terminal running a Kubernetes port-forward. Stop the process you own that uses 8090, then retry the startup command. |
| Readiness keeps failing | Run `docker compose ps` and `docker compose logs --tail=60 app db`. Check the database health and the app's first startup error. |
| Registration reports that email is unavailable | Use the exact demo address from `.env`. Confirm that the filename is `.env`, including the leading dot, and run `docker compose up -d` after edits. For a second account with real email, configure SendGrid. |
| Private Logs asks you to unlock | Open the presenter link in that browser profile. Chrome and the Codex in-app browser store their unlock state separately. |
