import type { ReactNode } from "react"
import { CircleAlert, CircleCheck, TriangleAlert } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"

const ICONS = { error: CircleAlert, warn: TriangleAlert, success: CircleCheck }

export function Notice(props: {
  kind: keyof typeof ICONS
  message: string
  details?: string[]
  children?: ReactNode
}) {
  const { kind, message, details, children } = props
  const Icon = ICONS[kind]
  return (
    <Alert variant={kind === "error" ? "destructive" : "default"} role={kind === "error" ? "alert" : "status"}>
      <Icon />
      <AlertTitle className="line-clamp-none">{message}</AlertTitle>
      {details?.length || children ? (
        <AlertDescription>
          {details?.length ? (
            <ul className="list-disc pl-4">
              {details.map((d, i) => (
                <li key={i}>{d}</li>
              ))}
            </ul>
          ) : null}
          {children}
        </AlertDescription>
      ) : null}
    </Alert>
  )
}
