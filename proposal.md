# Grace — Project Proposal

## 1. Motivation

## 2. Objectives, Architecture, and Key Features

## 3. Project Plan and Team Responsibilities

Starting from our existing MVP, we will develop Grace through short, overlapping iterations. We will verify existing workflows, standardize Docker Compose, and agree on a prioritized backlog, module boundaries, shared models, and API contracts. Feature development, integration, testing, and cloud preparation will progress in parallel. Friday meetings will review progress and blockers, with targets for core functionality on November 6, integration and joint debugging on November 13, and design finalization on November 20. We will prepare slides and an integrated demonstration by November 24 for our November 25 presentation, then complete stabilization, documentation, final testing, and the video demo. Each iteration will include planning, implementation, testing, an integrated demonstration, and backlog adjustment.

Liyourong (Liyourong Wang) will lead architecture and integration, maintain the shared application backbone, and coordinate schemas, API contracts, and cross-module decisions. chenmw215 (Minwen Chen) will own accounts, authentication, and access control. FancyMelon (Yiyun Mao) will own validation, upload, storage, and management of custom questions and papers. duckyay (Xinyu Liu) will own practice interactions, answer evaluation, attempt history, and progress recovery. Each feature owner will implement related UI and backend logic, write module tests, and resolve integration issues. We will assign owners for infrastructure, monitoring, and advanced features during iteration planning.

Paper management and practice depend on authenticated identity, while practice also depends on question schemas and paper retrieval. We will agree on request/response formats, error conventions, and migration procedures, using shared sample data or mock APIs until real interfaces are available. Developers will work on small task branches from `dev`, submit PRs for review, and run automated build and test checks. Testing will progress from Docker Compose to minikube and DigitalOcean Kubernetes, covering registration and login, private uploads, answer feedback, attempt history, logout, progress recovery, and cross-user data isolation. Cloud checks will cover PostgreSQL persistence across restarts, monitoring, scaling, security, backup restoration, and email delivery. Validated releases will be promoted to `prod` through release PRs with manual approval.

Changing interfaces, migration conflicts, delayed cloud setup, and reliance on the integration lead could block progress. Early contract reviews, coordinated migrations, frequent integration demos, prompt blocker reporting, and early cloud validation will address these risks. Feature owners will coordinate routine integration directly, with shared-model and architecture changes reviewed by the team.

## 4. Initial Team Reasoning and AI-Assisted Exploration

### 4.1 Initial Team Reasoning

### 4.2 Exploration and Evaluation
