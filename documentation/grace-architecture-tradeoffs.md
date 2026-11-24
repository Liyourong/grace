# Grace 架构取舍：十个答辩问题

基于 Grace v16（`dev` 提交 `caeefc3`）。第 2 题有团队比较两套原型的记录，第 8 题有本地对照实验。其余回答根据课程要求和实际代码解释方案取舍。有关价格、延迟和吞吐量的结论，只引用项目实际测过的数据。

## 01　课程允许 DigitalOcean 和 Fly.io，Grace 为什么选 DigitalOcean？

**口答**　我们选择了 Kubernetes 这条课程路线。DigitalOcean Kubernetes（DOKS）托管控制平面，又能让我们把本地 minikube 使用的 Deployment、Service 和持久卷配置带到云端。我们可以现场展示同一套机制如何分发请求、替换 Pod 和重新挂载数据卷。Fly.io 也是课程允许的平台，但如果只用 Fly Machines 的复制和扩容，还要另外实现课程要求的 Swarm 或 Kubernetes 编排。对 Grace 来说，DOKS 与我们要交付的架构更匹配。[课程要求](../requirements/requirements.md) · [团队提案](../proposal.md) · [DOKS 文档](https://docs.digitalocean.com/products/kubernetes/)

**什么时候会改选**　如果目标改成让无状态应用靠近多个地区的用户，同时取消指定编排的要求，我会重新评估 Fly Machines。它允许按地区放置实例。但 Grace 的答题写入要到 PostgreSQL，应用部署得更近，未必能缩短整笔提交。选平台前应测各地区的端到端提交延迟。[Fly Machines 文档](https://docs.fly.io/machines/overview) · [Grace 的答题事务](../backend/src/practice/mod.rs)

## 02　既然本地已经用 Docker Compose，为什么上云时选 Kubernetes，没选 Docker Swarm？

**口答**　Swarm 是我们最早看好的方案，因为团队已经熟悉 Compose。真正改变选择的是三个具体问题：数据库换节点后怎样取得原来的数据、负载上升时谁来增加副本、每天的备份由谁调度。团队的 Swarm 原型用了节点本地卷，数据库需要固定节点，扩容和备份还要另外接脚本。Kubernetes 用 PVC、HPA 和 CronJob 分别处理这些工作。我们接受更多 YAML 和排障成本，换取更少的自建运维逻辑。[团队原型对照](../proposal.md#42-exploration-and-evaluation) · [HPA 配置](../k8s/base/hpa.yaml) · [备份调度](../k8s/doks/backup.yaml)

**什么时候会改选**　如果应用长期只需固定副本，数据库已由外部服务管理，而且团队已有可靠的调度和监控，那么 Swarm 更有吸引力。这里影响决定的是团队原型需要补齐的能力。使用其他存储驱动的 Swarm 部署可以有不同的结果。[Docker Swarm 服务文档](https://docs.docker.com/engine/swarm/services/)

## 03　Rust Pod 本来就能发 HTTPS 请求，为什么还要经过 Serverless Function 调用 OpenAI？

**口答**　出题和 Explain 都由用户按需触发，两者共用一套模型凭据和调用逻辑。我们把这部分放进 DigitalOcean Function，让 OpenAI 密钥只进入 Function。Rust 仍负责用户身份、读取已交答案、校验生成的试卷和保存结果。这样的边界允许独立更新模型调用。代价是多一次网络往返、多一个部署点，还要协调两层超时。例如 Explain 的 Rust 调用只等 12 秒，Function 给模型留了 24 秒，模型在第 15 秒返回时用户已经收到失败。[Function 配置](../functions/project.yml) · [模型调用](../functions/packages/grace/explain-answer/index.js) · [Explain 超时](../backend/src/practice/explain.rs) · [出题校验](../backend/src/papers/generate.rs)

**什么时候会改选**　如果模型调用长期稳定、由同一团队随应用一起发布，而且实测额外一跳影响体验，我会考虑让 Rust Pod 直接调用 OpenAI，通过 Secret 注入密钥。两种方案都能让密钥留在服务端。当前 Function 路径也仍要让用户等待模型返回，它本身不会把长请求变成后台任务。[Rust 调用路径](../backend/src/cloud.rs) · [DigitalOcean Functions 文档](https://docs.digitalocean.com/products/functions/details/features/)

## 04　PostgreSQL 可以交给云厂商管理，为什么 Grace 自己在 Kubernetes 里运行它？

**口答**　课程要求我们亲自实现并展示持久存储。用 StatefulSet 和 PVC，我们能让学生先保存一道答案，更换数据库 Pod，再让新进程挂载原卷，读回同一个题号和答案。这个选择把存储行为变成可观察的演示。代价也很具体：当前 PostgreSQL 是单实例，重启时答题会暂时不可用，备份和恢复演练由团队负责。[课程的持久化要求](../requirements/requirements.md) · [数据库配置](../k8s/base/postgres.yaml) · [恢复手册](../k8s/doks/README.md)

**什么时候会改选**　如果这是面向付费用户的服务，有明确的数据库恢复时间要求，而团队也缺少数据库值班能力，我会优先评估具备自动故障切换的托管 PostgreSQL 方案，再通过演练核对实际恢复时间。给应用多开几个 Pod 解决不了数据库单实例中断。[应用与数据库的部署边界](../proposal.md)

## 05　Auth、Papers、Practice 已经分了模块，为什么没有拆成三个微服务？

**口答**　看一次交卷就能理解这个选择。Auth 识别用户，Papers 查找属于他的题目，Practice 判分，再用一次事务保存答案、成绩和位置。这三个动作在同一请求里紧密配合。放在一个 Rust 进程中，模块可以直接调用，出错也容易定位。若拆成三个独立部署，同一笔提交就要协调网络失败、接口版本和重试。现有设计的代价是登录压力上升时，扩的是整个应用。[团队记录的模块取舍](../proposal.md) · [一次提交的完整路径](../backend/src/practice/mod.rs)

**什么时候会改选**　如果测出某个模块长期独占资源，或者有独立团队需要频繁发布它，就有理由先拆这一块。Grace 已有一个例子：监控面板要持续监听集群，还有单独的 Kubernetes 读取权限，因此它拥有独立进程和 Pod。[面板部署](../k8s/monitoring/dashboard.yaml) · [面板权限](../k8s/monitoring/access.yaml)

## 06　已经有多个应用副本了，登录为什么还要查 PostgreSQL？为什么不用 JWT？

**口答**　我们希望用户登出后，这次会话立即失效。当前浏览器保存随机令牌，数据库保存摘要。任何 Pod 都能查验同一会话，登出时删除该条记录即可。自包含 JWT 可以让各 Pod 本地验签，减少逐次查库，但已签发的令牌在到期前仍可能有效。要实现立即撤销，还要引入撤销记录或其他共享状态。Grace 已经运行 PostgreSQL，当前方案少维护一个系统。[登录与登出](../backend/src/auth/mod.rs) · [会话读写](../backend/src/auth/store.rs)

**什么时候会改选**　如果实测会话查询成为瓶颈，且多个服务都需要独立验签，可以考虑短期 JWT，并明确可接受的撤销延迟。如果立即撤销仍是硬要求，也可以先比较 Redis 会话存储。选哪条路，取决于要解决的是数据库读取压力，还是跨服务授权。[当前会话期限与 Cookie 设置](../backend/src/auth/rules.rs)

## 07　两台设备同时答题时，为什么要检查版本号？让最后一次保存生效不是更简单吗？

**口答**　假设手机和电脑都停在版本 7。手机先保存答案，进度变成 8。电脑上一个较早发出的切题请求，因为网络慢才到达。若按到达顺序覆盖，旧操作就会把用户带回先前的位置。Grace 让客户端声明预期版本，数据库只接受与当前版本一致的更新。旧请求收到 409，页面加载最新状态。代价是版本号属于整份卷子，两台设备即便修改不同题，也可能需要复核。[数据库版本检查](../backend/src/practice/store.rs) · [前端冲突处理](../frontend/src/Practice.tsx)

**什么时候会改选**　如果产品支持多人共同编辑一份试卷，就值得把版本号细化到每道题，并给每个用户单独保存当前位置。对于主题颜色等容易撤销的偏好设置，最后一次写入生效则足够简单。

## 08　压测没有显示五个 Pod 比两个更快，为什么还保留 CPU 自动扩缩容？

**口答**　我们要分别回答两个问题：系统能否按负载增加副本，增加后能否改善这项业务。HPA 在测试中确实扩到了五个可用 Pod。但在两节点 minikube 上，每秒安排 120 个完整练习流程时，固定两个副本实际完成 83.75 个/秒，开启 HPA 后完成 79.89 个/秒。我们保留 HPA 来应对应用 CPU 压力，也能演示扩容机制。现有数据只支持第一项结论，不能据此承诺更高吞吐。[扩容策略](../k8s/base/hpa.yaml) · [实验条件与结果](../dashboard/evidence/practice-pressure.md)

**什么时候会改选**　如果真实流量较稳定，固定两个副本已经满足延迟目标，固定容量更容易预测。如果用户等待主要来自数据库或模型调用，CPU 也未必是合适的扩容信号。下一步要结合端到端延迟、数据库等待和实际完成量，判断应改变扩容指标，还是先解决别处的瓶颈。[业务压测方法](../scripts/compare-practice-hpa.py)

## 09　既然要做实时面板，为什么选 SSE？WebSocket 或定时轮询有什么优势？

**口答**　Grace 面板的大部分实时流量只有一个方向：服务端观察集群变化，浏览器接收快照。SSE 正好支持这种长连接，页面用 EventSource 接收更新。启动或停止一次负载测试，另发普通 HTTP 请求即可。WebSocket 更适合双方频繁发送消息，这里引入双向消息协议的收益有限。定时轮询实现最简单，但更新速度受轮询间隔限制，集群空闲时仍要请求。[事件流与心跳](../backend/src/bin/dashboard/main.rs) · [浏览器订阅](../dashboard/app.js) · [运维命令](../backend/src/devops.rs)

**什么时候会改选**　如果只展示每分钟刷新的只读状态，我会选轮询，省去长连接维护。如果以后要做多人协同操作，浏览器持续发送命令并接收确认，WebSocket 就更有价值。另一个实际代价是 SSE 连接本身需要重连，收到心跳只能证明连接仍在，资源新鲜度还要看最后一次集群同步。[面板显示的两种时间](../dashboard/README.md)

## 10　为什么每天用 pg_dump 备份到 Spaces？为什么不直接做卷快照，或者做时间点恢复？

**口答**　Grace 当前需要一份独立于原卷、能恢复到另一套 PostgreSQL 的备份。每天多伦多时间 05:00，CronJob 导出数据库并上传到私有 Spaces，检查上传大小后再清理旧归档。逻辑备份便于在空库里恢复，再通过登录、读卷和核对答案检查结果。卷快照更适合整卷回退，但依赖存储平台和一致性安排。每日导出的代价是两次成功备份之间存在数据缺口。[备份调度](../k8s/doks/backup.yaml) · [上传和保留](../k8s/doks/backup.sh) · [恢复步骤](../k8s/doks/README.md)

**什么时候会改选**　如果允许丢失的数据从“一天内”收紧为“几分钟内”，就应评估基础备份加连续 WAL 归档，让数据库能恢复到指定时间点。若目标是尽快恢复整卷，可以另评估卷快照。两种目标要分别演练：一项核对能找回哪一分钟的数据，另一项计时多久能让用户重新读回答案。
