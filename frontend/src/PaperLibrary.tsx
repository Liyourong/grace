import { useEffect, useRef, useState, type ReactNode } from "react"
import {
  ChevronDown,
  ChevronRight,
  Ellipsis,
  Folder,
  FolderOpen,
  FolderPlus,
} from "lucide-react"
import { DropdownMenu } from "radix-ui"

import { api, type FolderSummary, type Library, type PaperSummary } from "@/api"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"

type Action =
  | { kind: "create" }
  | { kind: "rename" | "deleteFolder"; folder: FolderSummary }
  | { kind: "move" | "deletePaper"; paper: PaperSummary }

const titles = {
  create: "New folder",
  rename: "Rename folder",
  deleteFolder: "Delete folder",
  move: "Move to…",
  deletePaper: "Delete paper",
}
const itemStyle =
  "cursor-pointer rounded px-2 py-1.5 text-sm outline-none focus:bg-accent"

function Actions({ name, children }: { name: string; children: ReactNode }) {
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <Button
          aria-label={`Actions for ${name}`}
          title={`Actions for ${name}`}
          variant="ghost"
          size="icon-sm"
        >
          <Ellipsis />
        </Button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          align="end"
          className="z-50 min-w-36 rounded-md border bg-popover p-1 text-popover-foreground shadow-md"
        >
          {children}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  )
}

function LibraryDialog({
  action,
  folders,
  close,
  saved,
  expired,
  reload,
}: {
  action: Action
  folders: FolderSummary[]
  close: () => void
  saved: () => void
  expired: () => void
  reload: () => void
}) {
  const dialog = useRef<HTMLDialogElement>(null)
  const [name, setName] = useState(
    action.kind === "rename" ? action.folder.name : ""
  )
  const [destination, setDestination] = useState(
    action.kind === "move" ? (action.paper.folder_id ?? "") : ""
  )
  const [pending, setPending] = useState(false)
  const [error, setError] = useState("")
  useEffect(() => {
    const element = dialog.current!
    element.showModal()
    return () => element.close()
  }, [])
  const deleting =
    action.kind === "deletePaper" || action.kind === "deleteFolder"
  const naming = action.kind === "create" || action.kind === "rename"
  async function submit() {
    if (pending) return
    setPending(true)
    let result
    switch (action.kind) {
      case "create":
        result = await api("POST", "/api/folders", { name })
        break
      case "rename":
        result = await api("PATCH", "/api/folders/" + action.folder.id, {
          name,
        })
        break
      case "move":
        result = await api("PATCH", "/api/papers/" + action.paper.id, {
          folder_id: destination || null,
        })
        break
      case "deleteFolder":
        result = await api("DELETE", "/api/folders/" + action.folder.id)
        break
      case "deletePaper":
        result = await api("DELETE", "/api/papers/" + action.paper.id)
        break
    }
    if (result.ok) return saved()
    if (result.status === 401) return expired()
    if (result.uncertain || result.status === 404) reload()
    setError(
      result.uncertain
        ? "Check the library before trying again. The change may have been saved."
        : result.error.message
    )
    setPending(false)
  }
  return (
    <dialog
      ref={dialog}
      aria-labelledby="library-dialog-title"
      onCancel={(e) => {
        e.preventDefault()
        if (!pending) close()
      }}
      className="fixed inset-0 m-auto w-[min(24rem,90vw)] rounded-xl border bg-background p-5 text-foreground shadow-xl backdrop:bg-black/40"
    >
      <form
        onSubmit={(e) => {
          e.preventDefault()
          void submit()
        }}
        className="grid gap-4"
      >
        <h2 id="library-dialog-title" className="text-lg font-semibold">
          {titles[action.kind]}
        </h2>
        {naming && (
          <label className="grid gap-2 text-sm">
            Folder name
            <Input
              autoFocus
              required
              maxLength={80}
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
        )}
        {action.kind === "move" && (
          <label className="grid gap-2 text-sm">
            Destination for “{action.paper.title}”
            <select
              value={destination}
              onChange={(e) => setDestination(e.target.value)}
              className="rounded-md border bg-background p-2"
            >
              <option value="">My papers (root)</option>
              {folders.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.name}
                </option>
              ))}
            </select>
          </label>
        )}
        {action.kind === "deleteFolder" && (
          <p>
            Delete “{action.folder.name}”? Its papers will move to My papers
            with their progress.
          </p>
        )}
        {action.kind === "deletePaper" && (
          <p>
            Permanently delete “{action.paper.title}” and its saved answers and
            progress? This cannot be undone.
          </p>
        )}
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
        <div className="flex justify-end gap-2">
          <Button
            type="button"
            autoFocus={deleting}
            variant="outline"
            disabled={pending}
            onClick={close}
          >
            Cancel
          </Button>
          <Button
            type="submit"
            variant={deleting ? "destructive" : "default"}
            disabled={pending || (naming && !name.trim())}
          >
            {deleting
              ? titles[action.kind]
              : action.kind === "move"
                ? "Move paper"
                : "Save"}
          </Button>
        </div>
      </form>
    </dialog>
  )
}

export function PaperLibrary({
  library,
  failed,
  activeId,
  select,
  reload,
  expired,
}: {
  library: Library | null
  failed: boolean
  activeId: string | null
  select: (id: string | null) => void
  reload: () => void
  expired: () => void
}) {
  const [action, setAction] = useState<Action | null>(null)
  const [closed, setClosed] = useState<string[]>([])
  function paperRow(p: PaperSummary) {
    return (
      <div key={p.id} className="flex items-center">
        <Button
          variant={p.id === activeId ? "secondary" : "ghost"}
          aria-current={p.id === activeId ? "page" : undefined}
          title={p.title}
          className="h-auto min-w-0 flex-1 justify-start px-2 py-2 text-left"
          onClick={() => select(p.id)}
        >
          <span className="line-clamp-2 leading-snug font-medium wrap-anywhere whitespace-normal">{p.title}</span>
        </Button>
        <Actions name={p.title}>
          <DropdownMenu.Item
            className={itemStyle}
            onSelect={() => setAction({ kind: "move", paper: p })}
          >
            Move to…
          </DropdownMenu.Item>
          <DropdownMenu.Item
            className={itemStyle}
            onSelect={() => setAction({ kind: "deletePaper", paper: p })}
          >
            Delete paper
          </DropdownMenu.Item>
        </Actions>
      </div>
    )
  }
  return (
    <>
      <div className="flex items-center justify-between px-2 text-xs font-medium text-muted-foreground">
        My papers
        <Button
          size="sm"
          variant="ghost"
          disabled={!library}
          onClick={() => setAction({ kind: "create" })}
        >
          <FolderPlus />
          New folder
        </Button>
      </div>
      <nav
        aria-label="My papers"
        className="flex max-h-48 flex-col gap-1 overflow-y-auto md:max-h-none md:flex-1"
      >
        {failed && (
          <p role="alert" className="px-2 text-sm">
            Couldn’t refresh papers.{" "}
            <button onClick={reload} className="underline">
              Retry
            </button>
          </p>
        )}
        {!library && !failed && <p className="px-2 text-sm">Loading…</p>}
        {library?.folders.map((f) => {
          const expanded = !closed.includes(f.id)
          const papers = library.papers.filter((p) => p.folder_id === f.id)
          return (
            <div key={f.id}>
              <div className="flex items-center">
                <Button
                  variant="ghost"
                  aria-expanded={expanded}
                  className="min-w-0 flex-1 justify-start px-1"
                  onClick={() =>
                    setClosed(
                      expanded
                        ? [...closed, f.id]
                        : closed.filter((id) => id !== f.id)
                    )
                  }
                >
                  {expanded ? <ChevronDown /> : <ChevronRight />}
                  {expanded ? <FolderOpen /> : <Folder />}
                  <span className="truncate">{f.name}</span>
                </Button>
                <Actions name={f.name}>
                  <DropdownMenu.Item
                    className={itemStyle}
                    onSelect={() => setAction({ kind: "rename", folder: f })}
                  >
                    Rename folder
                  </DropdownMenu.Item>
                  <DropdownMenu.Item
                    className={itemStyle}
                    onSelect={() =>
                      setAction({ kind: "deleteFolder", folder: f })
                    }
                  >
                    Delete folder
                  </DropdownMenu.Item>
                </Actions>
              </div>
              {expanded && (
                <div className="ml-4 border-l pl-2">
                  {papers.length ? (
                    papers.map(paperRow)
                  ) : (
                    <p className="p-2 text-xs text-muted-foreground">
                      Empty folder
                    </p>
                  )}
                </div>
              )}
            </div>
          )
        })}
        {library?.papers.filter((p) => p.folder_id === null).map(paperRow)}
        {library && !library.papers.length && (
          <p className="px-2 text-sm text-muted-foreground">No papers yet</p>
        )}
      </nav>
      {action && library && (
        <LibraryDialog
          action={action}
          folders={library.folders}
          expired={expired}
          reload={reload}
          close={() => setAction(null)}
          saved={() => {
            if (action.kind === "deletePaper" && activeId === action.paper.id)
              select(null)
            setAction(null)
            reload()
          }}
        />
      )}
    </>
  )
}
