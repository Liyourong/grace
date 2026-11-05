# Grace — Project Proposal

## 1. Motivation

Nowadays, self-paced study has become an increasing need among language learners. For a given language, such as French, one learner may have various goals such as preparing for a certification exams, communicating in the workplace, or learning native-like expressions. However, a pre-built question bank follow a generic curriculum and cannot cover all of these individual needs. Learners instead want to build practice material from class notes, textbooks, or their own error logs, which gives them control over the content. However, they are troubled by the work of organizing the questions, recording their answers, checking them against references, and tracking what they have mastered, which is inefficient for one-time self-made material. Giving this context, our project, Grace, aims to close the gap between "what I want to learn" and "what I can actually practice".

Our initial users are language learners with specific goals who want control over what they learn and practice. Grace will focus on three scenarios: exam preparation, classroom material consolidation, and targeting personal weak points. Users can upload their own papers, receive peer comments, and save their study progress. They can not only finish the practice papers we provide, but also create their own question sets and share them with other users. In long term, Grace can serve as a learning community for different roles, such as teachers, tutors, and study partiners who wish to prepare and share practice around specific teaching targets or shared goals.

Grace is built on the idea that practice is the most effective way to study when it matches the learner's actual needs, yet that match is difficult to achieve with fixed contents. Grace lets users turn notes, exam topics, and recurring mistakes into reusable exercises. Each submission returns immediate feeback, and the learner can revise and retry. Saved answer history and progress let users leave, resume and review at any time. Over time, one-off notes and error records grow into a personal question bank that adapts as the learner's goals change.

Pre-made question banks and learning apps offers convenient and structured practice,but the content is predetermined and cannot fully cover every learner's specific needs. Notebooks, documents, and spreadsheets offer full flexibility, but they provide no answer checking, feedback loop, or progress tracking. Grace aims to combine the strengths of both by offering a simple, focused workspace that supports multiple-choice, translation, and sentence-construction questions, where users bring their own content and the system handles delivery, feedback, and record-keeping. Uploaded materials are private by default, so learners can practice with personal data without exposing them unless they choose to share.

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
