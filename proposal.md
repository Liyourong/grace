# Grace — Project Proposal

## 1. Motivation

## 2. Objectives, Architecture, and Key Features

## 3. Project Plan and Team Responsibilities

## 4. Initial Team Reasoning and AI-Assisted Exploration

### 4.1 Initial Team Reasoning

Our initial idea for Grace came from a practical limitation in existing language learning apps: learners must follow fixed lessons and cannot easily practice their own study materials. We want a stateful web app where users practice built-in or private custom language exercises (multiple-choice, translation, and sentence construction), get immediate feedback with explanations, and resume unfinished sessions after logging back in.

Before using AI for system design, we formed two technical hypotheses. First, to avoid spending time on a complex frontend editor, we defined a JSON schema so users could write exercises externally or ask an AI tool to format their notes into valid JSON for upload. Second, as our local MVP already ran on Docker Compose with PostgreSQL, we assumed Docker Swarm would be the easiest orchestration choice since it reuses Compose configuration.

We also listed three open questions to investigate:
1. Whether is more practical on DigitalOcean once we add persistent PostgreSQL storage, auto-scaling, and scheduled backups, Docker Swarm or Kubernetes?
2. What counts as a useful real-time feature, since normal answer grading is a standard HTTP response and does not require live server push?
3. How to share one user flow between a serverless function and an email service while still demonstrating them as two separate cloud features?

### 4.2 Exploration and Evaluation

We checked these assumptions through team discussion, the course handout, local MVP testing, and AI-assisted demos. As the handout grades backend architecture and deployment rather than UI complexity, we kept the JSON schema upload approach. Testing our local Docker Compose + PostgreSQL setup confirmed that a mounted volume keeps user records across container restarts, and the handout clarified that managed databases do not count as implementing persistent storage.

For orchestration, we asked AI to build small demos for both Docker Swarm and Kubernetes and compared how they handle our planned features:

```mermaid
flowchart LR
  H["Initial Idea: Docker Swarm"] --> D["AI Demos: Swarm vs. K8s"]
  D --> E["Check: PVC, HPA & CronJob"]
  E --> K["Decision: Kubernetes (DOKS)"]
```

| Dimension | AI Demo A: Docker Swarm | AI Demo B: Kubernetes (`minikube` / DOKS) |
| :--- | :--- | :--- |
| **Initial Setup** | Directly reuses `docker-compose.yml` | More YAML upfront (`Deployment`, `Service`, `PVC`) |
| **PostgreSQL Storage** | Must pin DB to one node (`node.role == manager`) or a rescheduled container mounts an empty local disk | Reattaches volumes across pod restarts using `do-block-storage` CSI and `PersistentVolumeClaims` |
| **Auto-Scaling** | No built-in auto-scaler; needs custom scripts calling `docker service scale` | Built-in CPU/memory scaling via `HorizontalPodAutoscaler` |
| **Backups** | Host crontab tied to one machine | Cluster `CronJob` and `Secret` running `pg_dump` |

Although AI highlighted Swarm's quick setup, comparing both demos showed that Swarm creates maintenance issues for volume failover and scaling later on. We dropped the Swarm idea and chose Kubernetes (`minikube` locally, then DigitalOcean Kubernetes) because the extra YAML setup upfront saves us from writing custom workarounds for our six planned advanced features.
