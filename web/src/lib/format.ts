import { toast } from "sonner"

export function formatTime(timestamp?: number) {
  return timestamp ? new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "medium" }).format(new Date(timestamp * 1000)) : "—"
}

export function showError(error: Error) {
  toast.error(error.message)
}
