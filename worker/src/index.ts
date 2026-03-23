import { Hono, type Context } from "hono";
import {
  cfApiTokenAuth,
  ConfigurationError,
  readAccountId,
  type AuthVariables,
  type Env,
} from "./auth";
import {
  deleteConfigRecord,
  extractConfigId,
  extractRelativePath,
  getConfigManifest,
  listConfigRecords,
  RequestValidationError,
  saveConfigRecord,
  type ConfigUploadPayload,
} from "./config-store";

type AppContext = {
  Bindings: Env;
  Variables: AuthVariables;
};

const app = new Hono<AppContext>();
const protectedApi = new Hono<AppContext>();

app.get("/", (c) => {
  return c.json({
    service: "sync-devices-worker",
    version: "0.2.0",
    auth_method: "cf_api_token",
    kv_bound: !!c.env.SYNC_CONFIGS,
  });
});

app.get("/healthz", (c) => {
  return c.text("ok");
});

protectedApi.use("*", cfApiTokenAuth);

protectedApi.get("/configs", async (c) => {
  try {
    const ownerSubject = readAccountId(c);
    const toolFilter = c.req.query("tool");
    const categoryFilter = c.req.query("category");
    const items = await listConfigRecords(c.env, ownerSubject, {
      tool: toolFilter,
      category: categoryFilter,
    });

    return jsonResponse(
      {
        items,
        filters: {
          tool: toolFilter ?? null,
          category: categoryFilter ?? null,
        },
      },
      200,
    );
  } catch (error) {
    return handleApiError(error);
  }
});

protectedApi.get("/manifest", async (c) => {
  try {
    const ownerSubject = readAccountId(c);
    const manifest = await getConfigManifest(c.env, ownerSubject);
    return jsonResponse(manifest, 200);
  } catch (error) {
    return handleApiError(error);
  }
});

protectedApi.put("/configs/:tool/:category/*", async (c) => {
  try {
    const ownerSubject = readAccountId(c);
    const tool = c.req.param("tool");
    const category = c.req.param("category");

    let payload: ConfigUploadPayload;
    try {
      payload = (await c.req.json()) as ConfigUploadPayload;
    } catch {
      throw new RequestValidationError(400, {
        error: "invalid_request",
        error_description: "Expected a JSON request body.",
      });
    }

    const relPath = extractRelativePath(
      new URL(c.req.url).pathname,
      tool,
      category,
    );
    const item = await saveConfigRecord(
      c.env,
      ownerSubject,
      tool,
      category,
      relPath,
      payload,
    );
    return jsonResponse({ item }, 201);
  } catch (error) {
    return handleApiError(error);
  }
});

protectedApi.delete("/configs/:id", async (c) => {
  try {
    const ownerSubject = readAccountId(c);
    const configId = extractConfigId(new URL(c.req.url).pathname);
    const item = await deleteConfigRecord(c.env, ownerSubject, configId);
    return jsonResponse({ item }, 200);
  } catch (error) {
    return handleApiError(error);
  }
});

app.route("/api", protectedApi);

export default app;

function handleApiError(error: unknown): Response {
  if (error instanceof RequestValidationError) {
    return jsonResponse(error.body, error.status);
  }

  if (error instanceof ConfigurationError) {
    return jsonResponse(
      {
        error: "server_not_configured",
        error_description: error.message,
      },
      500,
    );
  }

  console.error("Unhandled API error", error);
  return jsonResponse(
    {
      error: "internal_error",
      error_description: "Unexpected API failure.",
    },
    500,
  );
}

function jsonResponse(body: unknown, status: number): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "content-type": "application/json; charset=UTF-8",
    },
  });
}
