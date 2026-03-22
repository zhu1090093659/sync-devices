import { ConfigurationError, type Env } from "./github-oauth";

const VALID_TOOLS = [
  "claude_code",
  "codex",
  "cursor",
  "shared_agents",
] as const;
const VALID_CATEGORIES = [
  "settings",
  "instructions",
  "commands",
  "skills",
  "mcp",
  "plugins",
  "rules",
] as const;
const CONFIG_KEY_PREFIX = "configs";

type ToolName = (typeof VALID_TOOLS)[number];
type CategoryName = (typeof VALID_CATEGORIES)[number];

interface ListKey {
  name: string;
}

interface ListResult {
  keys: ListKey[];
  list_complete: boolean;
  cursor?: string;
}

export interface ConfigUploadPayload {
  content: string;
  content_hash?: string;
  last_modified: number;
  device_id?: string;
  is_device_specific?: boolean;
}

export interface StoredConfigRecord {
  id: string;
  tool: ToolName;
  category: CategoryName;
  rel_path: string;
  content: string;
  content_hash: string;
  last_modified: number;
  device_id: string;
  is_device_specific: boolean;
  updated_at: number;
}

export interface ConfigListFilters {
  tool?: string | null;
  category?: string | null;
}

export interface ManifestEntryRecord {
  tool: ToolName;
  category: CategoryName;
  rel_path: string;
  content_hash: string;
  last_modified: number;
  device_id: string;
  is_device_specific: boolean;
}

export interface SyncManifestRecord {
  device_id: string;
  generated_at: number;
  items: ManifestEntryRecord[];
}

export class RequestValidationError extends Error {
  constructor(
    public readonly status: number,
    public readonly body: Record<string, unknown>,
  ) {
    super(
      typeof body.error_description === "string"
        ? body.error_description
        : "Invalid request",
    );
  }
}

export async function saveConfigRecord(
  env: Env,
  ownerSubject: string,
  tool: string,
  category: string,
  relPath: string,
  payload: ConfigUploadPayload,
): Promise<StoredConfigRecord> {
  const validatedTool = validateTool(tool);
  const validatedCategory = validateCategory(category);
  const normalizedPath = normalizeRelativePath(relPath);
  const validatedPayload = await validatePayload(payload);
  const record: StoredConfigRecord = {
    id: buildConfigId(validatedTool, validatedCategory, normalizedPath),
    tool: validatedTool,
    category: validatedCategory,
    rel_path: normalizedPath,
    content: validatedPayload.content,
    content_hash: validatedPayload.content_hash,
    last_modified: validatedPayload.last_modified,
    device_id: validatedPayload.device_id,
    is_device_specific: validatedPayload.is_device_specific,
    updated_at: Math.floor(Date.now() / 1000),
  };

  const key = buildConfigKey(
    ownerSubject,
    validatedTool,
    validatedCategory,
    normalizedPath,
  );
  const store = requireConfigStore(env);
  await store.put(key, JSON.stringify(record));
  return record;
}

export async function listConfigRecords(
  env: Env,
  ownerSubject: string,
  filters: ConfigListFilters,
): Promise<StoredConfigRecord[]> {
  const validatedTool = validateOptionalTool(filters.tool);
  const validatedCategory = validateOptionalCategory(filters.category);
  const store = requireConfigStore(env);
  const prefix = buildListPrefix(
    ownerSubject,
    validatedTool,
    validatedCategory,
  );

  const records: StoredConfigRecord[] = [];
  let cursor: string | undefined;

  do {
    const result = await store.list({
      prefix,
      cursor,
    });

    const batch = await Promise.all(
      result.keys.map((key) => store.get<StoredConfigRecord>(key.name, "json")),
    );

    for (const record of batch) {
      if (!record) {
        continue;
      }

      if (validatedTool && record.tool !== validatedTool) {
        continue;
      }

      if (validatedCategory && record.category !== validatedCategory) {
        continue;
      }

      records.push(record);
    }

    cursor = result.list_complete ? undefined : result.cursor;
  } while (cursor);

  return records.sort(compareConfigRecords);
}

export async function getConfigManifest(
  env: Env,
  ownerSubject: string,
): Promise<SyncManifestRecord> {
  const records = await listConfigRecords(env, ownerSubject, {});
  return {
    device_id: buildRemoteManifestId(ownerSubject),
    generated_at: Math.floor(Date.now() / 1000),
    items: records.map(toManifestEntry),
  };
}

export async function deleteConfigRecord(
  env: Env,
  ownerSubject: string,
  configId: string,
): Promise<StoredConfigRecord> {
  const parsed = parseConfigId(configId);
  const key = buildConfigKey(
    ownerSubject,
    parsed.tool,
    parsed.category,
    parsed.relPath,
  );
  const store = requireConfigStore(env);
  const record = await store.get<StoredConfigRecord>(key, "json");
  if (!record) {
    throw new RequestValidationError(404, {
      error: "config_not_found",
      error_description: "The requested config does not exist.",
    });
  }

  await store.delete(key);
  return record;
}

export function extractConfigId(pathname: string): string {
  const prefix = "/api/configs/";
  if (!pathname.startsWith(prefix)) {
    throw new RequestValidationError(400, {
      error: "invalid_request",
      error_description: "Request path does not match the config route.",
    });
  }

  const configId = pathname.slice(prefix.length);
  if (!configId) {
    throw new RequestValidationError(400, {
      error: "invalid_request",
      error_description: "The config id must not be empty.",
    });
  }

  return configId;
}

export function extractRelativePath(
  pathname: string,
  tool: string,
  category: string,
): string {
  const prefix = `/api/configs/${tool}/${category}/`;
  if (!pathname.startsWith(prefix)) {
    throw new RequestValidationError(400, {
      error: "invalid_request",
      error_description: "Request path does not match the config upload route.",
    });
  }

  return pathname.slice(prefix.length);
}

function validateTool(value: string): ToolName {
  if (isToolName(value)) {
    return value;
  }

  throw new RequestValidationError(400, {
    error: "invalid_tool",
    error_description: `Unsupported tool '${value}'.`,
  });
}

function validateCategory(value: string): CategoryName {
  if (isCategoryName(value)) {
    return value;
  }

  throw new RequestValidationError(400, {
    error: "invalid_category",
    error_description: `Unsupported category '${value}'.`,
  });
}

function validateOptionalTool(
  value: string | null | undefined,
): ToolName | null {
  if (!value) {
    return null;
  }

  return validateTool(value);
}

function validateOptionalCategory(
  value: string | null | undefined,
): CategoryName | null {
  if (!value) {
    return null;
  }

  return validateCategory(value);
}

function normalizeRelativePath(value: string): string {
  const decoded = decodePathSegment(value);
  const normalized = decoded.replace(/\\/g, "/").replace(/^\/+/, "");
  const parts = normalized.split("/").filter((part) => part.length > 0);

  if (parts.length === 0) {
    throw new RequestValidationError(400, {
      error: "invalid_path",
      error_description: "The config path must not be empty.",
    });
  }

  if (parts.some((part) => part === "." || part === "..")) {
    throw new RequestValidationError(400, {
      error: "invalid_path",
      error_description:
        "The config path must not contain '.' or '..' segments.",
    });
  }

  return parts.join("/");
}

async function validatePayload(payload: ConfigUploadPayload): Promise<{
  content: string;
  content_hash: string;
  last_modified: number;
  device_id: string;
  is_device_specific: boolean;
}> {
  if (typeof payload.content !== "string") {
    throw new RequestValidationError(400, {
      error: "invalid_request",
      error_description:
        "The request body must include a string content field.",
    });
  }

  if (!Number.isInteger(payload.last_modified) || payload.last_modified < 0) {
    throw new RequestValidationError(400, {
      error: "invalid_request",
      error_description:
        "The request body must include a non-negative integer last_modified field.",
    });
  }

  if (
    typeof payload.is_device_specific !== "undefined" &&
    typeof payload.is_device_specific !== "boolean"
  ) {
    throw new RequestValidationError(400, {
      error: "invalid_request",
      error_description:
        "The is_device_specific field must be a boolean when provided.",
    });
  }

  if (
    typeof payload.device_id !== "undefined" &&
    typeof payload.device_id !== "string"
  ) {
    throw new RequestValidationError(400, {
      error: "invalid_request",
      error_description: "The device_id field must be a string when provided.",
    });
  }

  const computedHash = await sha256Hex(payload.content);
  if (
    typeof payload.content_hash === "string" &&
    payload.content_hash.length > 0 &&
    payload.content_hash !== computedHash
  ) {
    throw new RequestValidationError(400, {
      error: "invalid_request",
      error_description:
        "The provided content_hash does not match the request body content.",
    });
  }

  return {
    content: payload.content,
    content_hash: computedHash,
    last_modified: payload.last_modified,
    device_id: payload.device_id?.trim() ?? "",
    is_device_specific: payload.is_device_specific ?? false,
  };
}

function buildConfigId(
  tool: ToolName,
  category: CategoryName,
  relPath: string,
): string {
  return `${tool}:${category}:${encodeURIComponent(relPath)}`;
}

function parseConfigId(configId: string): {
  tool: ToolName;
  category: CategoryName;
  relPath: string;
} {
  const parts = configId.split(":");
  if (parts.length !== 3) {
    throw new RequestValidationError(400, {
      error: "invalid_config_id",
      error_description:
        "The config id must use the tool:category:path format.",
    });
  }

  const [tool, category, relPath] = parts;
  return {
    tool: validateTool(tool),
    category: validateCategory(category),
    relPath: normalizeRelativePath(relPath),
  };
}

function buildConfigKey(
  ownerSubject: string,
  tool: ToolName,
  category: CategoryName,
  relPath: string,
): string {
  return `${CONFIG_KEY_PREFIX}:${ownerSubject}:${tool}:${category}:${encodeURIComponent(relPath)}`;
}

function buildListPrefix(
  ownerSubject: string,
  tool: ToolName | null,
  category: CategoryName | null,
): string {
  let prefix = `${CONFIG_KEY_PREFIX}:${ownerSubject}:`;
  if (!tool) {
    return prefix;
  }

  prefix += `${tool}:`;
  if (!category) {
    return prefix;
  }

  return `${prefix}${category}:`;
}

function requireConfigStore(env: Env) {
  if (!env.SYNC_CONFIGS) {
    throw new ConfigurationError("Missing SYNC_CONFIGS KV binding.");
  }

  return env.SYNC_CONFIGS;
}

function toManifestEntry(record: StoredConfigRecord): ManifestEntryRecord {
  return {
    tool: record.tool,
    category: record.category,
    rel_path: record.rel_path,
    content_hash: record.content_hash,
    last_modified: record.last_modified,
    device_id: record.device_id,
    is_device_specific: record.is_device_specific,
  };
}

function buildRemoteManifestId(ownerSubject: string): string {
  return `remote:${ownerSubject}`;
}

function compareConfigRecords(
  a: StoredConfigRecord,
  b: StoredConfigRecord,
): number {
  if (a.tool !== b.tool) {
    return a.tool.localeCompare(b.tool);
  }

  if (a.category !== b.category) {
    return a.category.localeCompare(b.category);
  }

  return a.rel_path.localeCompare(b.rel_path);
}

function isToolName(value: string): value is ToolName {
  return VALID_TOOLS.includes(value as ToolName);
}

function isCategoryName(value: string): value is CategoryName {
  return VALID_CATEGORIES.includes(value as CategoryName);
}

function decodePathSegment(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    throw new RequestValidationError(400, {
      error: "invalid_path",
      error_description: "The config path is not valid URL-encoded text.",
    });
  }
}

async function sha256Hex(content: string): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(content),
  );
  return Array.from(new Uint8Array(digest), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}
