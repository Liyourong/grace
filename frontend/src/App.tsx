import { useCallback, useEffect, useRef, useState } from "react"
import { FileJson, LayoutDashboard, LibraryBig, LogOut, Monitor, Moon, Plus, Sun, Upload } from "lucide-react"

import { api, type ApiError, type Library, type User } from "@/api"
import { Notice } from "@/components/notice"
import { useTheme } from "@/components/theme-provider"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { cn } from "@/lib/utils"
import { PaperLibrary } from "@/PaperLibrary"
import { Overview } from "@/Overview"
import { flushStudyTime } from "@/useStudyTime"
import { Practice } from "@/Practice"

const appName = import.meta.env.VITE_GRACE_VERSION ? `Grace ${import.meta.env.VITE_GRACE_VERSION}` : "Grace"

const THEMES = {
  system: { icon: Monitor, label: "System", next: "light" },
  light: { icon: Sun, label: "Light", next: "dark" },
  dark: { icon: Moon, label: "Dark", next: "system" },
} as const

function ThemeButton({ className }: { className?: string }) {
  const { theme, setTheme } = useTheme()
  const { icon: Icon, label, next } = THEMES[theme]
  const title = `Theme: ${label} (click to switch)`
  return (
    <Button variant="ghost" size="icon-sm" className={className} title={title} aria-label={title} onClick={() => setTheme(next)}>
      <Icon />
    </Button>
  )
}

type View =
  | { name: "loading" }
  | { name: "error"; error: ApiError }
  | { name: "auth"; message?: string }
  | { name: "app"; user: User }

export default function App() {
  const [view, setView] = useState<View>({ name: "loading" })

  const boot = useCallback(() => {
    return api<{ user: User }>("GET", "/api/auth/me").then((r) => {
      if (r.ok) setView({ name: "app", user: r.data.user })
      else if (r.status === 401) setView({ name: "auth" })
      else setView({ name: "error", error: r.error })
    })
  }, [])
  useEffect(() => {
    boot()
  }, [boot])

  const expired = useCallback(
    (what: string) => setView({ name: "auth", message: `Your session has expired; ${what}. Please sign in again.` }),
    []
  )
  const logout = useCallback(async () => {
    await flushStudyTime()
    const r = await api("POST", "/api/auth/logout")
    if (!r.ok && r.status !== 401) return alert(`Sign-out failed; you are still signed in: ${r.error.message}`)
    setView({ name: "auth" })
  }, [])

  if (view.name === "app") return <Shell user={view.user} onLogout={logout} onExpired={expired} />
  return (
    <div className="flex min-h-svh items-center justify-center px-6 py-20">
      <header className="absolute inset-x-0 top-0 flex items-center justify-between px-5 py-4">
        <span className="text-sm font-semibold">{appName}</span>
        <ThemeButton />
      </header>
      {view.name === "loading" && <p className="text-muted-foreground">Loading…</p>}
      {view.name === "error" && (
        <div className="w-full max-w-sm">
          <Notice kind="error" message={view.error.message} details={view.error.details}>
            <Button variant="outline" size="sm" onClick={() => {
              setView({ name: "loading" })
              void boot()
            }}>
              Retry
            </Button>
          </Notice>
        </div>
      )}
      {view.name === "auth" && (
        <Auth message={view.message} onDone={(user) => setView({ name: "app", user })} />
      )}
    </div>
  )
}

type Verification = { verification: { email: string; resend_after_seconds: number } }

function Auth({ message, onDone }: { message?: string; onDone: (user: User) => void }) {
  const [isLogin, setIsLogin] = useState(true)
  const [username, setUsername] = useState("")
  const [email, setEmail] = useState("")
  const [password, setPassword] = useState("")
  // Registration has two steps: the form, then the code that was emailed. The account exists only after step two.
  const [codeSentTo, setCodeSentTo] = useState<string | null>(null)
  const [code, setCode] = useState("")
  const [cooldown, setCooldown] = useState(0)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<ApiError | null>(null)
  const [info, setInfo] = useState<string | null>(null)

  useEffect(() => {
    if (cooldown <= 0) return
    const timer = setTimeout(() => setCooldown((c) => c - 1), 1000)
    return () => clearTimeout(timer)
  }, [cooldown])

  function codeRequested(r: { data: Verification }, resent: boolean) {
    setCodeSentTo(r.data.verification.email)
    setCooldown(r.data.verification.resend_after_seconds)
    setInfo(resent ? "We sent the code again." : null)
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault()
    setBusy(true)
    setError(null)
    setInfo(null)
    if (isLogin) {
      const r = await api<{ user: User }>("POST", "/api/auth/login", { username, password })
      setBusy(false)
      if (r.ok) return onDone(r.data.user)
      return setError(r.error)
    }
    if (codeSentTo === null) {
      const r = await api<Verification>("POST", "/api/auth/register", { username, password, email })
      setBusy(false)
      if (r.ok) return codeRequested(r, false)
      return setError(r.error)
    }
    const r = await api<{ user: User }>("POST", "/api/auth/verify", { email: codeSentTo, code })
    setBusy(false)
    if (r.ok) return onDone(r.data.user)
    setError(
      r.uncertain
        ? { code: "uncertain", message: "We couldn't confirm whether the account was created. Try signing in with your username and password first." }
        : r.error
    )
  }

  async function resend() {
    if (codeSentTo === null) return
    setBusy(true)
    setError(null)
    const r = await api<Verification>("POST", "/api/auth/register/resend", { email: codeSentTo })
    setBusy(false)
    if (r.ok) return codeRequested(r, true)
    setInfo(null)
    setError(r.error)
  }

  function backToForm() {
    setCodeSentTo(null)
    setCode("")
    setCooldown(0)
    setError(null)
    setInfo(null)
  }

  const verifying = !isLogin && codeSentTo !== null
  return (
    <Card className="w-full max-w-sm">
      <CardHeader className="text-center">
        <div aria-hidden className="mx-auto mb-2 flex size-10 items-center justify-center rounded-lg bg-primary font-semibold text-primary-foreground">
          G
        </div>
        <CardTitle className="text-2xl leading-tight font-medium tracking-tight">
          <h1>{verifying ? "Check your email" : isLogin ? "Welcome back" : "Create your account"}</h1>
        </CardTitle>
        <CardDescription>
          {verifying
            ? `Enter the code we sent to ${codeSentTo}`
            : isLogin
              ? "Sign in to continue practicing"
              : "Practice languages with your own papers"}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form className="grid gap-3" onSubmit={submit}>
          {verifying ? (
            <div className="grid gap-1.5">
              <Input
                autoFocus
                name="code"
                placeholder="Verification code"
                aria-label="Verification code"
                aria-describedby="code-hint"
                autoComplete="one-time-code"
                inputMode="numeric"
                spellCheck={false}
                required
                maxLength={32}
                value={code}
                onChange={(e) => setCode(e.target.value)}
              />
              <p id="code-hint" className="text-xs text-muted-foreground">
                The 6-digit code in the email. It may take a minute to arrive; check your spam folder.
              </p>
            </div>
          ) : (
            <>
              <div className="grid gap-1.5">
                <Input
                  autoFocus
                  name="username"
                  placeholder="Username"
                  aria-label="Username"
                  aria-describedby="username-hint"
                  autoComplete="username"
                  autoCapitalize="none"
                  spellCheck={false}
                  required
                  minLength={3}
                  maxLength={32}
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                />
                <p id="username-hint" className="text-xs text-muted-foreground">
                  3–32 letters, digits or underscores; not case-sensitive
                </p>
              </div>
              {!isLogin && (
                <div className="grid gap-1.5">
                  <Input
                    type="email"
                    name="email"
                    placeholder="Email"
                    aria-label="Email"
                    aria-describedby="email-hint"
                    autoComplete="email"
                    autoCapitalize="none"
                    spellCheck={false}
                    required
                    maxLength={254}
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                  />
                  <p id="email-hint" className="text-xs text-muted-foreground">
                    We'll email you a code to verify it
                  </p>
                </div>
              )}
              <div className="grid gap-1.5">
                <Input
                  type="password"
                  name="password"
                  placeholder="Password"
                  aria-label="Password"
                  aria-describedby="password-hint"
                  autoComplete={isLogin ? "current-password" : "new-password"}
                  required
                  minLength={8}
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                />
                <p id="password-hint" className="text-xs text-muted-foreground">
                  8–128 characters; case-sensitive
                </p>
              </div>
            </>
          )}
          {error ? (
            <Notice kind="error" message={error.message} details={error.details} />
          ) : info ? (
            <Notice kind="success" message={info} />
          ) : message && !verifying ? (
            <Notice kind="warn" message={message} />
          ) : null}
          <Button type="submit" disabled={busy}>
            {busy ? "Please wait…" : isLogin ? "Sign in" : verifying ? "Verify and create account" : "Send verification code"}
          </Button>
          {verifying && (
            <div className="flex items-center justify-between text-sm">
              <Button type="button" variant="link" className="h-auto p-0" onClick={backToForm} disabled={busy}>
                Use a different email
              </Button>
              <Button type="button" variant="link" className="h-auto p-0" onClick={resend} disabled={busy || cooldown > 0}>
                {cooldown > 0 ? `Resend code (${cooldown}s)` : "Resend code"}
              </Button>
            </div>
          )}
          {!verifying && (
            <p className="text-center text-sm text-muted-foreground">
              {isLogin ? "Don't have an account?" : "Already have an account?"}
              <Button
                type="button"
                variant="link"
                className="h-auto p-0 pl-1"
                onClick={() => {
                  setIsLogin(!isLogin)
                  setError(null)
                }}
              >
                {isLogin ? "Sign up" : "Sign in"}
              </Button>
            </p>
          )}
        </form>
      </CardContent>
    </Card>
  )
}

function Shell(props: { user: User; onLogout: () => void; onExpired: (what: string) => void }) {
  const { user, onLogout, onExpired } = props
  const [library, setLibrary] = useState<Library | null>(null)
  const listRequest = useRef(0)
  const [listFailed, setListFailed] = useState(false)
  const [screen, setScreen] = useState("overview")
  const [mobileLibrary, setMobileLibrary] = useState(false)
  const activeId = screen === "overview" || screen === "upload" ? null : screen
  function select(id: string | null) {
    setScreen(id ?? "overview")
    setMobileLibrary(false)
  }
  // A list request still in flight after logout must not bounce the login page with a stale "expired" message.
  const alive = useRef(true)
  useEffect(() => {
    alive.current = true
    return () => {
      alive.current = false
    }
  }, [])

  const loadList = useCallback(() => {
    const request = ++listRequest.current
    return api<Library>("GET", "/api/papers").then((r) => {
      if (!alive.current || request !== listRequest.current) return
      if (r.ok) setLibrary(r.data)
      else if (r.status === 401) return onExpired("the paper list could not be loaded")
      setListFailed(!r.ok)
    })
  }, [onExpired])
  useEffect(() => {
    void loadList()
  }, [loadList])
  const goHome = () => select(null)
  const openUpload = () => select("upload")
  useEffect(() => {
    if (screen !== "overview") return
    window.addEventListener("focus", loadList)
    return () => window.removeEventListener("focus", loadList)
  }, [screen, loadList])

  return (
    <div className="flex min-h-svh flex-col md:h-svh md:flex-row">
      <aside className="flex shrink-0 flex-col gap-3 border-b bg-sidebar p-3 text-sidebar-foreground md:w-64 md:border-r md:border-b-0">
        <div className="px-2 pt-1 text-lg font-semibold">{appName}</div>
        <div className="flex flex-wrap gap-1 md:flex-col">
          <Button variant={screen === "overview" ? "secondary" : "ghost"} aria-current={screen === "overview" ? "page" : undefined} className="justify-start" onClick={goHome}>
            <LayoutDashboard /> Overview
          </Button>
          <Button variant={screen === "upload" ? "secondary" : "ghost"} className="justify-start" onClick={openUpload}>
            <Plus /> Upload a paper
          </Button>
          <Button variant="ghost" className="md:hidden" aria-expanded={mobileLibrary} onClick={() => setMobileLibrary(!mobileLibrary)}>
            <LibraryBig /> My papers
          </Button>
        </div>
        <div className={cn("min-h-0 flex-1 flex-col gap-2 md:flex", mobileLibrary ? "flex" : "hidden")}>
          <PaperLibrary library={library} failed={listFailed} activeId={activeId} select={select}
            reload={loadList} expired={() => onExpired("the library could not be updated")} />
        </div>
        <div className="flex items-center gap-2 border-t pt-3">
          <span aria-hidden className="flex size-8 shrink-0 items-center justify-center rounded-full bg-primary text-sm text-primary-foreground">
            {user.username[0].toUpperCase()}
          </span>
          <span className="min-w-0 flex-1 truncate text-sm">{user.username}</span>
          <ThemeButton />
          <Button variant="ghost" size="icon-sm" title="Sign out" aria-label="Sign out" onClick={onLogout}>
            <LogOut />
          </Button>
        </div>
      </aside>
      <main className="min-w-0 flex-1 overflow-y-auto">
        <div className={cn("mx-auto grid gap-5 p-5 md:px-8 md:py-12", screen === "overview" ? "max-w-4xl" : "max-w-2xl")}>
          {activeId ? (
            <Practice key={activeId} paperId={activeId} onExpired={onExpired} onChanged={loadList} onHome={goHome} />
          ) : screen === "upload" ? (
            <UploadPaper onExpired={onExpired} onChanged={loadList} onOpen={select} />
          ) : library ? (
            <Overview library={library} open={select} upload={openUpload} />
          ) : (
            <p role="status">{listFailed ? <button className="underline" onClick={loadList}>Retry loading overview</button> : "Loading overview…"}</p>
          )}
        </div>
      </main>
    </div>
  )
}

const FORMAT_EXAMPLE = `{
  "format_version": 1,
  "title": "French Basics",
  "questions": [
    { "id": "q1", "type": "translation",
      "prompt": "Translate “thank you” into French.",
      "accepted_answers": ["Merci"],
      "correct_answer": "Merci.",
      "explanation": "A common way to say thanks." }
  ]
}`

type UploadStatus =
  | { kind: "saving" }
  | { kind: "saved"; id: string; title: string; count: number }
  | { kind: "uncertain" }
  | { kind: "error"; error: ApiError }

function UploadPaper(props: { onExpired: (what: string) => void; onChanged: () => void; onOpen: (id: string) => void }) {
  const { onExpired, onChanged, onOpen } = props
  const input = useRef<HTMLInputElement>(null)
  const [file, setFile] = useState<File | null>(null)
  const [status, setStatus] = useState<UploadStatus | null>(null)
  const [over, setOver] = useState(false)

  function choose(f: File | undefined) {
    setFile(f ?? null)
    setStatus(null)
  }

  async function upload() {
    if (!file) return
    setStatus({ kind: "saving" })
    let text
    try {
      text = await file.text()
    } catch {
      return setStatus({ kind: "error", error: { code: "read", message: "Couldn't read the local file. Please choose it again." } })
    }
    const r = await api<{ paper: { id: string; title: string; question_count: number } }>("POST", "/api/papers", text)
    if (r.ok) {
      const p = r.data.paper
      if (input.current) input.current.value = ""
      setFile(null)
      setStatus({ kind: "saved", id: p.id, title: p.title, count: p.question_count })
      onChanged()
    } else if (r.status === 401) {
      onExpired("the paper was not uploaded")
    } else if (r.uncertain) {
      setStatus({ kind: "uncertain" })
    } else {
      setStatus({ kind: "error", error: { ...r.error, message: `Not imported: ${r.error.message}` } })
    }
  }

  return (
    <>
      <h1 className="text-[1.75rem] leading-tight font-medium tracking-tight sm:text-[2.125rem]">Upload a paper</h1>
      <p className="text-muted-foreground">
        Upload a JSON paper you prepared elsewhere. Grace saves it to the cloud so you can keep practicing on any device.
      </p>
      <label
        className={cn(
          "flex cursor-pointer flex-col items-center gap-1 rounded-xl border border-dashed p-8 text-center text-sm transition-colors hover:bg-accent/50 has-focus-visible:ring-[3px] has-focus-visible:ring-ring/50",
          over && "border-primary bg-accent"
        )}
        onDragOver={(e) => {
          e.preventDefault()
          setOver(true)
        }}
        onDragLeave={() => setOver(false)}
        onDrop={(e) => {
          e.preventDefault()
          setOver(false)
          choose(e.dataTransfer.files[0])
        }}
      >
        <input
          ref={input}
          type="file"
          accept=".json,application/json"
          className="sr-only"
          onChange={(e) => choose(e.target.files?.[0])}
        />
        <Upload aria-hidden className="mb-1 size-6 text-muted-foreground" />
        <span>Click to choose or drop a .json file</span>
        <span className="text-xs text-muted-foreground">Each upload creates a new paper; 1 MiB max</span>
      </label>
      {file && (
        <Card className="py-3">
          <CardContent className="flex items-center gap-3 px-4">
            <FileJson aria-hidden className="size-5 shrink-0 text-muted-foreground" />
            <span className="min-w-0 flex-1 truncate text-sm">
              {file.name}
              <span className="text-xs text-muted-foreground"> · {(file.size / 1024).toFixed(1)} KB · not uploaded yet</span>
            </span>
            <Button disabled={status?.kind === "saving"} onClick={upload}>
              {status?.kind === "saving" ? "Saving…" : "Upload"}
            </Button>
          </CardContent>
        </Card>
      )}
      {status?.kind === "saving" && (
        <p className="text-sm text-muted-foreground">Uploading and saving to the cloud. Please keep this page open.</p>
      )}
      {status?.kind === "saved" && (
        <Notice kind="success" message={`Saved to the cloud: “${status.title}” (${status.count} questions)`}>
          <Button size="sm" onClick={() => onOpen(status.id)}>
            Start practicing
          </Button>
        </Notice>
      )}
      {status?.kind === "uncertain" && (
        <Notice
          kind="error"
          message="We couldn't confirm whether the paper was saved. Check the list first; uploading again creates a separate new paper."
        >
          <Button variant="outline" size="sm" onClick={onChanged}>
            Refresh list
          </Button>
        </Notice>
      )}
      {status?.kind === "error" && (
        <Notice kind="error" message={status.error.message} details={status.error.details} />
      )}
      <details className="text-sm">
        <summary className="cursor-pointer text-muted-foreground">Paper file format</summary>
        <p className="mt-2 text-muted-foreground">
          The root object has format_version (1), title, an optional description, and a questions array. choice
          questions need options and correct_option_id; translation and sentence questions need accepted_answers.
          Every question needs id, prompt, correct_answer and explanation.
        </p>
        <pre className="mt-2 overflow-x-auto rounded-lg bg-muted p-3 text-xs">{FORMAT_EXAMPLE}</pre>
      </details>
    </>
  )
}
