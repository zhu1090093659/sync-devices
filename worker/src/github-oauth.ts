const DEVICE_CODE_URL = "https://github.com/login/device/code";
const ACCESS_TOKEN_URL = "https://github.com/login/oauth/access_token";
const GITHUB_USER_URL = "https://api.github.com/user";
const DEVICE_GRANT_TYPE = "urn:ietf:params:oauth:grant-type:device_code";

export interface KVNamespaceLike {
  get<T>(key: string, type: "json"): Promise<T | null>;
  put(key: string, value: string): Promise<void>;
  delete(key: string): Promise<void>;
  list(options?: { prefix?: string; cursor?: string }): Promise<{
    keys: Array<{ name: string }>;
    list_complete: boolean;
    cursor?: string;
  }>;
}

export interface Env {
  GITHUB_CLIENT_ID: string;
  GITHUB_DEVICE_SCOPE?: string;
  JWT_SECRET?: string;
  JWT_ISSUER?: string;
  JWT_TTL_SECONDS?: string;
  SYNC_CONFIGS?: KVNamespaceLike;
}

export interface GitHubDeviceCodeResponse {
  device_code: string;
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
}

export interface GitHubAccessTokenResponse {
  access_token?: string;
  token_type?: string;
  scope?: string;
  error?: string;
  error_description?: string;
  error_uri?: string;
  interval?: number;
}

export interface GitHubAuthenticatedUser {
  id: number;
  login: string;
  name: string | null;
  avatar_url: string;
}

export interface GitHubErrorResponse {
  error: string;
  error_description?: string;
  error_uri?: string;
  interval?: number;
}

interface GitHubProxyResponse<T> {
  status: number;
  body: T;
}

export class ConfigurationError extends Error {}

export class UpstreamRequestError extends Error {
  constructor(
    public readonly status: number,
    public readonly body: Record<string, unknown>,
  ) {
    super("GitHub OAuth request failed");
  }
}

export async function requestDeviceCode(
  env: Env,
): Promise<
  GitHubProxyResponse<GitHubDeviceCodeResponse | GitHubErrorResponse>
> {
  const form = new URLSearchParams({
    client_id: requireClientId(env),
  });

  const scope = env.GITHUB_DEVICE_SCOPE?.trim();
  if (scope) {
    form.set("scope", scope);
  }

  return postForm<GitHubDeviceCodeResponse | GitHubErrorResponse>(
    DEVICE_CODE_URL,
    form,
  );
}

export async function exchangeDeviceCode(
  env: Env,
  deviceCode: string,
): Promise<GitHubProxyResponse<GitHubAccessTokenResponse>> {
  const form = new URLSearchParams({
    client_id: requireClientId(env),
    device_code: deviceCode,
    grant_type: DEVICE_GRANT_TYPE,
  });

  return postForm<GitHubAccessTokenResponse>(ACCESS_TOKEN_URL, form);
}

export async function fetchAuthenticatedUser(
  accessToken: string,
): Promise<GitHubAuthenticatedUser> {
  const response = await fetch(GITHUB_USER_URL, {
    headers: {
      Accept: "application/vnd.github+json",
      Authorization: `Bearer ${accessToken}`,
      "User-Agent": "sync-devices-worker",
      "X-GitHub-Api-Version": "2022-11-28",
    },
  });

  const payload = await parseJsonObject(response);
  if (!response.ok) {
    throw new UpstreamRequestError(response.status, payload);
  }

  const id = payload.id;
  const login = payload.login;
  const avatarUrl = payload.avatar_url;
  const name = payload.name;
  if (
    typeof id !== "number" ||
    typeof login !== "string" ||
    typeof avatarUrl !== "string" ||
    (name !== null && typeof name !== "string" && typeof name !== "undefined")
  ) {
    throw new UpstreamRequestError(502, {
      error: "upstream_invalid_response",
      error_description:
        "GitHub user profile response is missing required fields.",
    });
  }

  return {
    id,
    login,
    name: name ?? null,
    avatar_url: avatarUrl,
  };
}

function requireClientId(env: Env): string {
  const clientId = env.GITHUB_CLIENT_ID?.trim();
  if (!clientId) {
    throw new ConfigurationError("Missing GITHUB_CLIENT_ID binding.");
  }

  return clientId;
}

async function postForm<T extends object>(
  url: string,
  body: URLSearchParams,
): Promise<GitHubProxyResponse<T>> {
  const response = await fetch(url, {
    method: "POST",
    headers: {
      Accept: "application/json",
      "Content-Type": "application/x-www-form-urlencoded",
      "User-Agent": "sync-devices-worker",
    },
    body: body.toString(),
  });

  const payload = await parseJsonObject(response);
  if (!response.ok) {
    throw new UpstreamRequestError(response.status, payload);
  }

  return {
    status: response.status,
    body: payload as T,
  };
}

async function parseJsonObject(
  response: Response,
): Promise<Record<string, unknown>> {
  const text = await response.text();
  if (!text) {
    return {};
  }

  try {
    const parsed = JSON.parse(text) as unknown;
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>;
    }
  } catch {
    return {
      error: "upstream_invalid_response",
      error_description: "GitHub returned a non-JSON response.",
      raw: text,
    };
  }

  return {
    error: "upstream_invalid_response",
    error_description: "GitHub returned a JSON value with an unexpected shape.",
  };
}
