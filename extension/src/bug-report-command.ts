// One-line /bug-report command. Pi's own /bug uploads to the pi developers and
// cannot be overridden by an extension, so free-pi exposes its own command
// instead of pretending to replace the built-in one.
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { buildBugRequest, sendBugReport } from "./bug-report";

export function registerBugReportCommand(
  pi: ExtensionAPI,
  opts: { baseUrl: string; getToken: () => string | Promise<string> },
): void {
  pi.registerCommand("bug-report", {
    description: "Report a bug to free-pi (text only, no files attached).",
    handler: async (args: string, ctx: ExtensionContext) => {
      const description = args.trim();
      if (!description) {
        ctx.ui.notify("Usage: /bug-report <what went wrong>", "info");
        return;
      }
      const result = await sendBugReport(opts, buildBugRequest(description, {
        sessionId: ctx.sessionManager.getSessionId(),
        model: ctx.model?.id,
      }));
      if (result.ok) {
        ctx.ui.notify(`Thanks — bug report received (#${result.id}).`, "info");
      } else {
        ctx.ui.notify(result.message, "error");
      }
    },
  });
}
