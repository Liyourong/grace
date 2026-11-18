use super::{Store, now};
use reqwest::{Client, Response};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    error::Error,
    fs,
    time::Duration,
};
use tokio::{task::JoinHandle, time::sleep};

type Fault = Box<dyn Error + Send + Sync>;
const SA: &str = "/var/run/secrets/kubernetes.io/serviceaccount";
#[derive(Clone)]
pub struct Kube {
    client: Client,
}

impl Kube {
    pub fn new() -> Result<Self, Fault> {
        let ca = reqwest::Certificate::from_pem(&fs::read(format!("{SA}/ca.crt"))?)?;
        Ok(Self {
            client: Client::builder()
                .add_root_certificate(ca)
                .connect_timeout(Duration::from_secs(5))
                .build()?,
        })
    }
    async fn get(
        &self,
        path: &str,
        query: &[(&str, &str)],
        timeout: u64,
    ) -> Result<Response, Fault> {
        let req = self
            .client
            .get(format!("https://kubernetes.default.svc{path}"))
            .bearer_auth(fs::read_to_string(format!("{SA}/token"))?.trim())
            .query(query)
            .timeout(Duration::from_secs(timeout));
        Ok(req.send().await?.error_for_status()?)
    }
}
fn path(kind: &str) -> String {
    match kind {
        "nodes" => "/api/v1/nodes".into(),
        "slices" => "/apis/discovery.k8s.io/v1/namespaces/grace/endpointslices".into(),
        "hpa" => "/apis/autoscaling/v2/namespaces/grace/horizontalpodautoscalers".into(),
        "deployments" => "/apis/apps/v1/namespaces/grace/deployments".into(),
        other => format!(
            "/api/v1/namespaces/grace/{}",
            if other == "claims" {
                "persistentvolumeclaims"
            } else {
                other
            }
        ),
    }
}
fn items(v: &Value) -> impl Iterator<Item = &Value> {
    v.as_array().into_iter().flatten()
}
fn project(kind: &str, v: &Value) -> Option<Value> {
    let name = v["metadata"]["name"].as_str()?;
    let spec = &v["spec"];
    let status = &v["status"];
    let ready = items(&status["conditions"]).any(|c| c["type"] == "Ready" && c["status"] == "True");
    let app = &v["metadata"]["labels"]["app"];
    let value = match kind {
        "nodes" => json!({"ready":ready}),
        "pods" if ["grace", "caddy", "db"].contains(&app.as_str().unwrap_or("")) => {
            json!({"uid":v["metadata"]["uid"],"app":app,"node":spec["nodeName"],"ready":ready,"phase":status["phase"],"deleting":!v["metadata"]["deletionTimestamp"].is_null(),"claims":items(&spec["volumes"]).filter_map(|x|x["persistentVolumeClaim"]["claimName"].as_str()).collect::<Vec<_>>()})
        }
        "services" if ["app", "caddy", "db"].contains(&name) => json!({}),
        "slices" => {
            let svc = &v["metadata"]["labels"]["kubernetes.io/service-name"];
            if !["app", "db", "caddy"].contains(&svc.as_str().unwrap_or("")) {
                return None;
            }
            json!({"service":svc,"pods":items(&v["endpoints"]).filter(|e|e["targetRef"]["kind"]=="Pod" && e["conditions"]["ready"]!=false && e["conditions"]["terminating"]!=true).filter_map(|e|e["targetRef"]["name"].as_str()).collect::<Vec<_>>()})
        }
        "claims" => json!({"phase":status["phase"]}),
        "deployments" if name == "app" => {
            json!({"desired":spec["replicas"],"available":status["availableReplicas"].as_u64().unwrap_or(0)})
        }
        "hpa" if name == "app" => {
            json!({"max":spec["maxReplicas"],"desired":status["desiredReplicas"],"cpu":items(&status["currentMetrics"]).find(|m|m["resource"]["name"]=="cpu").map(|m|&m["resource"]["current"]["averageUtilization"]), "target":items(&spec["metrics"]).find(|m|m["resource"]["name"]=="cpu").map(|m|&m["resource"]["target"]["averageUtilization"])})
        }
        _ => return None,
    };
    Some(value)
}
#[derive(Clone, serde::Serialize)]
pub struct PodEvent {
    id: String,
    action: &'static str,
    pod: String,
    time: u64,
}

fn pod_transition(
    events: &mut VecDeque<PodEvent>,
    name: &str,
    old: Option<&Value>,
    new: Option<&Value>,
) {
    let uid = |v: &Value| v["uid"].as_str().map(str::to_owned);
    let before = old.and_then(uid);
    let after = new.and_then(uid);
    if before == after {
        return;
    }
    for (action, identity) in [("Deleted", before), ("Created", after)] {
        if let Some(id) = identity {
            events.push_back(PodEvent {
                id: format!("{action}/{id}"),
                action,
                pod: name.into(),
                time: now(),
            });
            if events.len() > 100 {
                events.pop_front();
            }
        }
    }
}

// Kubernetes events and pod logs are newline-delimited, independent of HTTP chunk boundaries.
async fn lines(
    mut response: Response,
    mut visit: impl FnMut(&[u8]) -> Result<(), Fault>,
) -> Result<(), Fault> {
    let mut pending = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        for part in chunk.split_inclusive(|b| *b == b'\n') {
            pending.extend_from_slice(part);
            if pending.len() > 4 * 1024 * 1024 {
                return Err("line exceeds limit".into());
            }
            if pending.last() == Some(&b'\n') {
                visit(&pending)?;
                pending.clear()
            }
        }
    }
    if !pending.is_empty() {
        visit(&pending)?
    }
    Ok(())
}
async fn list_watch(k: &Kube, s: &Store, kind: &str) -> Result<(), Fault> {
    let path = path(kind);
    let list: Value = k.get(&path, &[], 12).await?.json().await?;
    let version = list["metadata"]["resourceVersion"]
        .as_str()
        .ok_or("missing resource version")?;
    let group: BTreeMap<_, _> = items(&list["items"])
        .filter_map(|v| {
            Some((
                v["metadata"]["name"].as_str()?.to_owned(),
                project(kind, v)?,
            ))
        })
        .collect();
    s.send_modify(|v| {
        if kind == "pods" {
            if let Some(previous) = v.resources.get(kind) {
                for (name, pod) in previous {
                    pod_transition(&mut v.pod_events, name, Some(pod), group.get(name));
                }
                for (name, pod) in &group {
                    if !previous.contains_key(name) {
                        pod_transition(&mut v.pod_events, name, None, Some(pod));
                    }
                }
            }
        }
        v.resources.insert(kind.into(), group);
        v.synced.insert(kind.into(), true);
        v.updated = now()
    });
    let resp = k
        .get(
            &path,
            &[
                ("watch", "true"),
                ("resourceVersion", version),
                ("timeoutSeconds", "60"),
                ("allowWatchBookmarks", "true"),
            ],
            75,
        )
        .await?;
    lines(resp, |line| {
        let e: Value = serde_json::from_slice(line)?;
        if e["type"] == "ERROR" {
            return Err("Kubernetes watch expired".into());
        }
        if e["type"] == "BOOKMARK" {
            return Ok(());
        }
        let obj = &e["object"];
        let name = obj["metadata"]["name"].as_str().ok_or("missing name")?;
        let data = project(kind, obj);
        s.send_if_modified(|v| {
            let group = v.resources.get_mut(kind).unwrap();
            let data = data.filter(|_| e["type"] != "DELETED");
            if kind == "pods" {
                pod_transition(&mut v.pod_events, name, group.get(name), data.as_ref());
            }
            let changed = if data.is_none() {
                group.remove(name).is_some()
            } else if group.get(name) != data.as_ref() {
                group.insert(name.into(), data.unwrap());
                true
            } else {
                false
            };
            if changed {
                v.updated = now()
            }
            changed
        });
        Ok(())
    })
    .await
}
pub async fn observe(k: Kube, s: Store, kind: &'static str) {
    loop {
        if let Err(e) = list_watch(&k, &s, kind).await {
            eprintln!("watch {kind}: {e}");
            s.send_if_modified(|v| v.synced.insert(kind.into(), false) != Some(false));
            sleep(Duration::from_secs(3)).await;
        }
    }
}
async fn log_tail(k: Kube, s: Store, pod: String, logs: super::logs::Logs) {
    let path = format!("/api/v1/namespaces/grace/pods/{pod}/log");
    loop {
        if let Ok(resp) = k
            .get(
                &path,
                &[
                    ("container", "grace"),
                    ("follow", "true"),
                    ("sinceSeconds", "120"),
                ],
                125,
            )
            .await
        {
            let _ = lines(resp, |line| {
                let Ok(v) = serde_json::from_slice::<Value>(line) else {
                    return Ok(());
                };
                if v["event"] == "grace.request" {
                    logs.ingest(&pod, v);
                    return Ok(());
                }
                if v["event"] != "grace.demo.login" {
                    return Ok(());
                }
                s.send_if_modified(|state| {
                    let Some(row) = state
                        .run
                        .as_mut()
                        .and_then(|r| r.requests.iter_mut().find(|r| v["id"] == r.id))
                    else {
                        return false;
                    };
                    if row.pod.is_some() {
                        return false;
                    }
                    row.pod = Some(pod.clone());
                    row.server_status = v["status"].as_u64().and_then(|n| u16::try_from(n).ok());
                    true
                });
                Ok(())
            })
            .await;
        }
        sleep(Duration::from_secs(1)).await;
    }
}
pub async fn observe_logs(k: Kube, s: Store, logs: super::logs::Logs) {
    let mut rx = s.subscribe();
    let mut tasks: HashMap<String, JoinHandle<()>> = HashMap::new();
    loop {
        let pods: HashSet<_> = rx
            .borrow_and_update()
            .resources
            .get("pods")
            .into_iter()
            .flat_map(|g| g.iter())
            .filter(|(_, v)| v["app"] == "grace" && v["phase"] == "Running")
            .map(|(name, _)| name.clone())
            .collect();
        tasks.retain(|p, t| {
            if pods.contains(p) {
                true
            } else {
                t.abort();
                false
            }
        });
        for pod in pods {
            tasks
                .entry(pod.clone())
                .or_insert_with(|| tokio::spawn(log_tail(k.clone(), s.clone(), pod, logs.clone())));
        }
        if rx.changed().await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pod_history_tracks_identities_once_across_updates_and_replacements() {
        let mut events = VecDeque::new();
        let a = json!({"uid":"a"});
        let terminating = json!({"uid":"a","deleting":true});
        let b = json!({"uid":"b"});
        pod_transition(&mut events, "db-0", None, Some(&a));
        pod_transition(&mut events, "db-0", Some(&a), Some(&terminating));
        pod_transition(&mut events, "db-0", Some(&terminating), Some(&b));
        pod_transition(&mut events, "db-0", Some(&b), None);
        pod_transition(&mut events, "db-0", None, None);
        assert_eq!(
            events.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["Created/a", "Deleted/a", "Created/b", "Deleted/b"]
        );
    }

    #[test]
    fn service_edges_include_only_routable_pods() {
        let slice = json!({
            "metadata":{"name":"app-demo","labels":{"kubernetes.io/service-name":"app"}},
            "endpoints":[
                {"targetRef":{"kind":"Pod","name":"serving"},"conditions":{"ready":true}},
                {"targetRef":{"kind":"Pod","name":"starting"},"conditions":{"ready":false}},
                {"targetRef":{"kind":"Pod","name":"draining"},"conditions":{"ready":true,"terminating":true}},
                {"targetRef":{"kind":"Node","name":"worker"}}
            ]
        });
        assert_eq!(
            project("slices", &slice).unwrap()["pods"],
            json!(["serving"])
        );
    }
}
