import { toast } from "sonner";

// Short confirmations; anything the person must act on is also shown inline.
export const ok = (message: string) => toast.success(message, { duration: 3500 });
export const err = (message: string) => toast.error(message, { duration: 6000 });

export async function copyText(text: string, done: string, failed: string) {
  try {
    await navigator.clipboard.writeText(text);
    ok(done);
  } catch {
    err(failed);
  }
}
