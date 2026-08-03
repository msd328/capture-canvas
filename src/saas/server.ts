import { accessTokenVerificationConfigured } from "./access-readiness";
import {
  CreateUploadSessionRequestSchema,
  MAX_RECORDING_UPLOAD_BYTES,
  MAX_UPLOAD_REQUEST_BYTES,
  SAAS_API_VERSION,
  SaasCapabilitiesSchema,
  type SaasApiError,
} from "./contracts";
import { supabaseReadiness } from "./supabase";

const API_PREFIX = `/api/${SAAS_API_VERSION}`;
const MIN_CONFIGURED_UPLOAD_BYTES = 1024 * 1024;
const UPLOAD_REQUEST_READ_TIMEOUT_MS = 10_000;

type ServerEnvironment = Record<string, string | undefined>;
type ServerGlobal = typeof globalThis & {
  process?: { env?: ServerEnvironment };
};

type BoundedJsonResult = { ok: true; value: unknown } | { ok: false; response: Response };

function responseHeaders(extra?: HeadersInit): Headers {
  const headers = new Headers({
    "cache-control": "no-store",
    "content-type": "application/json; charset=utf-8",
    "x-content-type-options": "nosniff",
  });
  if (extra) {
    new Headers(extra).forEach((value, key) => headers.set(key, value));
  }
  return headers;
}

function jsonResponse(body: unknown, status = 200, extraHeaders?: HeadersInit): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: responseHeaders(extraHeaders),
  });
}

function requestId(): string {
  return crypto.randomUUID();
}

function errorResponse(
  status: number,
  code: SaasApiError["error"]["code"],
  message: string,
  extraHeaders?: HeadersInit,
): Response {
  return jsonResponse(
    {
      error: {
        code,
        message,
        requestId: requestId(),
      },
    } satisfies SaasApiError,
    status,
    extraHeaders,
  );
}

function recordValue(source: unknown, key: string): string | undefined {
  if (!source || typeof source !== "object") return undefined;
  const value = (source as Record<string, unknown>)[key];
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function envValue(env: unknown, key: string): string | undefined {
  const runtimeValue = recordValue(env, key);
  if (runtimeValue) return runtimeValue;

  const serverEnvironment = (globalThis as ServerGlobal).process?.env;
  return recordValue(serverEnvironment, key);
}

function isTrustedServerUrl(value: string | undefined): boolean {
  if (!value) return false;
  try {
    const url = new URL(value);
    if (!url.hostname || url.username || url.password || url.search || url.hash) {
      return false;
    }
    if (url.protocol === "https:") return true;
    return (
      url.protocol === "http:" &&
      (url.hostname === "127.0.0.1" || url.hostname === "[::1]" || url.hostname === "::1")
    );
  } catch {
    return false;
  }
}

function configuredUploadLimit(env: unknown): number {
  const raw = envValue(env, "RECORDER_MAX_UPLOAD_BYTES");
  if (!raw) return MAX_RECORDING_UPLOAD_BYTES;

  const parsed = Number(raw);
  if (!Number.isSafeInteger(parsed)) return MAX_RECORDING_UPLOAD_BYTES;
  return Math.min(MAX_RECORDING_UPLOAD_BYTES, Math.max(MIN_CONFIGURED_UPLOAD_BYTES, parsed));
}

function capabilities(env: unknown) {
  const authenticationConfigured =
    isTrustedServerUrl(envValue(env, "RECORDER_AUTH_ISSUER")) &&
    Boolean(envValue(env, "RECORDER_AUTH_AUDIENCE")) &&
    accessTokenVerificationConfigured(env);
  const uploadsConfigured =
    isTrustedServerUrl(envValue(env, "RECORDER_UPLOAD_ORIGIN")) &&
    Boolean(envValue(env, "RECORDER_UPLOAD_BUCKET"));
  const database = supabaseReadiness(env);

  return SaasCapabilitiesSchema.parse({
    apiVersion: SAAS_API_VERSION,
    configured: authenticationConfigured && database.configured && uploadsConfigured,
    authentication: {
      configured: authenticationConfigured,
      protocol: "oidc-pkce",
    },
    database,
    uploads: {
      configured: uploadsConfigured,
      resumable: uploadsConfigured,
      maxUploadBytes: configuredUploadLimit(env),
      acceptedContentTypes: ["video/mp4"],
    },
    visibility: ["private", "unlisted", "public"],
  });
}

function parseContentLength(request: Request): number | null | Response {
  const raw = request.headers.get("content-length");
  if (raw === null) return null;

  const parsed = Number(raw);
  if (!Number.isSafeInteger(parsed) || parsed < 0) {
    return errorResponse(400, "bad_request", "Invalid Content-Length header.");
  }
  if (parsed > MAX_UPLOAD_REQUEST_BYTES) {
    return errorResponse(
      413,
      "payload_too_large",
      "Upload-session request exceeds the allowed size.",
    );
  }
  return parsed;
}

function readWithDeadline(
  reader: ReadableStreamDefaultReader<Uint8Array>,
  deadline: number,
): Promise<ReadableStreamReadResult<Uint8Array> | null> {
  const remaining = deadline - Date.now();
  if (remaining <= 0) return Promise.resolve(null);

  return new Promise((resolve, reject) => {
    let settled = false;
    const timeout = globalThis.setTimeout(() => {
      settled = true;
      resolve(null);
    }, remaining);

    reader.read().then(
      (result) => {
        if (settled) return;
        settled = true;
        globalThis.clearTimeout(timeout);
        resolve(result);
      },
      (error: unknown) => {
        if (settled) return;
        settled = true;
        globalThis.clearTimeout(timeout);
        reject(error);
      },
    );
  });
}

async function readBoundedJson(request: Request): Promise<BoundedJsonResult> {
  const contentType = request.headers.get("content-type")?.split(";", 1)[0]?.trim().toLowerCase();
  if (contentType !== "application/json") {
    return {
      ok: false,
      response: errorResponse(
        415,
        "unsupported_media_type",
        "Upload-session requests must use application/json.",
      ),
    };
  }

  const contentEncoding = request.headers.get("content-encoding")?.trim().toLowerCase();
  if (contentEncoding && contentEncoding !== "identity") {
    return {
      ok: false,
      response: errorResponse(
        415,
        "unsupported_media_type",
        "Compressed upload-session requests are not supported.",
      ),
    };
  }

  const declaredLength = parseContentLength(request);
  if (declaredLength instanceof Response) {
    return { ok: false, response: declaredLength };
  }

  if (!request.body) {
    return {
      ok: false,
      response: errorResponse(400, "bad_request", "A JSON request body is required."),
    };
  }

  const reader = request.body.getReader();
  const deadline = Date.now() + UPLOAD_REQUEST_READ_TIMEOUT_MS;
  const chunks: Uint8Array[] = [];
  let received = 0;

  try {
    while (true) {
      const result = await readWithDeadline(reader, deadline);
      if (!result) {
        await reader.cancel("request body deadline exceeded").catch(() => undefined);
        return {
          ok: false,
          response: errorResponse(
            408,
            "request_timeout",
            "Upload-session request body was not received in time.",
          ),
        };
      }

      if (result.done) break;
      received += result.value.byteLength;
      if (received > MAX_UPLOAD_REQUEST_BYTES) {
        await reader.cancel("request body limit exceeded").catch(() => undefined);
        return {
          ok: false,
          response: errorResponse(
            413,
            "payload_too_large",
            "Upload-session request exceeds the allowed size.",
          ),
        };
      }
      chunks.push(result.value);
    }
  } catch {
    return {
      ok: false,
      response: errorResponse(400, "bad_request", "Unable to read the request body."),
    };
  }

  if (declaredLength !== null && declaredLength !== received) {
    return {
      ok: false,
      response: errorResponse(
        400,
        "bad_request",
        "Request body length does not match Content-Length.",
      ),
    };
  }

  const bytes = new Uint8Array(received);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }

  let text: string;
  try {
    text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return {
      ok: false,
      response: errorResponse(400, "bad_request", "Request body must be valid UTF-8."),
    };
  }

  try {
    return { ok: true, value: JSON.parse(text) as unknown };
  } catch {
    return {
      ok: false,
      response: errorResponse(400, "bad_request", "Request body must contain valid JSON."),
    };
  }
}

async function handleUploadSessionRequest(request: Request, env: unknown): Promise<Response> {
  if (request.method !== "POST") {
    return errorResponse(405, "method_not_allowed", "Upload sessions must be created with POST.", {
      allow: "POST, OPTIONS",
    });
  }

  const body = await readBoundedJson(request);
  if (!body.ok) return body.response;

  const parsedRequest = CreateUploadSessionRequestSchema.safeParse(body.value);
  if (!parsedRequest.success) {
    return errorResponse(
      400,
      "bad_request",
      "Upload-session request does not match the required contract.",
    );
  }

  const current = capabilities(env);
  if (parsedRequest.data.fileSizeBytes > current.uploads.maxUploadBytes) {
    return errorResponse(
      413,
      "payload_too_large",
      "Recording exceeds the configured upload limit.",
    );
  }

  if (!current.configured) {
    return errorResponse(
      503,
      "not_configured",
      "Recorder cloud authentication, database, and upload storage are not configured.",
    );
  }

  return errorResponse(
    501,
    "not_implemented",
    "Authenticated upload-session creation is not implemented yet.",
  );
}

export async function handleSaasApiRequest(
  request: Request,
  env: unknown,
): Promise<Response | null> {
  const url = new URL(request.url);
  if (!url.pathname.startsWith(`${API_PREFIX}/`) && url.pathname !== API_PREFIX) {
    return null;
  }

  if (request.method === "GET" && url.pathname === `${API_PREFIX}/health`) {
    return jsonResponse({ apiVersion: SAAS_API_VERSION, status: "ok" });
  }

  if (request.method === "GET" && url.pathname === `${API_PREFIX}/capabilities`) {
    return jsonResponse(capabilities(env));
  }

  if (request.method === "OPTIONS") {
    return new Response(null, {
      status: 204,
      headers: new Headers({
        allow: "GET, POST, OPTIONS",
        "cache-control": "no-store",
        "x-content-type-options": "nosniff",
      }),
    });
  }

  if (url.pathname === `${API_PREFIX}/upload-sessions`) {
    return handleUploadSessionRequest(request, env);
  }

  return errorResponse(404, "not_found", "SaaS API route not found.");
}
