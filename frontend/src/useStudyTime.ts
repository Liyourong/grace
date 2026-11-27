import { useEffect } from "react"

// One question view is mounted per browser tab. Sign-out flushes its tail before revoking the session.
let flushCurrent: (() => Promise<void>) | undefined
export async function flushStudyTime() {
  await flushCurrent?.()
}

export function useStudyTime(clockOffset: number | null, active: boolean, refreshLibrary: () => void) {
  useEffect(() => {
    if (!active || clockOffset === null) return
    const now = () => Math.floor(performance.now() + clockOffset)
    const foreground = () => document.visibilityState === "visible" && document.hasFocus()
    const pending = new Set<Promise<void>>()
    let lastInput = now()
    let start: number | null = foreground() ? lastInput : null

    function flush(at: number) {
      if (start === null) return
      const end = Math.min(at, lastInput + 120_000)
      while (start < end) {
        const stop = Math.min(end, start + 15_000)
        const report = fetch("/api/practice/time", {
          method: "POST", credentials: "same-origin", keepalive: true,
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ start_ms: start, end_ms: stop }), signal: AbortSignal.timeout(4000),
        }).then(() => {}, () => {})
        pending.add(report)
        void report.finally(() => pending.delete(report))
        start = stop
      }
      if (at >= lastInput + 120_000) start = null
    }
    function pause() {
      flush(now())
      start = null
    }
    function interact() {
      if (!foreground()) return
      const at = now()
      if (at >= lastInput + 120_000) flush(at)
      lastInput = at
      start ??= at
    }
    function visibility() {
      if (foreground()) interact()
      else pause()
    }
    async function finish() {
      pause()
      await Promise.all(pending)
    }
    flushCurrent = finish
    const timer = window.setInterval(() => foreground() ? flush(now()) : pause(), 15_000)
    const inputs = ["pointerdown", "pointermove", "keydown", "scroll", "touchstart"]
    for (const event of inputs) document.addEventListener(event, interact, { passive: true, capture: true })
    document.addEventListener("visibilitychange", visibility)
    window.addEventListener("blur", pause)
    window.addEventListener("focus", visibility)
    window.addEventListener("pagehide", pause)
    window.addEventListener("pageshow", visibility)
    return () => {
      clearInterval(timer)
      for (const event of inputs) document.removeEventListener(event, interact, true)
      document.removeEventListener("visibilitychange", visibility)
      window.removeEventListener("blur", pause)
      window.removeEventListener("focus", visibility)
      window.removeEventListener("pagehide", pause)
      window.removeEventListener("pageshow", visibility)
      flushCurrent = undefined
      void finish().then(refreshLibrary)
    }
  }, [active, clockOffset, refreshLibrary])
}
