// free-pi's pi extension bundle. The Rust CLI materializes this file under
// ~/.free-pi/agent/extensions/free-pi.js and starts pi with
// PI_CODING_AGENT_DIR pointed at the same agent directory.
//
// Everything that used to be an InlineExtension in the SDK build is registered
// here from the extension entry point. The provider itself comes from
// models.json (apiKey "$FREEPI_JWT"); this bundle adds the per-session headers,
// ads, tools, commands, Fusion, and the branded header.
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { homedir } from "node:os";
import { join } from "node:path";
import { createAdsExtension } from "@freepi/pi-ads";
import { createBuyToolExtension, BUY_TOOL_NAME } from "./buy-tool";
import { createDocsToolExtension, DOCS_TOOL_NAME } from "./docs-tool";
import { createErrorNoticeExtension } from "./error-notice";
import { createFusionExtension, resolveFusionIds } from "./fusion";
import { startGravityPixel } from "./gravity-pixel";
import { createHeaderExtension } from "./header";
import { createImageToolExtension, IMAGE_TOOL_NAME } from "./image-tool";
import { createFreePiCommandsExtension } from "./commands";
import { createTodoToolExtension, TODO_TOOL_NAME } from "./todo-tool";
import { createToolAliasesExtension } from "./tool-aliases";
import { createToolGuardExtension } from "./tool-guard";
import { createUsageToolExtension, USAGE_TOOL_NAME } from "./usage-tool";
import { createWebToolsExtension, DOCS_LOOKUP_TOOL_NAME, WEB_SEARCH_TOOL_NAME } from "./web-tools";
import { registerBugReportCommand } from "./bug-report-command";
import { CLI_VERSION } from "./version";

interface CatalogModel {
  id: string;
  name: string;
}

const SAFE_TOOLS = ["read", "bash", "edit", "write", "grep", "find", "ls"] as const;

const ALLOWED_TOOL_NAMES: readonly string[] = [
  ...SAFE_TOOLS,
  USAGE_TOOL_NAME,
  BUY_TOOL_NAME,
  DOCS_TOOL_NAME,
  WEB_SEARCH_TOOL_NAME,
  DOCS_LOOKUP_TOOL_NAME,
  IMAGE_TOOL_NAME,
  TODO_TOOL_NAME,
  "codemode",
  "tool_search",
];

function parseCatalog(raw: string | undefined): CatalogModel[] {
  if (!raw) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((m): m is CatalogModel => Boolean(m) && typeof m === "object" && typeof (m as CatalogModel).id === "string")
      .map((m) => ({ id: m.id, name: typeof m.name === "string" && m.name ? m.name : m.id }));
  } catch {
    return [];
  }
}

function parseFusion(raw: string | undefined): { planner: string; executor: string } | undefined {
  if (!raw) return undefined;
  try {
    const parsed = JSON.parse(raw) as { planner?: unknown; executor?: unknown };
    if (typeof parsed?.planner === "string" && typeof parsed?.executor === "string") {
      return { planner: parsed.planner, executor: parsed.executor };
    }
  } catch {
    // fall through
  }
  return undefined;
}

export default function freePiExtension(pi: ExtensionAPI): void {
  const env = process.env;
  const baseUrl = (env.FREEPI_BASE_URL ?? "https://api.freepi.ai").replace(/\/+$/, "");
  const jwt = env.FREEPI_JWT ?? "";
  const getToken = () => jwt;
  const sessionId = env.FREEPI_SESSION_ID ?? "";
  const models = parseCatalog(env.FREEPI_MODELS_JSON);
  const defaultModelId = models[0]?.id ?? "deepseek-v4-flash";
  const modelName = models[0]?.name ?? defaultModelId;
  const updateLatest = env.FREEPI_UPDATE_LATEST;
  const gravityDir = env.FREEPI_GRAVITY_DIR ?? join(homedir(), ".free-pi", "gravity");

  // The pixel must never be able to stop startup: it is optional and the
  // getter simply returns undefined until a Gravity session exists.
  let getGravityContext: (() => string | undefined) | undefined;
  try {
    if (env.FREEPI_GRAVITY_ENABLED === "1") {
      getGravityContext = startGravityPixel(jwt, gravityDir);
    }
  } catch {
    getGravityContext = undefined;
  }

  // First catalog row is what /model starts on; Fusion is listed first in the
  // scoped picker by settings.json but routes through these physical models.
  createFusionExtension(defaultModelId, resolveFusionIds(models.map((m) => m.id), parseFusion(env.FREEPI_FUSION_JSON))).factory(pi);

  let advertiserUrl: string | undefined;
  createAdsExtension({
    baseUrl,
    getToken,
    sessionId,
    getGravityContext,
    onBannerAd: (url) => {
      advertiserUrl = url;
    },
  }).factory(pi);

  createUsageToolExtension({ baseUrl, getToken }).factory(pi);
  createBuyToolExtension({ baseUrl, getToken }).factory(pi);
  createDocsToolExtension().factory(pi);
  createWebToolsExtension({ baseUrl, getToken }).factory(pi);
  createImageToolExtension({ baseUrl, getToken }).factory(pi);
  createErrorNoticeExtension().factory(pi);
  createToolGuardExtension({ baseUrl, getToken, allowedTools: ALLOWED_TOOL_NAMES }).factory(pi);
  createFreePiCommandsExtension({
    baseUrl,
    getToken,
    getAdvertiserUrl: () => advertiserUrl,
  }).factory(pi);
  createHeaderExtension({ modelName, updateLatest }).factory(pi);
  createTodoToolExtension().factory(pi);
  createToolAliasesExtension().factory(pi);
  registerBugReportCommand(pi, { baseUrl, getToken });

  // x-client-version is also written into models.json as a static header; the
  // session id must be read per request because /new and /resume can change it
  // mid-session. Pi's provider-auth layer keeps its own headers intact.
  pi.on("before_provider_headers", (event, ctx: ExtensionContext) => {
    event.headers["x-session-id"] = ctx.sessionManager.getSessionId();
    event.headers["x-client-version"] = CLI_VERSION;
  });
}
