import type { Context, MiddlewareHandler } from "hono";
import { jwt, sign, type JwtVariables } from "hono/jwt";
import { ConfigurationError, type Env, type GitHubAuthenticatedUser } from "./github-oauth";

const DEFAULT_JWT_ISSUER = "sync-devices-worker";
const DEFAULT_JWT_TTL_SECONDS = 60 * 60 * 24 * 7;

export type AuthVariables = JwtVariables;

export interface SessionTokenResponse {
  access_token: string;
  token_type: "bearer";
  expires_in: number;
  scope: string;
  user: SessionUser;
}

export interface SessionUser {
  id: number;
  login: string;
  name: string | null;
  avatar_url: string;
}

type SessionClaims = Parameters<typeof sign>[0] & {
  sub: string;
  provider: "github";
  login: string;
  name?: string;
  avatar_url: string;
  iat: number;
  exp: number;
  iss: string;
};

export async function issueSessionToken(
  env: Env,
  user: GitHubAuthenticatedUser,
  scope: string,
): Promise<SessionTokenResponse> {
  const now = Math.floor(Date.now() / 1000);
  const expiresIn = parseJwtTtlSeconds(env);
  const claims: SessionClaims = {
    sub: `github:${user.id}`,
    provider: "github",
    login: user.login,
    avatar_url: user.avatar_url,
    iat: now,
    exp: now + expiresIn,
    iss: resolveJwtIssuer(env),
  };

  if (user.name) {
    claims.name = user.name;
  }

  const accessToken = await sign(claims, requireJwtSecret(env), "HS256");
  return {
    access_token: accessToken,
    token_type: "bearer",
    expires_in: expiresIn,
    scope,
    user: toSessionUser(user),
  };
}

export const jwtAuthMiddleware: MiddlewareHandler<{
  Bindings: Env;
  Variables: AuthVariables;
}> = async (c, next) => {
  try {
    const middleware = jwt({
      secret: requireJwtSecret(c.env),
      alg: "HS256",
      verification: {
        iss: resolveJwtIssuer(c.env),
      },
    });

    return await middleware(c, next);
  } catch (error) {
    if (error instanceof ConfigurationError) {
      return jsonResponse(
        {
          error: "server_not_configured",
          error_description: error.message,
        },
        500,
      );
    }

    throw error;
  }
};

export function readSessionUser(
  c: Context<{ Bindings: Env; Variables: AuthVariables }>,
): SessionUser | null {
  const payload = c.get("jwtPayload");
  if (!payload || typeof payload !== "object") {
    return null;
  }

  const subject = getString(payload, "sub");
  const login = getString(payload, "login");
  const avatarUrl = getString(payload, "avatar_url");
  if (!subject || !login || !avatarUrl) {
    return null;
  }

  const id = parseGitHubUserId(subject);
  if (id === null) {
    return null;
  }

  return {
    id,
    login,
    name: getString(payload, "name"),
    avatar_url: avatarUrl,
  };
}

function toSessionUser(user: GitHubAuthenticatedUser): SessionUser {
  return {
    id: user.id,
    login: user.login,
    name: user.name,
    avatar_url: user.avatar_url,
  };
}

function parseJwtTtlSeconds(env: Env): number {
  const raw = env.JWT_TTL_SECONDS?.trim();
  if (!raw) {
    return DEFAULT_JWT_TTL_SECONDS;
  }

  const parsed = Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    throw new ConfigurationError("JWT_TTL_SECONDS must be a positive integer.");
  }

  return parsed;
}

function resolveJwtIssuer(env: Env): string {
  return env.JWT_ISSUER?.trim() || DEFAULT_JWT_ISSUER;
}

function requireJwtSecret(env: Env): string {
  const secret = env.JWT_SECRET?.trim();
  if (!secret) {
    throw new ConfigurationError("Missing JWT_SECRET binding.");
  }

  return secret;
}

function parseGitHubUserId(subject: string): number | null {
  if (!subject.startsWith("github:")) {
    return null;
  }

  const id = Number.parseInt(subject.slice("github:".length), 10);
  return Number.isFinite(id) ? id : null;
}

function getString(payload: object, key: string): string | null {
  const value = Reflect.get(payload, key);
  return typeof value === "string" ? value : null;
}

function jsonResponse(body: unknown, status: number): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "content-type": "application/json; charset=UTF-8",
    },
  });
}
