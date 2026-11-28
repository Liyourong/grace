# Grace: Final Report

## Team Information

## Motivation

## Objectives

## Technical Stack

## Features

## User Guide

Grace helps you practice French with papers of multiple-choice, translation and sentence questions. Your answers and position are saved to your account.

### 1. Register and verify your email

1. Open Grace and choose **Sign up**.
2. Enter a username (3 to 32 letters, digits or underscores), your email and a password (8 to 128 characters), then choose **Send verification code**.

![Registration form](docs/images/02-register.png)

3. Enter the six-digit code from the email and choose **Verify and create account**. Each code allows five attempts. **Resend code** unlocks after 60 seconds and sends the same code again.

![Code entry](docs/images/03-verify-code.png)

If no email arrives, check your spam folder, wait a minute and choose **Resend code**. After five wrong codes, choose **Use a different email** and start again.

Your account starts with six starter papers. Sign in with your username and password. Usernames are not case-sensitive, so Alice and alice are the same account. Sessions last seven days, then Grace asks you to sign in again. Use the sign-out icon at the bottom of the sidebar to leave, and the monitor icon beside it to switch themes.

![Overview after registration](docs/images/04-overview.png)

### 2. Open or add a paper

- To open a paper, pick it under **My papers**, or choose **Start practicing** or **Continue** on the Overview.
- To add one, choose **Upload a paper**, pick or drop a `.json` file (1 MiB maximum), choose **Upload**, then **Start practicing**. The page shows the required format: JSON with `format_version` 1, a `title` and a list of `questions`, each with an `id`, `type`, `prompt` and answers. `examples/french-basics.json` makes a good template. Each upload creates a new paper.
- To organize papers, use **New folder** and the **⋯** menu beside a paper.

![Paper saved after upload](docs/images/06-uploaded.png)

### 3. Generate a paper with AI

Choose **Generate a paper** and describe the topic and level, such as "A2 French questions about travel". Pick 1 to 10 questions and optionally select question types; leave all types unchecked to let AI choose. Then choose **Generate with AI**, wait up to a minute, and open the result with **Open paper**. Generated papers appear under **My papers**.

![Generate form](docs/images/07-generate.png)

If generation is unavailable or busy, try again later or ask your DevOps contact.

### 4. Submit an answer

For multiple choice, click an option. For translation and sentence questions, type in **Your answer** and press Enter (Shift+Enter adds a line) or click the arrow button. Answers can be up to 2,000 characters. Translation and sentence answers are checked against the paper's accepted answers, so valid alternatives count. Grace immediately shows Correct or Incorrect, the reference answer and a short explanation.

![Graded answer with Explain button](docs/images/10-feedback.png)

You can edit and resubmit an answer; Grace warns about unsubmitted changes. Move with **Previous**, **Next** or the question numbers. **Finish** shows a summary, and you can review your answers afterwards.

### 5. Request an explanation

After an answer is graded, choose **Explain** for a short AI explanation. It appears under **Additional explanation** and never changes your grade. If you edited the answer, submit it first. If you see "Explanation is temporarily unavailable," choose **Retry Explain** later.

### 6. Resume on another device

Sign in with the same username and password. The Overview shows **Continue practice** with your latest paper, question number and answered count. Choose **Continue** to return to that question. The Overview also totals your study time, which counts only active practice, and answered questions.

![Overview on a second browser](docs/images/13-resume-overview.png)

Grace saves your position as you move between questions. If another device changed your progress, Grace asks you to review the latest position and try again.

### Features that require DevOps access

Only operator accounts see the **DevOps** sidebar link, which opens `/devops/` with cluster topology, metrics, readiness, logs and load tests. DevOps must also configure SendGrid for verification emails and an AI function for generation and explanations. Without them, those features report as unavailable.

## Development Guide

### Prerequisites

- Git, and Python 3 for the smoke test
- Docker with Compose v2 (easiest), or Rust 1.85+, Node.js 22+ and PostgreSQL 16+

### Repository layout

- `backend/`: Rust (Axum) API with `auth`, `papers` and `practice` modules, plus SQL migrations
- `frontend/`: React and Vite single-page app
- `functions/`: the AI function; `k8s/`: Kubernetes manifests; `tests/`: smoke and function tests

### Quick start (Docker Compose)

```sh
git clone https://github.com/Liyourong/grace.git && cd grace
docker compose up --build -d --wait
```

Open <http://localhost:8090> (host port 8090 maps to the container's 8080; edit `compose.yaml` if it is taken). The first build takes several minutes. After changing code, rerun the same command to rebuild.

### Without Docker

```sh
psql -U postgres -c "CREATE USER grace PASSWORD 'grace_local_only'" \
                 -c "CREATE DATABASE grace OWNER grace"
(cd frontend && npm ci && npm run build)
cd backend
DATABASE_URL=postgres://grace:grace_local_only@localhost:5432/grace \
STATIC_DIR=../frontend/dist APP_ORIGINS=http://localhost:8080 cargo run
```

Open <http://localhost:8080> in your browser. Grace serves the built frontend from disk, so after editing `frontend/` run `npm run build` and refresh; the backend needs no restart. The Vite dev server has no API proxy, so use the built app.

### Environment variables

| Variable | Purpose |
| --- | --- |
| `DATABASE_URL` | Required PostgreSQL connection string |
| `APP_ORIGINS` | Browser origins allowed to make changes; must match the address you open |
| `STATIC_DIR`, `PORT` | Built frontend folder (default `../frontend`) and port (default 8080) |
| `COOKIE_SECURE` | Set `true` only behind HTTPS |
| `POSTGRES_PASSWORD` | Compose only; default `grace_local_only` |
| `SENDGRID_API_KEY`, `SENDGRID_FROM_EMAIL` | Verification emails |
| `DEMO_EMAIL`, `DEMO_CODE` | One address may register once with this fixed code |
| `GRACE_EXPLAIN_URL`, `GRACE_EXPLAIN_TOKEN` | AI generation and explanations |
| `GRACE_TEST_PASSWORD_HASH` | Argon2 hash that creates operator account `grace_test` |

Compose reads these from your shell or an untracked `.env`. Never commit secrets.

### PostgreSQL and persistent storage

When it starts, Grace applies the SQL files in `backend/migrations` and creates its authentication tables, so an empty database needs no manual setup. Compose keeps data in the named volume `pgdata`: `docker compose down` preserves it, while `docker compose down -v` deletes it. Without Docker, data lives in your PostgreSQL data directory. Inspect it with `docker compose exec db psql -U grace -d grace`.

### External credentials

| Feature | Needs |
| --- | --- |
| Verification emails | SendGrid API key and verified sender; otherwise registration returns 503 |
| AI generation and explanations | HTTPS URL and token of the function in `functions/`, which holds an `OPENAI_API_KEY`; otherwise both report "unavailable" |

For local registration without email, set `DEMO_EMAIL` and `DEMO_CODE`, then register that address with the code. Sign-in, uploads, starter papers, grading and resuming need no credentials.

### Local tests

```sh
cd backend && cargo test --locked                  # Rust unit tests
cd frontend && npm ci && npm run lint && npm run build
node --test tests/generate-paper.cjs               # AI function contract
python3 tests/smoke.py http://localhost:8090       # Grace must be running
```

CI (`.github/workflows/grace.yml`) runs the same checks on every pull request to `dev`.

The smoke test checks the homepage, health endpoints and anonymous access (HTTP 401). Its signed-in checks run only in CI's temporary `grace-ci` and `grace-release` Compose projects.

### Confirm Grace started

1. `docker compose ps` shows `db` healthy and `app` running.
2. `docker compose logs app` ends with `grace listening on 0.0.0.0:8080`. On a first start it also lists `applied migration 0001_init` through `0007_devops`.
3. `curl -i localhost:8090/health/ready` returns `200`. `curl -i localhost:8090/api/auth/me` returns `401` while signed out.
4. The browser shows the **Welcome back** sign-in page.

Without Docker, use port 8080 and read the log in your terminal. Stop Grace with `docker compose down` (add `-v` only to delete data) or Ctrl+C.

If signing in or registering fails with "Request origin not allowed" (HTTP 403), `APP_ORIGINS` does not match the address in your browser.

## Deployment Information

The live application is at <https://146-190-191-107.sslip.io>.

Open it in a signed-out browser. The Grace "Welcome back" sign-in page should load. Grace runs on DigitalOcean Kubernetes and is redeployed on each push to `dev`.

## AI Assistance & Verification (Summary)

## Individual Contributions

## Lessons Learned and Concluding Remarks