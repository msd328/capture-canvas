import { z } from "zod";
import {
  MeAccessResponseSchema,
  SupabaseAccessRowSchema,
  type MeAccessResponse,
} from "./access-contracts";
import { isAccessTokenConfigurationError, verifySupabaseAccessToken } from "./access-token";
import { SAAS_API_VERSION, type SaasApiError } from "./contracts";
import { requireSupabaseServerConfig } from "./supabase";

const ACCESS_PATH = `/api/${SAAS_API_VERSION}/me/access`;
const MAX_AUTHORIZATION_HEADER_BYTES = 16 * 1024 + 16;
const MAX_ACCESS_RPC_BYTES = 32 * 1024;
const ACCESS_RPC_TIMEOUT_MS = 5_000;
const SupabaseAccessRowsSchema = z.array(SupabaseAccessRowSchema).length(1);

type AccessFailureCode = SaasApiError["error"]["code"];

function responseHeaders(extra?: HeadersInit): Headers {
  const headers = new Headers({
    "cache-control": "no-store",
    "content-type": "application/json; charset=utf-8",
    pragma: "no-cache",
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

function errorResponse(
  status: number,
  code: AccessFailureCode,
  message: string,
  extraHeaders?: HeadersInit,
): Response {
  return jsonResponse(
    {
      error: {
        code,
        message,
        requestId: crypto.randomUUID(),
      },
    } satisfies SaasApiError,
    status,
    extraHeaders,
  );
}

function bearerToken(request: Request): string | null {
  const authorization = request.headers.get("authorization");
  if (!authorization) return null;
  if (new TextEncoder().encode(authorization).byteLength > MAX_AUTHORIZATION_HEADER_BYTES) {
    return null;
  }
  const match = /^Bearer ([A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+)$/.exec(authorization);
  return match?.[1] ?? null;
}

async function readBoundedResponse(response: Response, maximumBytes: number): Promise<Uint8Array> {
  const declaredLength = response.headers.get("content-length");
  if (declaredLength !== null) {
    const parsed = Number(declaredLength);
    if (!Number.isSafeInteger(parsed) || parsed < 0 || parsed > maximumBytes) {
      throw new Error("access_rpc_response_size_invalid");
    }
  }
  if (!response.body) return new Uint8Array();

  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let received = 0;
  try {
    while (true) {
      const result = await reader.read();
      if (result.done) break;
      received += result.value.byteLength;
      if (received > maximumBytes) {
        await reader.cancel("response limit exceeded").catch(() => undefined);
        throw new Error("access_rpc_response_too_large");
      }
      chunks.push(result.value);
    }
  } finally {
    reader.releaseLock();
  }

  const bytes = new Uint8Array(received);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return bytes;
}

async function queryAccessRpc(token: string, env: unknown): Promise<unknown> {
  const config = requireSupabaseServerConfig(env);
  const endpoint = new URL("/rest/v1/rpc/get_my_access", config.url);
  const controller = new AbortController();
  const timeout = globalThis.setTimeout(() => controller.abort(), ACCESS_RPC_TIMEOUT_MS);

  try {
    const response = await fetch(endpoint, {
      method: "POST",
      headers: {
        accept: "application/json",
        apikey: config.secretKey,
        authorization: `Bearer ${token}`,
        "content-type": "application/json",
      },
      body: "{}",
      redirect: "error",
      cache: "no-store",
      credentials: "omit",
      signal: controller.signal,
    });
    if (!response.ok) {
      throw new Error(
        response.status === 401 || response.status === 403
          ? "access_rpc_not_authorized"
          : "access_rpc_http_failure",
      );
    }
    const contentType = response.headers
      .get("content-type")
      ?.split(";", 1)[0]
      ?.trim()
      .toLowerCase();
    if (contentType !== "application/json") {
      throw new Error("access_rpc_content_type_invalid");
    }

    const bytes = await readBoundedResponse(response, MAX_ACCESS_RPC_BYTES);
    let text: string;
    try {
      text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    } catch {
      throw new Error("access_rpc_encoding_invalid");
    }
    try {
      return JSON.parse(text) as unknown;
    } catch {
      throw new Error("access_rpc_json_invalid");
    }
  } catch (error) {
    if (controller.signal.aborted) throw new Error("access_rpc_timeout");
    throw error;
  } finally {
    globalThis.clearTimeout(timeout);
  }
}

function normalizeAccessResponse(userId: string, raw: unknown): MeAccessResponse {
  const [row] = SupabaseAccessRowsSchema.parse(raw);
  const entitlements = [...new Set(row.entitlements)].sort();
  const calculatedFullAccess =
    row.account_status === "active" && entitlements.includes("desktop_full_access");
  if (row.full_access !== calculatedFullAccess) {
    throw new Error("access_rpc_full_access_mismatch");
  }

  const hasSubscriptionIdentity =
    row.subscription_provider !== null && row.subscription_status !== null;
  if (
    (row.subscription_provider === null) !== (row.subscription_status === null) ||
    (!hasSubscriptionIdentity && row.current_period_end !== null)
  ) {
    throw new Error("access_rpc_subscription_shape_invalid");
  }

  return MeAccessResponseSchema.parse({
    userId,
    authenticated: true,
    accountStatus: row.account_status,
    subscription: hasSubscriptionIdentity
      ? {
          provider: row.subscription_provider,
          status: row.subscription_status,
          currentPeriodEnd: row.current_period_end,
        }
      : null,
    entitlements,
    fullAccess: calculatedFullAccess,
  });
}

async function handleGetAccess(request: Request, env: unknown): Promise<Response> {
  const token = bearerToken(request);
  if (!token) {
    console.info("[Recorder][SaasHealth] stage=me_access ok=false code=not_authenticated");
    return errorResponse(401, "not_authenticated", "Authentication is required.", {
      "www-authenticate": "Bearer",
    });
  }

  let userId: string;
  try {
    const identity = await verifySupabaseAccessToken(token, env);
    userId = identity.userId;
  } catch (error) {
    if (isAccessTokenConfigurationError(error)) {
      console.error("[Recorder][SaasHealth] stage=me_access ok=false code=not_configured");
      return errorResponse(503, "not_configured", "SaaS authentication is not configured.");
    }
    console.info("[Recorder][SaasHealth] stage=me_access ok=false code=invalid_token");
    return errorResponse(401, "not_authenticated", "Authentication is required.", {
      "www-authenticate": 'Bearer error="invalid_token"',
    });
  }

  try {
    const access = normalizeAccessResponse(userId, await queryAccessRpc(token, env));
    console.info(
      `[Recorder][SaasHealth] stage=me_access ok=true full_access=${access.fullAccess} entitlement_count=${access.entitlements.length}`,
    );
    return jsonResponse(access);
  } catch {
    console.error("[Recorder][SaasHealth] stage=me_access ok=false code=access_lookup_failed");
    return errorResponse(502, "internal_error", "Unable to determine account access.");
  }
}

export async function handleSaasAccessRequest(
  request: Request,
  env: unknown,
): Promise<Response | null> {
  const url = new URL(request.url);
  if (url.pathname !== ACCESS_PATH) return null;

  if (request.method === "OPTIONS") {
    return new Response(null, {
      status: 204,
      headers: new Headers({
        allow: "GET, OPTIONS",
        "cache-control": "no-store",
        "x-content-type-options": "nosniff",
      }),
    });
  }
  if (request.method !== "GET") {
    return errorResponse(405, "method_not_allowed", "Account access must be read with GET.", {
      allow: "GET, OPTIONS",
    });
  }
  return handleGetAccess(request, env);
}
