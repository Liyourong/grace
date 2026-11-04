# Project Requirements

## Core Technical Requirements

### Containerization and Local Development (Required for All Projects)

- Use **Docker** to containerize the application (e.g., Node.js backend, database).
- Use **Docker Compose** for a multi-container setup (e.g., app + database).

### State Management (Required for All Projects)

- Use **PostgreSQL** for relational data persistence.
- Implement **persistent storage** (e.g., [DigitalOcean Volumes](https://docs.digitalocean.com/products/volumes/) or [Fly Volumes](https://fly.io/docs/volumes/)) to ensure state survives container restarts or redeployments.

#### Note on Managed Databases

You may explore managed database services (e.g., DigitalOcean Managed PostgreSQL) if you wish. However, because managed services handle storage for you, they do **not** count as implementing persistent storage for the purposes of this project.

Your system must still **demonstrate persistent storage implementation** (e.g., using Kubernetes PersistentVolumes or DigitalOcean/Fly volumes). The goal is to help you understand how stateful storage is implemented and managed in containerized environments.

### Deployment Provider (Required for All Projects)

- Deploy to either **DigitalOcean** (IaaS focus) or **Fly.io** (edge/PaaS focus).

### Orchestration Approach (Choose One)

#### Option A: Docker Swarm Mode

- Use Docker Swarm mode for clustering and orchestration.
- Implement service replication and load balancing.

#### Option B: Kubernetes

- Use Kubernetes for orchestration (start with minikube locally, then deploy to a cloud-managed cluster, e.g., [DigitalOcean Kubernetes](https://www.digitalocean.com/products/kubernetes)).
- Include Deployments, Services, and PersistentVolumeClaims for stateful data.

#### Note on Orchestration on Fly.io

Using Fly.io’s built-in scaling, replication, or global distribution features alone does **not** satisfy the orchestration requirement, because these are provider-level capabilities rather than an implementation of Docker Swarm or Kubernetes.

[Fly Kubernetes Service (FKS)](https://fly.io/docs/kubernetes/fks-quickstart/) is **not** required. It is a paid beta feature ($75/month per cluster) and may have limited documentation or bugs. You may use it if you wish, but it is not recommended.

For Kubernetes, please use minikube locally plus a managed Kubernetes service — DigitalOcean Kubernetes — or Docker Swarm on Fly.io/DigitalOcean. These options fully satisfy the course requirements and avoid unnecessary cost or complexity.

### Monitoring and Observability (Required for All Projects)

- Integrate monitoring using provider tools (e.g., DigitalOcean metrics/alerts for CPU, memory, disk; Fly.io logs/metrics).
- Set up basic alerts or dashboards for key metrics.
- **Optional:** You may also integrate metrics/logs/traces into your frontend if you have one to make the demo clearer.

### Note on Frontend (UI)

A frontend is **not required**, but you may include a simple web interface (e.g., built with [Next.js](https://nextjs.org/) or another framework) to make your demo and presentation clearer. The grading focus is on your backend, architecture, and deployment, not UI complexity.

## Advanced Features (Must Implement at Least Two)

- **Serverless integration:** e.g., DigitalOcean Functions or Fly.io for event-driven tasks like notifications.
- **Real-time functionality:** e.g., WebSockets for live updates.
- **Auto-scaling and high availability:** e.g., configure Swarm/K8s to scale based on load.
- **Security enhancements:** e.g., authentication/authorization, HTTPS, secrets management.
- **CI/CD pipeline:** e.g., GitHub Actions for automated builds/deployments.
- **Backup and recovery:** e.g., automated database backups to cloud storage.
- **Integration with external services:** e.g., email notifications via SendGrid.
- **Edge-specific optimizations:** e.g., global distribution on Fly.io with region-based routing.
