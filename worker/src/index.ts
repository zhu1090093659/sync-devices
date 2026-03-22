import { Hono, type Context } from "hono";
import {
  ConfigurationError,
  UpstreamRequestError,
  exchangeDeviceCode,
  fetchAuthenticatedUser,
  requestDeviceCode,
  type Env,
} from "./github-oauth";
import {
  issueSessionToken,
  jwtAuthMiddleware,
  readSessionUser,
  type AuthVariables,
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
    status: "ok",
  });
});

app.get("/healthz", (c) => {
  return c.text("ok");
});

app.post("/api/auth/device/code", async (c) => {
  try {
    const result = await requestDeviceCode(c.env);
    return jsonResponse(result.body, result.status);
  } catch (error) {
    return handleApiError(error);
  }
});

app.post("/api/auth/device/token", async (c) => {
  const deviceCode = await readDeviceCode(c);
  if (!deviceCode) {
    return jsonResponse(
      {
        error: "invalid_request",
        error_description:
          "Expected a JSON body with a non-empty device_code field.",
      },
      400,
    );
  }

  try {
    const result = await exchangeDeviceCode(c.env, deviceCode);
    if (!hasGitHubAccessToken(result.body)) {
      return jsonResponse(result.body, result.status);
    }

    const user = await fetchAuthenticatedUser(result.body.access_token);
    const session = await issueSessionToken(
      c.env,
      user,
      result.body.scope ?? "",
    );
    return jsonResponse(session, 200);
  } catch (error) {
    return handleApiError(error);
  }
});

protectedApi.use("*", jwtAuthMiddleware);
protectedApi.get("/session", (c) => {
  const user = readSessionUser(c);
  if (!user) {
    return jsonResponse(
      {
        error: "invalid_token",
        error_description: "The JWT payload is missing required user claims.",
      },
      401,
    );
  }

  const payload = c.get("jwtPayload");
  return jsonResponse(
    {
      user,
      token: {
        issuer: getStringClaim(payload, "iss"),
        subject: getStringClaim(payload, "sub"),
        issued_at: getNumberClaim(payload, "iat"),
        expires_at: getNumberClaim(payload, "exp"),
      },
    },
    200,
  );
});

protectedApi.get("/configs", async (c) => {
  try {
    const ownerSubject = requireOwnerSubject(c);
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
    const ownerSubject = requireOwnerSubject(c);
    const manifest = await getConfigManifest(c.env, ownerSubject);
    return jsonResponse(manifest, 200);
  } catch (error) {
    return handleApiError(error);
  }
});

protectedApi.put("/configs/:tool/:category/*", async (c) => {
  try {
    const ownerSubject = requireOwnerSubject(c);
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
    const ownerSubject = requireOwnerSubject(c);
    const configId = extractConfigId(new URL(c.req.url).pathname);
    const item = await deleteConfigRecord(c.env, ownerSubject, configId);
    return jsonResponse({ item }, 200);
  } catch (error) {
    return handleApiError(error);
  }
});

app.route("/api", protectedApi);

export default app;

async function readDeviceCode(c: Context<AppContext>): Promise<string | null> {
  let payload: unknown;

  try {
    payload = await c.req.json();
  } catch {
    return null;
  }

  if (!payload || typeof payload !== "object") {
    return null;
  }

  const deviceCode = (payload as { device_code?: unknown }).device_code;
  if (typeof deviceCode !== "string") {
    return null;
  }

  const trimmed = deviceCode.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function requireOwnerSubject(c: Context<AppContext>): string {
  const subject = getStringClaim(c.get("jwtPayload"), "sub");
  if (!subject) {
    throw new RequestValidationError(401, {
      error: "invalid_token",
      error_description: "The JWT payload is missing the subject claim.",
    });
  }
  return subject;
}

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

  if (error instanceof UpstreamRequestError) {
    return jsonResponse(error.body, error.status);
  }

  console.error("Unhandled authentication route error", error);
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

function hasGitHubAccessToken(
  body: unknown,
): body is { access_token: string; scope?: string } {
  if (!body || typeof body !== "object") {
    return false;
  }

  const accessToken = Reflect.get(body, "access_token");
  return typeof accessToken === "string" && accessToken.trim().length > 0;
}

function getStringClaim(payload: unknown, key: string): string | null {
  if (!payload || typeof payload !== "object") {
    return null;
  }

  const value = Reflect.get(payload, key);
  return typeof value === "string" ? value : null;
}

function getNumberClaim(payload: unknown, key: string): number | null {
  if (!payload || typeof payload !== "object") {
    return null;
  }

  const value = Reflect.get(payload, key);
  return typeof value === "number" ? value : null;
}
