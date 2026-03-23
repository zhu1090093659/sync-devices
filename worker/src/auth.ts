import type { Context, MiddlewareHandler } from "hono";

const CF_TOKEN_VERIFY_URL =
  "https://api.cloudflare.com/client/v4/user/tokens/verify";

export interface KVNamespaceLike {
  get<T>(key: string, type: "json"): Promise<T | null>;
  put(
    key: string,
    value: string,
    options?: { metadata?: Record<string, unknown> },
  ): Promise<void>;
  delete(key: string): Promise<void>;
  list(options?: {
    prefix?: string;
    cursor?: string;
  }): Promise<ListResult>;
}

export interface ListKey {
  name: string;
  metadata?: Record<string, unknown> | null;
}

export interface ListResult {
  keys: ListKey[];
  list_complete: boolean;
  cursor?: string;
}

export interface Env {
  SYNC_CONFIGS?: KVNamespaceLike;
}

export interface AuthVariables {
  accountId: string;
}

interface CfTokenVerifySuccess {
  result: {
    id: string;
    status: string;
  };
  success: true;
  messages: Array<{ message: string }>;
}

interface CfTokenVerifyFailure {
  success: false;
  errors: Array<{ code: number; message: string }>;
}

type CfTokenVerifyResponse = CfTokenVerifySuccess | CfTokenVerifyFailure;

export class ConfigurationError extends Error {}

export const cfApiTokenAuth: MiddlewareHandler<{
  Bindings: Env;
  Variables: AuthVariables;
}> = async (c, next) => {
  const header = c.req.header("Authorization");
  if (!header || !header.startsWith("Bearer ")) {
    return unauthorizedResponse("Missing or malformed Authorization header.");
  }

  const token = header.slice("Bearer ".length).trim();
  if (!token) {
    return unauthorizedResponse("Bearer token is empty.");
  }

  const response = await fetch(CF_TOKEN_VERIFY_URL, {
    method: "GET",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
  });

  let body: CfTokenVerifyResponse;
  try {
    body = (await response.json()) as CfTokenVerifyResponse;
  } catch {
    return unauthorizedResponse("Cloudflare token verification returned an invalid response.");
  }

  if (!body.success) {
    const msg = body.errors?.[0]?.message ?? "Token verification failed.";
    return unauthorizedResponse(msg);
  }

  if (body.result.status !== "active") {
    return unauthorizedResponse(`Token status is '${body.result.status}', expected 'active'.`);
  }

  c.set("accountId", body.result.id);
  await next();
};

export function readAccountId(
  c: Context<{ Bindings: Env; Variables: AuthVariables }>,
): string {
  return c.get("accountId");
}

function unauthorizedResponse(description: string): Response {
  return new Response(
    JSON.stringify({
      error: "unauthorized",
      error_description: description,
    }),
    {
      status: 401,
      headers: { "content-type": "application/json; charset=UTF-8" },
    },
  );
}
