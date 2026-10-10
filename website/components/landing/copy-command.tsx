"use client";
import { Check, Copy } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

// Shared by the getting-started section and the last stage of the atlas tour.
export const setupCommands = [
  { command: "mct-cli --root . init", label: "initLabel" },
  { command: "mct-cli --root . mcp-register", label: "registerLabel" },
  { command: "mct-cli --root . status", label: "statusLabel" },
] as const;
export function CopyCommand({
  command,
  label,
}: {
  command: string;
  label: string;
}) {
  const { t } = useTranslation();
  const [feedback, setFeedback] = useState<"copied" | "copyError" | null>(null);
  async function copy() {
    try {
      await navigator.clipboard.writeText(command);
      setFeedback("copied");
    } catch {
      setFeedback("copyError");
    }
  }
  return (
    <div className="command-row">
      <div>
        <span>{label}</span>
        <code>{command}</code>
      </div>
      <button
        type="button"
        onClick={copy}
        aria-label={`${t("copy")}: ${command}`}
      >
        {feedback === "copied" ? (
          <Check size={18} aria-hidden />
        ) : (
          <Copy size={18} aria-hidden />
        )}
      </button>
      <p role="status">{feedback && t(feedback)}</p>
    </div>
  );
}
