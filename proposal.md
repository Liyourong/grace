# Grace — Project Proposal

## 1. Motivation

Nowadays, self-paced study has become an increasing need among language learners. For a given language, such as French, one learner may have various goals such as preparing for a certification exams, communicating in the workplace, or learning native-like expressions. However, a pre-built question bank follow a generic curriculum and cannot cover all of these individual needs. Learners instead want to build practice material from class notes, textbooks, or their own error logs, which gives them control over the content. However, they are troubled by the work of organizing the questions, recording their answers, checking them against references, and tracking what they have mastered, which is inefficient for one-time self-made material. Giving this context, our project, Grace, aims to close the gap between "what I want to learn" and "what I can actually practice".

Our initial users are language learners with specific goals who want control over what they learn and practice. Grace will focus on three scenarios: exam preparation, classroom material consolidation, and targeting personal weak points. Users can upload their own papers, receive peer comments, and save their study progress. They can not only finish the practice papers we provide, but also create their own question sets and share them with other users. In long term, Grace can serve as a learning community for different roles, such as teachers, tutors, and study partiners who wish to prepare and share practice around specific teaching targets or shared goals.

Grace is built on the idea that practice is the most effective way to study when it matches the learner's actual needs, yet that match is difficult to achieve with fixed contents. Grace lets users turn notes, exam topics, and recurring mistakes into reusable exercises. Each submission returns immediate feeback, and the learner can revise and retry. Saved answer history and progress let users leave, resume and review at any time. Over time, one-off notes and error records grow into a personal question bank that adapts as the learner's goals change.

Pre-made question banks and learning apps offers convenient and structured practice,but the content is predetermined and cannot fully cover every learner's specific needs. Notebooks, documents, and spreadsheets offer full flexibility, but they provide no answer checking, feedback loop, or progress tracking. Grace aims to combine the strengths of both by offering a simple, focused workspace that supports multiple-choice, translation, and sentence-construction questions, where users bring their own content and the system handles delivery, feedback, and record-keeping. Uploaded materials are private by default, so learners can practice with personal data without exposing them unless they choose to share.

## 2. Objectives, Architecture, and Key Features

### Main Objectives and Core Functionality

The main objective is to give language learners control over what they practise by turning their own materials into customised exercises with immediate feedback and persistent progress tracking.

The core functionalities include user registration and authentication, custom JSON paper uploads, immediate grading and feedback, and persistent storage of answers and progress for resuming practice across sessions and devices.

### Assumptions
The whole project is under the assumption that users access the service from any location with internet access through a web browser and they are geographically concentrated. The initial capacity limit is 100 registered users and 20 concurrently active learners. 

### Planned system architecture and rationales

As shown in Figure 1 below, users primarily interact with the Web Frontend, sending requests through the HTTPS Gateway to the Orchestrator for routing to application modules. The Authenticator verifies user identity; the Paper Manager calls the Paper Validator to check uploaded papers; and the Grader evaluates submitted answers while the Progress Tracker records the latest answers. These modules share a PostgreSQL database allowing users to resume their work across sessions and devices.

![Grace system architecture: browser frontend, HTTPS gateway, application modules, PostgreSQL, and persistent storage](assets/grace-program-diagram.png)

*Figure 1. Grace system architecture.*

The yellow `Local` area represents browser-based interaction occurring on users' devices, while the blue `Kubernetes` area contains the server-side workloads for keeping authoritative state on the server. The gateway, application, and PostgreSQL run in separate `Pods` because their respective responsibilities. This separation allows application replicas to scale without coupling lifecycles to the database.

Within the `Pod Scalable` boundary, the business logic is organised into separate functional modules. This reduces integration and deployment overhead at the cost of releasing and scaling all modules together.

All application replicas share PostgreSQL-backed sessions and practice records, allowing requests to reach  replicas without relying on replica-local state. PostgreSQL therefore uses a Persistent Volume whose lifecycle is separate from the database Pod, allowing replacement Pods to reuse existing records. This supports recovery from Pod replacement.

### Alternative Design and Tradeoffs

The team initially considered using Fly.io's higher-level application deployment model, which is known for its integrated multi-region deployment and easy management. 

However, such choice offers limited benefit under the assumption of a small, geographically concentrated user base. The selected Kubernetes design prioritises explicit control over deployment, storage, and scaling policies, while trading-off greater configuration and integration complexity for Fly.io's higher-level deployment model requires. A Kubernetes-based approach may also expose the system to the risk of exhausting shared database capacity if user demand grows unexpectedly..

However, under the initial workload assumptions, these risks are considered manageable through staged deployment testing. The choice therefore accepts additional operational responsibility in exchange for deployment control.


### Required course technologies

- `Docker and Docker Compose`: Docker will package the application and PostgreSQL as separate containers. Docker Compose will provide a consistent local development and integration-testing environment.

- `PostgreSQL and persistent storage`: PostgreSQL will persist accounts, sessions, papers, questions, answers, and positions. Local named volumes and Kubernetes PersistentVolumeClaims keep database files independent of container replacement.

- `Deployment provider`: DigitalOcean provides cloud platform, integrating managed cluster infrastructure with provider networking, storage, and monitoring.

- `Orchestration`: Kubernetes was selected over Docker Swarm because HPA, PVCs, and CronJobs provide a consistent way to configure autoscaling, persistent storage, and scheduled backups. This reduces custom operational scripting at the cost of more initial configuration. See Section 4.2, Exploration and Evaluation, for the detailed comparison.

- `Monitoring and observability`: DigitalOcean resource metrics and alerts, kube-state-metrics, external HTTPS availability checks, and structured logs will expose resource pressure and application failures. Liveness and readiness endpoints distinguish a running process from an application able to reach its database.

### Advanced features and integration

- `Auto-scaling and high availability`: The team will use Kubernetes horizontal pod autoscaling and multiple application replicas to handle changing demand and keep the application available when an application Pod fails. This provides automatic scaling and application-level redundancy.

- `Security enhancements`: The team will implement HTTPS, user-level access controls, and deployment secret management to protect data in transit, prevent unauthorised access, and keep credentials outside the code repository.

- `CI/CD pipeline`: The team will automate tests, container builds, and deployment steps, with manual approval for production releases. This creates a repeatable integration and delivery process for team contributions.

- `Backup and recovery`: The team will schedule PostgreSQL backups to separate storage and test database restoration. This provides a verified recovery path beyond the protection offered by persistent volumes.

- `Integration with external services`: The team will integrate an external email provider's API so users can request their practice results by email. This connects the application's saved results to a third-party delivery service.

### Scope and feasibility

The scope of the project is a single-region, stateful language-practice service for an initially small, geographically concentrated user base. Delivery covers the complete workflow from account creation and custom paper upload to grading and saved-progress recovery, supported by a monitored Kubernetes deployment with persistent PostgreSQL storage. 

The feasibility of the project is supported by the following:

- The team has completed an MVP that provides a foundation for accounts, paper uploads, practice, database persistence, and deployment.

- Three feature owners can develop accounts, papers, and practice in parallel, while the system architect establishes the shared framework, API contracts, and data structures to reduce blocking dependencies.

- Scaling, monitoring, security, CI/CD, and backups extend the same application and deployment foundation, limiting additional implementation overhead while requiring separate validation of each capability.

- Weekly Friday team meetings will support coordination, with core functionality targeted for November 6, integration and joint debugging for November 13, and design finalisation for November 20. Further details appear in Section 3, Project Plan and Team Responsibilities.

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
