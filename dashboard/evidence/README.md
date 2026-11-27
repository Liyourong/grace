# Local rehearsal · 18 November 2026

The presentation computer used Caddy to reach the app Service in a dedicated two-node minikube profile. All ten accounts belonged to the local fixture. The request timelines use milliseconds from the presentation computer.

| Check | Observed result |
| --- | --- |
| Ten simultaneous logins | All ten client intervals overlapped. The batch finished in 51 ms. Both Pods served requests, split 6 and 4. |
| One incorrect password | Nine HTTP 200 responses and one HTTP 401. All ten had a confirmed Pod. |
| HPA | Two login workers ran for 120 seconds with a 100 ms pause. HPA CPU reached 110% against a 70% target. The app went from two available replicas to four, and returned to two after cooldown. |
| Login after expansion | All ten succeeded. Four Pods handled 5, 2, 2 and 1 requests. Each newly created Pod handled two. |
| Watch and reconnect | Adding and removing a temporary empty EndpointSlice appeared in the dashboard snapshot on the first poll after each command. Cutting the browser's event stream displayed the interrupted state. Reconnecting and refreshing preserved the ten client rows and counts. |
| Metrics | Both nodes supplied CPU, memory, root disk and network series. One accepted and one stale answer submission produced a 50% HTTP failure rate in the active sampling window. |
| Database volume | The default local hostPath volume produced no kubelet capacity or available-byte series. The dashboard displayed missing data. |

The JSON files contain the recorded counts and samples. Screenshots were taken from the running dashboard. The numbered dots in the in-flight screenshot show confirmed completions. Login durations come from the client timeline.

The one-click CPU test on the Topology tab reached four available app replicas at 114 seconds and five at 129 seconds. The button stopped the load at five. The [browser screenshot](hpa-button.png) and [observations](hpa-button.json) record that run.
