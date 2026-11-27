// Opt-in HPA rehearsal. One fixed CPU pulse requires one fresh lease.
use std::time::{Duration, Instant};

pub async fn run() {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(2))
        .build()
        .expect("demo client");
    let fast = std::env::var("GRACE_HPA_FAST").as_deref() != Ok("0");
    loop {
        let active = client
            .get("http://grace-dashboard:9090/api/load-test")
            .send()
            .await
            .is_ok_and(|r| r.status() == reqwest::StatusCode::NO_CONTENT);
        if active {
            let _ = tokio::task::spawn_blocking(move || {
                let end = Instant::now() + Duration::from_millis(if fast { 800 } else { 300 });
                while Instant::now() < end {
                    std::hint::spin_loop();
                }
            })
            .await;
        }
        let pause = if active {
            Duration::from_millis(if fast { 200 } else { 700 })
        } else {
            Duration::from_millis(if fast { 750 } else { 3000 })
        };
        tokio::time::sleep(pause).await;
    }
}
