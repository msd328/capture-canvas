import {
  MAX_RECORDING_UPLOAD_BYTES,
  SAAS_API_VERSION,
  SaasCapabilitiesSchema,
  type SaasApiError,
} from "./contracts";

const API_PREFIX = `/api/${SAAS_API_VERSION}`;
const MIN_CONFIGURED_UPLOAD_BYTES = 1024 * 1024;

function responseHeaders(): Headers {
  return new Headers({
    "cache-control": "no-store",
    "content-type": "application/json; charset=utf-8",
    "x-content-type-options": "nosniff",
  });
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: responseHeaders(),
  });
}

function requestId(): string {
  return crypto.randomUUID();
}

function errorResponse(
  status: number,
  code: SaasApiError["error"]["code"],
  message: string,
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

  if (typeof process !== "undefined") {
    return recordValue(process.env, key);
  }
  return undefined;
}

function isHttpsUrl(value: string | undefined): boolean {
  if (!value) return false;
  try {
    return new URL(value).protocol === "https:";
  } catch {
    return false;
  }
}

function configuredUploadLimit(env: unknown): number {
  const raw = envValue(env, "RECORDER_MAX_UPLOAD_BYTES");
  if (!raw) return MAX_RECORDING_UPLOAD_BYTES;

  const parsed = Number(raw);
  if (!Number.isSafeInteger(parsed)) return MAX_RECORDING_UPLOAD_BYTES;
  return Math.min(
    MAX_RECORDING_UPLOAD_BYTES,
    Math.max(MIN_CONFIGURED_UPLOAD_BYTES, parsed),
  );
}

function capabilities(env: unknown) {
  const authenticationConfigured =
    isHttpsUrl(envValue(env, "RECORDER_AUTH_ISSUER")) &&
    Boolean(envValue(env, "RECORDER_AUTH_AUDIENCE"));
  const uploadsConfigured =
    isHttpsUrl(envValue(env, "RECORDER_UPLOAD_ORIGIN")) &&
    Boolean(envValue(env, "RECORDER_UPLOAD_BUCKET"));

  return SaasCapabilitiesSchema.parse({
    apiVersion: SAAS_API_VERSION,
    configured: authenticationConfigured && uploadsConfigured,
    authentication: {
      configured: authenticationConfigured,
      protocol: "oidc-pkce",
    },
    uploads: {
      configured: uploadsConfigured,
      resumable: uploadsConfigured,
      maxUploadBytes: configuredUploadLimit(env),
      acceptedContentTypes: ["video/mp4"],
    },
    visibility: ["private", "unlisted", "public"],
  });
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
        "cache-control": "no-store",
        "x-content-type-options": "nosniff",
      }),
    });
  }

  if (url.pathname === `${API_PREFIX}/upload-sessions`) {
    const current = capabilities(env);
    if (!current.configured) {
      return errorResponse(
        503,
        "not_configured",
        "Recorder cloud authentication and upload storage are not configured.",
      );
    }
    return errorResponse(
      501,
      "not_implemented",
      "Authenticated upload-session creation is not implemented yet.",
    );
  }

  return errorResponse(404, "not_found", "SaaS API route not found.");
}
