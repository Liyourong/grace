use axum::{Json, http::StatusCode};
use futures_util::future::join_all;
use serde_json::{Value, json};
use std::time::Duration;

const QUERIES: &[(&str, &str)] = &[
    (
        "cpu",
        "100 * (1 - avg by (node) (rate(node_cpu_seconds_total{job=\"grace-nodes\",mode=\"idle\"}[2m])))",
    ),
    (
        "memory",
        "100 * (1 - max by(node)(node_memory_MemAvailable_bytes{job=\"grace-nodes\"}) / max by(node)(node_memory_MemTotal_bytes{job=\"grace-nodes\"}))",
    ),
    (
        "root",
        "100 * (1 - max by(node)(node_filesystem_avail_bytes{job=\"grace-nodes\",mountpoint=\"/\"}) / max by(node)(node_filesystem_size_bytes{job=\"grace-nodes\",mountpoint=\"/\"}))",
    ),
    (
        "netIn",
        "sum by(node)(rate(node_network_receive_bytes_total{job=\"grace-nodes\",device!~\"lo|veth.*|cali.*|cni.*|docker.*|br-.*|flannel.*\"}[2m]))",
    ),
    (
        "netOut",
        "sum by(node)(rate(node_network_transmit_bytes_total{job=\"grace-nodes\",device!~\"lo|veth.*|cali.*|cni.*|docker.*|br-.*|flannel.*\"}[2m]))",
    ),
    (
        "podIn",
        r#"sum(max by(pod,interface)(irate(container_network_receive_bytes_total{job="kubernetes-nodes-cadvisor",namespace="grace",pod=~"app-.+",interface!="lo",interface!=""}[2m])))"#,
    ),
    (
        "podOut",
        r#"sum(max by(pod,interface)(irate(container_network_transmit_bytes_total{job="kubernetes-nodes-cadvisor",namespace="grace",pod=~"app-.+",interface!="lo",interface!=""}[2m])))"#,
    ),
    (
        "pvc",
        "max by(persistentvolumeclaim)(kubelet_volume_stats_available_bytes{namespace=\"grace\",persistentvolumeclaim=\"data-db-0\"})",
    ),
    (
        "capacity",
        "max by(persistentvolumeclaim)(kubelet_volume_stats_capacity_bytes{namespace=\"grace\",persistentvolumeclaim=\"data-db-0\"})",
    ),
    (
        "desired",
        "max(kube_deployment_spec_replicas{namespace=\"grace\",deployment=\"app\"})",
    ),
    (
        "available",
        "max(kube_deployment_status_replicas_available{namespace=\"grace\",deployment=\"app\"})",
    ),
    (
        "answers",
        "60 * sum(rate(grace_answers_total{namespace=\"grace\"}[2m]))",
    ),
    (
        "failures",
        "100 * sum(rate(grace_answer_failures_total{namespace=\"grace\"}[2m])) / sum(rate(grace_answers_total{namespace=\"grace\"}[2m]))",
    ),
    (
        "latency",
        "1000 * sum(rate(grace_answer_seconds_total{namespace=\"grace\"}[2m])) / sum(rate(grace_answers_total{namespace=\"grace\"}[2m]))",
    ),
];
pub async fn ranges() -> Result<Json<Value>, StatusCode> {
    let end = super::now() / 1000 / 15 * 15;
    let base = std::env::var("PROMETHEUS_URL")
        .unwrap_or_else(|_| "http://grace-monitor-prometheus-server.grace.svc".into());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let url = format!("{base}/api/v1/query_range");
    let results = join_all(QUERIES.iter().map(|(key, query)| {
        let req = client.get(&url).query(&[
            ("query", query.to_string()),
            ("start", (end - 900).to_string()),
            ("end", end.to_string()),
            ("step", "15".into()),
        ]);
        async move {
            let result = async {
                let data = req
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<Value>()
                    .await?;
                Ok::<_, reqwest::Error>(data)
            }
            .await;
            (
                *key,
                result
                    .ok()
                    .filter(|v| v["status"] == "success")
                    .map(|v| v["data"]["result"].clone()),
            )
        }
    }))
    .await;
    if results.iter().all(|r| r.1.is_none()) {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    Ok(Json(
        json!({"end":end,"series":results.into_iter().collect::<std::collections::BTreeMap<_,_>>() }),
    ))
}
