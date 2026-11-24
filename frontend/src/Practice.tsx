import { useEffect, useRef, useState } from "react"
import { ArrowLeft, ArrowRight, ArrowUp, Check, X } from "lucide-react"

import { api, activity, operationId, KIND_LABEL, type Answer, type ApiError, type ExplainResponse, type Paper, type Question, type Stats } from "@/api"
import { useStudyTime } from "@/useStudyTime"
import { Notice } from "@/components/notice"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Progress } from "@/components/ui/progress"
import { Textarea } from "@/components/ui/textarea"
import { cn } from "@/lib/utils"

// shadcn has no success token; these two are the only colors outside the theme.
const OK = "border-emerald-600/50! text-emerald-700 dark:text-emerald-400"
const BAD = "border-destructive/50! text-destructive"

type Warn = { kind: "error" | "warn"; message: string }
type ExplainState = {
  paperId: string
  questionId: string
  status: "loading" | "success" | "error"
  message?: string
}

export function Practice(props: {
  paperId: string
  group: React.RefObject<string | undefined>
  onExpired: (what: string) => void
  onChanged: (group?: string) => void
  onHome: () => void
}) {
  const { paperId, group, onExpired, onChanged, onHome } = props
  const base = `/api/papers/${encodeURIComponent(paperId)}`
  const [paper, setPaper] = useState<Paper | null>(null)
  const [clockOffset, setClockOffset] = useState<number | null>(null)
  const [loadError, setLoadError] = useState<ApiError | null>(null)
  const [attempt, setAttempt] = useState(0)
  const [index, setIndex] = useState(0)
  const [warn, setWarn] = useState<Warn | null>(null)
  const [busy, setBusy] = useState(false)
  const [finished, setFinished] = useState(false)
  const [explain, setExplain] = useState<ExplainState | null>(null)
  // Every new answer, question, or paper invalidates older Function responses.
  const explainRequestId = useRef(0)
  function clearExplain() {
    explainRequestId.current += 1
    setExplain(null)
  }
  // Unsubmitted text lives only in this page; it is never saved or restored.
  const [drafts, setDrafts] = useState(() => new Map<string, string>())
  // Each load starts a new queue generation. Discard queued actions from an outdated view.
  const progress = useRef<{ revision: number } | null>(null)
  const queue = useRef<Promise<unknown>>(Promise.resolve())
  const targetIndex = useRef(0)
  const [navigating, setNavigating] = useState(false)

  function send<T>(method: "PUT" | "POST", path: string, body?: object, action?:string) {
    const view = progress.current
    const next = queue.current.then(async () => {
      if (!view || view !== progress.current) return
      const r = await api<T & { revision?: number }>(method, path, method === "PUT" ? { ...body, expected_revision: view.revision } : body,action)
      if (view !== progress.current) return
      if (r.ok && method === "POST") return r
      const revision = r.ok ? r.data?.revision : undefined
      if (typeof revision === "number" && Number.isSafeInteger(revision) && revision === view.revision + 1) {
        view.revision = revision
      } else if (r.ok || r.uncertain || r.error.code === "progress_conflict") {
        progress.current = null
        clearExplain()
        const message = !r.ok && r.error.code === "progress_conflict"
          ? "Progress changed on another device. Your action was not saved. Review the latest position and try again."
          : "The save could not be confirmed. Review the latest progress before trying again."
        setWarn({ kind: "warn", message })
        setFinished(false)
        setPaper(null)
        setAttempt((n) => n + 1)
        onChanged()
        return
      }
      return r
    })
    queue.current = next
    return next
  }

  useEffect(() => {
    let cancelled = false
    api<Paper>("GET", base,undefined,group.current).then((r) => {
      if (cancelled) return
      if (r.ok) {
        if (!Number.isSafeInteger(r.data.revision)) {
          setLoadError({ code: "revision_required", message: "Refresh the page to load the current practice version." })
          return
        }
        progress.current = { revision: r.data.revision }
        targetIndex.current = Math.max(0, r.data.questions.findIndex((q) => q.id === r.data.current_question_id))
        const serverDate = Date.parse(r.serverDate ?? "")
        const epoch = Number.isFinite(serverDate) && Math.abs(serverDate - Date.now()) > 1500 ? serverDate : Date.now()
        setClockOffset(epoch - performance.now())
        setPaper(r.data)
        setIndex(targetIndex.current)
        setNavigating(false)
        setBusy(false)
      } else if (r.status === 401) onExpired("the paper could not be opened")
      else setLoadError(r.error)
    })
    return () => {
      cancelled = true
      progress.current = null
      // Also discard a pending explanation when switching papers/unmounting.
      explainRequestId.current += 1
    }
  }, [base, attempt, onExpired,group])

  useStudyTime(clockOffset, !!paper?.questions.length && !finished, onChanged)

  if (loadError)
    return (
      <Notice kind="error" message={loadError.message} details={loadError.details}>
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            setLoadError(null)
            setAttempt(attempt + 1)
          }}
        >
          Retry
        </Button>
      </Notice>
    )
  if (!paper) return <p className="text-muted-foreground">Loading paper…</p>
  if (!paper.questions.length) return <p className="text-muted-foreground">This paper has no questions.</p>

  const { questions, stats } = paper
  const q = questions[index]
  const last = index === questions.length - 1
  const hasUnsavedChanges =
    q.kind !== "choice" &&
    !!q.answer &&
    (drafts.get(q.id) ?? q.answer.saved_answer) !== q.answer.saved_answer

  async function go(i: number) {
    clearExplain()
    if (i < 0 || i >= questions.length) return
    targetIndex.current = i
    setNavigating(true)
    const request = send("PUT", `${base}/position`, { question_id: questions[i].id })
    const r = await request
    if (!r) return
    if (queue.current === request) {
      setNavigating(false)
      if (!r.ok) targetIndex.current = index
    }
    if (r.ok) {
      setIndex(i)
      setFinished(false)
      setWarn(null)
    } else if (r.status === 401) onExpired("your position was not saved")
    else setWarn({ kind: "warn", message: `Your position was not saved: ${r.error.message}` })
  }

  async function submit(q: Question, payload: { option_id: string } | { text: string }) {
    clearExplain()
    setBusy(true)
    const action=operationId()
    const r = await send<{ answer: Answer; stats: Stats }>("PUT", `${base}/answers/${encodeURIComponent(q.id)}`, payload,action)
    if (!r) return
    setBusy(false)
    const num = questions.indexOf(q) + 1
    if (r.ok) {
      const { answer, stats } = r.data
      setDrafts((previous) => {
        if (!previous.has(q.id)) return previous
        const next = new Map(previous)
        next.delete(q.id)
        return next
      })
      setPaper((p) => p && { ...p, stats, questions: p.questions.map((x) => (x.id === q.id ? { ...x, answer } : x)) })
      setWarn(null)
      onChanged(action)
    } else if (r.status === 401) {
      onExpired(`your answer to question ${num} was not saved`)
    } else {
      setWarn({ kind: "error", message: `Your answer to question ${num} was not saved: ${r.error.message}` })
    }
  }

  async function explainSaved(q: Question) {
    if (!q.answer) return
    const id = ++explainRequestId.current
    setExplain({ paperId, questionId: q.id, status: "loading" })
    // No answer or secret is sent from the browser. The backend loads the saved answer.
    const r = await api<ExplainResponse>("POST", `${base}/answers/${encodeURIComponent(q.id)}/explain`)
    if (id !== explainRequestId.current) return
    if (r.ok) {
      setExplain({ paperId, questionId: q.id, status: "success", message: r.data.explanation })
    } else if (r.status === 401) {
      onExpired("an explanation could not be requested")
    } else {
      setExplain({ paperId, questionId: q.id, status: "error", message: r.error.message })
    }
  }

  async function finish() {
    clearExplain()
    const r = await send<{ stats: Stats }>("POST", `${base}/finish`)
    if (!r) return
    if (r.status === 401) return onExpired("the practice could not be finished")
    if (!r.ok) return setWarn({ kind: "error", message: r.error.message })
    const { stats } = r.data
    setPaper((p) => p && { ...p, stats })
    setFinished(true)
  }

  return (
    <>
      <h1 className="font-reading text-[clamp(2rem,4vw,2.75rem)] leading-tight font-normal tracking-tight wrap-anywhere">{paper.title}</h1>
      {paper.description && <p className="text-muted-foreground">{paper.description}</p>}
      <div className="grid gap-1.5">
        <Progress value={stats.total ? (stats.answered / stats.total) * 100 : 0} aria-label="Progress" />
        <p className="text-sm text-muted-foreground">
          Answered {stats.answered} / {stats.total} · {stats.correct} correct · {stats.incorrect} incorrect
        </p>
      </div>
      <div className="flex flex-wrap gap-1.5">
        {questions.map((x, i) => (
          <Button
            key={x.id}
            variant="outline"
            size="icon-sm"
            className={cn(x.answer && (x.answer.is_correct ? OK : BAD), i === index && "ring-2 ring-ring")}
            aria-label={`Question ${i + 1}${x.answer ? (x.answer.is_correct ? ", correct" : ", incorrect") : ", unanswered"}`}
            aria-current={i === index ? "step" : undefined}
            onClick={() => go(i)}
          >
            {i + 1}
          </Button>
        ))}
      </div>
      {warn && <Notice kind={warn.kind} message={warn.message} />}
      <Card>
        {finished ? (
          <CardContent className="grid gap-4">
            <h2 className="text-lg font-semibold">Practice complete</h2>
            <div className="grid grid-cols-3 gap-3 text-center text-sm text-muted-foreground">
              <div>
                <strong className={cn("block text-3xl", OK)}>{stats.correct}</strong>Correct
              </div>
              <div>
                <strong className={cn("block text-3xl", BAD)}>{stats.incorrect}</strong>Incorrect
              </div>
              <div>
                <strong className="block text-3xl text-foreground">{stats.unanswered}</strong>Unanswered
              </div>
            </div>
            <p className="text-sm text-muted-foreground">Your answers are saved. You can come back and change them anytime.</p>
            <div className="flex justify-between">
              <Button variant="ghost" onClick={() => {setFinished(false);void activity("review_answers")}}>
                Back to edit
              </Button>
              <Button onClick={onHome}>Done</Button>
            </div>
          </CardContent>
        ) : (
          <CardContent className="grid gap-4">
            <div className="flex items-center justify-between">
              <Badge variant="secondary">{KIND_LABEL[q.kind]}</Badge>
              <span className="text-sm text-muted-foreground">
                Question {index + 1} of {questions.length}
              </span>
            </div>
            <p className="font-reading text-[1.375rem] leading-[1.45] wrap-anywhere whitespace-pre-wrap sm:text-2xl">{q.prompt}</p>
            {q.kind === "choice" ? (
              <div className="grid gap-2">
                {q.options?.map((o, i) => {
                  const chosen = q.answer?.saved_answer === o.id
                  return (
                    <Button
                      key={o.id}
                      variant="outline"
                      className={cn(
                        "h-auto justify-start py-2 text-left text-base leading-relaxed font-normal whitespace-normal",
                        chosen && (q.answer?.is_correct ? OK : BAD)
                      )}
                      aria-pressed={chosen}
                      disabled={busy || navigating}
                      onClick={() => submit(q, { option_id: o.id })}
                    >
                      <span aria-hidden className="flex size-6 shrink-0 items-center justify-center rounded-md bg-muted text-xs text-foreground">
                        {String.fromCharCode(65 + i)}
                      </span>
                      {o.text}
                    </Button>
                  )
                })}
              </div>
            ) : (
              <TextAnswer
                key={q.id}
                saved={q.answer?.saved_answer}
                value={drafts.get(q.id) ?? q.answer?.saved_answer ?? ""}
                busy={busy || navigating}
                onDraft={(text) => {
                  clearExplain()
                  setDrafts((previous) => new Map(previous).set(q.id, text))
                }}
                onEmpty={() => setWarn({ kind: "warn", message: "Please enter an answer first." })}
                onSubmit={(text) => submit(q, { text })}
              />
            )}
            {q.answer && (
              <Feedback
                q={q}
                a={q.answer}
                busy={busy || navigating}
                explain={explain?.paperId === paperId && explain.questionId === q.id ? explain : null}
                onExplain={() => explainSaved(q)}
                hasUnsavedChanges={hasUnsavedChanges}
              />
            )}
            <div className="flex justify-between">
              <Button variant="ghost" disabled={index === 0} onClick={() => go(targetIndex.current - 1)}>
                <ArrowLeft /> Previous
              </Button>
              {last ? (
                <Button disabled={navigating} onClick={finish}>Finish</Button>
              ) : (
                <Button variant="ghost" onClick={() => go(targetIndex.current + 1)}>
                  Next <ArrowRight />
                </Button>
              )}
            </div>
          </CardContent>
        )}
      </Card>
    </>
  )
}

function TextAnswer(props: {
  saved?: string
  value: string
  busy: boolean
  onDraft: (text: string) => void
  onEmpty: () => void
  onSubmit: (text: string) => void
}) {
  const { saved, value, busy, onDraft, onEmpty, onSubmit } = props
  const submit = () => (value.trim() ? onSubmit(value) : onEmpty())
  return (
    <div className="grid gap-1.5">
      <div className="flex items-end gap-2">
        <Textarea
          aria-label="Your answer"
          placeholder="Type your answer, Enter to submit (Shift+Enter for a new line)"
          rows={2}
          maxLength={2000}
          disabled={busy}
          value={value}
          onChange={(e) => onDraft(e.target.value)}
          onKeyDown={(e) => {
            // isComposing: Enter that confirms an IME candidate (e.g. pinyin) must not submit.
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault()
              submit()
            }
          }}
        />
        <Button size="icon" aria-label="Submit answer" title="Submit" disabled={busy} onClick={submit}>
          <ArrowUp />
        </Button>
      </div>
      {saved !== undefined && value !== saved && (
        <p className="text-sm text-muted-foreground">You have unsubmitted changes; they won't be saved if you leave</p>
      )}
    </div>
  )
}

function Feedback({
  q,
  a,
  busy,
  explain,
  onExplain,
  hasUnsavedChanges,
}: {
  q: Question
  a: Answer
  busy: boolean
  explain: ExplainState | null
  onExplain: () => void
  hasUnsavedChanges: boolean
}) {
  const rows = [
    ...(q.kind === "choice" ? [] : [["Your answer", a.saved_answer]]),
    ["Reference answer", a.correct_answer],
    ["Explanation", a.explanation],
  ]
  return (
    <div className={cn("grid gap-2 rounded-lg border p-4 text-sm", a.is_correct ? OK : BAD)}>
      <div className="flex items-center gap-1.5 font-medium">
        {a.is_correct ? <Check className="size-4" /> : <X className="size-4" />}
        {a.is_correct ? "Correct" : "Incorrect"}
      </div>
      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-foreground">
        {rows.map(([label, text]) => (
          <div key={label} className="contents">
            <dt className="text-muted-foreground">{label}</dt>
            <dd className="whitespace-pre-wrap">{text}</dd>
          </div>
        ))}
      </dl>
      <div className="grid justify-items-start gap-2 border-t pt-3">
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={onExplain}
          disabled={
            busy ||
            hasUnsavedChanges ||
            explain?.status === "loading" ||
            explain?.status === "success"
          }
        >
          {explain?.status === "loading"
            ? "Explaining…"
            : explain?.status === "success"
              ? "Explained"
              : explain?.status === "error"
                ? "Retry Explain"
                : "Explain"}
        </Button>
        {hasUnsavedChanges && (
          <p className="text-muted-foreground">Submit your edited answer before requesting a new explanation.</p>
        )}
        {explain?.status === "success" && (
          <div className="grid gap-1 text-foreground" role="status">
            <strong>Additional explanation</strong>
            <p className="whitespace-pre-wrap font-normal">{explain.message}</p>
          </div>
        )}
        {explain?.status === "error" && (
          <p className="text-destructive" role="alert">Explanation unavailable: {explain.message}. Please try again.</p>
        )}
      </div>
    </div>
  )
}
