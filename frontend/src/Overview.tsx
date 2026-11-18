import { ArrowRight, BookOpen, CheckCheck, Clock3, Upload } from "lucide-react"
import type { Library, PaperSummary } from "@/api"
import { Button } from "@/components/ui/button"

function duration(seconds: number) {
  if (seconds < 60) return seconds + "s"
  const minutes = Math.floor(seconds / 60)
  return minutes < 60 ? minutes + "m" : Math.floor(minutes / 60) + "h " + minutes % 60 + "m"
}

export function Overview({ library, open, upload }: {
  library: Library; open: (id: string) => void; upload: () => void
}) {
  const { papers } = library
  let latest: PaperSummary | undefined
  let answered = 0
  for (const paper of papers) {
    if (paper.last_activity_at && (!latest || paper.last_activity_at > latest.last_activity_at!)) latest = paper
    answered += paper.stats.answered
  }
  const paper = latest ?? papers.find(p => p.is_default) ?? papers[0]
  const metrics = [
    { label: "Study time", value: duration(library.study_seconds), detail: "Active practice time", Icon: Clock3 },
    { label: "Questions answered", value: answered, detail: "Across your papers", Icon: CheckCheck },
  ]
  return <>
    <h1 className="text-[1.75rem] leading-tight font-medium tracking-tight sm:text-[2.125rem]">Learning overview</h1>
    <section aria-label="Practice" className="relative isolate overflow-hidden rounded-2xl border border-teal-900 bg-[#123d3a] p-6 text-white sm:p-9">
      <div aria-hidden="true" className="pointer-events-none absolute inset-y-0 right-0 -z-10 w-1/2 opacity-25 [mask-image:linear-gradient(90deg,transparent,black)] [background-image:radial-gradient(#a7f3d0_1px,transparent_1px)] [background-size:12px_12px]" />
      <div className="mb-5 flex items-center gap-2 text-sm font-medium text-teal-100"><BookOpen size={17} />{latest ? "Continue practice" : "Start practicing"}</div>
      <h2 className="font-reading text-[clamp(2.25rem,5vw,3.25rem)] leading-[1.12] font-normal tracking-tight text-balance wrap-anywhere">{paper?.title ?? "Add your first paper"}</h2>
      {paper && <div className="mt-3 text-sm text-teal-50">
        Question {paper.current_question_number} of {paper.stats.total} · {paper.stats.answered} answered
        <progress aria-label="Paper progress" max={paper.stats.total} value={paper.stats.answered}
          className="mt-3 block h-1.5 w-full max-w-60 overflow-hidden rounded-full border-0 bg-white/20 [&::-moz-progress-bar]:bg-teal-200 [&::-webkit-progress-bar]:bg-transparent [&::-webkit-progress-value]:bg-teal-200" />
      </div>}
      <Button onClick={() => paper ? open(paper.id) : upload()} className="mt-7 bg-teal-100 text-teal-950 hover:bg-white">
        {paper ? <>{latest ? "Continue" : "Start practicing"}<ArrowRight /></> : <><Upload />Upload a paper</>}
      </Button>
    </section>
    <dl className="grid overflow-hidden rounded-2xl border bg-card sm:grid-cols-2">
      {metrics.map(({ label, value, detail, Icon }) => <div key={label} className="border-b px-5 py-5 last:border-0 sm:border-r sm:border-b-0">
        <dt className="flex items-center gap-2 text-sm font-medium"><Icon size={16} className="text-teal-700 dark:text-teal-300" />{label}</dt>
        <dd className="my-2 text-[2rem] leading-tight font-medium tracking-tight tabular-nums">{value}</dd>
        <dd className="text-xs text-muted-foreground">{detail}</dd>
      </div>)}
    </dl>
  </>
}
