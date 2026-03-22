import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const configPath = resolve(process.cwd(), "wrangler.jsonc");
const raw = await readFile(configPath, "utf8");
const parsed = JSON.parse(stripJsonc(raw));

const binding = Array.isArray(parsed.kv_namespaces)
  ? parsed.kv_namespaces.find((item) => item?.binding === "SYNC_CONFIGS")
  : null;

if (!binding) {
  fail("Missing SYNC_CONFIGS KV binding in wrangler.jsonc.");
}

assertNamespaceId("id", binding.id);
assertNamespaceId("preview_id", binding.preview_id);

console.log("Deploy configuration is ready.");

function assertNamespaceId(fieldName, value) {
  if (typeof value !== "string" || !/^[a-f0-9]{32}$/i.test(value)) {
    fail(`SYNC_CONFIGS.${fieldName} must be a 32-character namespace id.`);
  }

  if (/^0{32}$/i.test(value)) {
    fail(
      `SYNC_CONFIGS.${fieldName} is still the placeholder value. Run the KV bootstrap scripts before deploying.`,
    );
  }
}

function stripJsonc(source) {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/,\s*([}\]])/g, "$1");
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
