export type ApiError = { code: string; message: string; details?: string[] }
export type ApiResult<T> =
  | { ok: true; status: number; data: T; serverDate: string | null }
  | { ok: false; status: number; error: ApiError; uncertain: boolean }

export type User = { username: string }
export type Stats = { total: number; answered: number; correct: number; incorrect: number; unanswered: number }
export type FolderSummary = { id: string; name: string }
export type Library = { folders: FolderSummary[]; papers: PaperSummary[]; study_seconds: number }
export type PaperSummary = {
  current_question_number: number
  is_default: boolean
  folder_id: string | null
  id: string
  title: string
  uploaded_at: string
  last_activity_at: string | null
  stats: Stats
}
export type Answer = { saved_answer: string; is_correct: boolean; correct_answer: string; explanation: string }
export type ExplainResponse = { explanation: string }
export type Question = {
  id: string
  kind: "choice" | "translation" | "sentence"
  prompt: string
  options: { id: string; text: string }[] | null
  answer: Answer | null
}
export type Paper = {
  title: string
  description: string | null
  revision: number
  current_question_id: string | null
  questions: Question[]
  stats: Stats
}

// Never retries: a replayed upload creates a second paper. `uncertain` means the write may or may not have landed.
export async function api<T>(method: string, path: string, body?: unknown): Promise<ApiResult<T>> {
  const opts: RequestInit = { method, credentials: "same-origin" }
  if (body !== undefined) {
    opts.headers = { "Content-Type": "application/json" }
    opts.body = typeof body === "string" ? body : JSON.stringify(body)
  }
  let res: Response
  try {
    res = await fetch(path, opts)
  } catch {
    return { ok: false, status: 0, uncertain: true, error: { code: "network", message: "Network connection failed" } }
  }
  let data = null
  try {
    data = await res.json()
  } catch {
    /* 204 or a non-JSON proxy page */
  }
  if (res.ok) return { ok: true, status: res.status, data, serverDate: res.headers.get("Date") }
  const error = data?.error ?? { code: `http_${res.status}`, message: `Request failed (HTTP ${res.status})` }
  return { ok: false, status: res.status, error, uncertain: res.status >= 500 }
}
