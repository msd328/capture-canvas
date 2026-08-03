import { requireSupabaseServerConfig } from "./supabase";

const MAX_ACCESS_TOKEN_BYTES = 16 * 1024;
const MAX_JWKS_BYTES = 64 * 1024;
const MAX_JWKS_KEYS = 16;
const JWKS_FETCH_TIMEOUT_MS = 5_000;
const JWKS_CACHE_MS = 5 * 60 * 1_000;
const CLOCK_SKEW_SECONDS = 60;
const MAX_TOKEN_LIFETIME_SECONDS = 24 * 60 * 60;

const UUID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const KID_PATTERN = /^[A-Za-z0-9._~-]{1,128}$/;

type ServerEnvironment = Record<string, string | undefined>;
type ServerGlobal = typeof globalThis & {
  process?: { env?: ServerEnvironment };
};

type ApprovedAlgorithm = "RS256" | "ES256";

interface AccessTokenTrustConfig {
  issuer: string;
  audience: string;
  clientId: string;
  jwksUri: string;
  algorithms: ReadonlySet<ApprovedAlgorithm>;
}

interface ParsedJwt {
  signingInput: Uint8Array;
  signature: Uint8Array;
  header: {
    alg: ApprovedAlgorithm;
    kid: string;
  };
  payload: Record<string, unknown>;
}

interface CachedJwks {
  expiresAt: number;
  keys: JsonWebKey[];
}

export interface VerifiedSupabaseIdentity {
  userId: string;
  issuer: string;
  audience: string;
  clientId: string;
  expiresAt: number;
}

const jwksCache = new Map<string, CachedJwks>();

function recordValue(source: unknown, key: string): string | undefined {
  if (!source || typeof source !== "object") return undefined;
  const value = (source as Record<string, unknown>)[key];
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function envValue(env: unknown, key: string): string | undefined {
  const runtimeValue = recordValue(env, key);
  if (runtimeValue) return runtimeValue;
  return recordValue((globalThis as ServerGlobal).process?.env, key);
}

function requireTrustConfig(env: unknown): AccessTokenTrustConfig {
  const supabase = requireSupabaseServerConfig(env);
  const rawJwksUri = envValue(env, "RECORDER_AUTH_JWKS_URI");
  const clientId = envValue(env, "RECORDER_AUTH_CLIENT_ID");
  const rawAlgorithms = envValue(env, "RECORDER_AUTH_ALLOWED_ALGORITHMS");
  if (!rawJwksUri || !clientId || !rawAlgorithms) {
    throw new Error("access_token_trust_not_configured");
  }
  if (
    clientId.length > 512 ||
    [...clientId].some((character) => character === "\0" || /\p{C}/u.test(character))
  ) {
    throw new Error("access_token_client_id_invalid");
  }

  let jwksUri: URL;
  try {
    jwksUri = new URL(rawJwksUri);
  } catch {
    throw new Error("access_token_jwks_uri_invalid");
  }
  if (
    jwksUri.protocol !== "https:" ||
    !jwksUri.hostname ||
    jwksUri.username ||
    jwksUri.password ||
    jwksUri.search ||
    jwksUri.hash ||
    jwksUri.origin !== supabase.authIssuer.origin ||
    jwksUri.pathname.replace(/\/$/, "") !== "/auth/v1/.well-known/jwks.json"
  ) {
    throw new Error("access_token_jwks_uri_untrusted");
  }

  const algorithms = new Set<ApprovedAlgorithm>();
  for (const value of rawAlgorithms.split(",").map((item) => item.trim())) {
    if (value === "RS256" || value === "ES256") {
      algorithms.add(value);
    } else if (value) {
      throw new Error("access_token_algorithm_unapproved");
    }
  }
  if (algorithms.size === 0) {
    throw new Error("access_token_algorithm_missing");
  }

  return {
    issuer: supabase.authIssuer.toString().replace(/\/$/, ""),
    audience: supabase.authAudience,
    clientId,
    jwksUri: jwksUri.toString(),
    algorithms,
  };
}

function decodeBase64Url(segment: string, maximumBytes: number): Uint8Array {
  if (!segment || !/^[A-Za-z0-9_-]+$/.test(segment)) {
    throw new Error("jwt_segment_invalid");
  }
  const padded = `${segment.replace(/-/g, "+").replace(/_/g, "/")}${"=".repeat(
    (4 - (segment.length % 4)) % 4,
  )}`;
  let binary: string;
  try {
    binary = globalThis.atob(padded);
  } catch {
    throw new Error("jwt_segment_invalid");
  }
  if (binary.length > maximumBytes) {
    throw new Error("jwt_segment_too_large");
  }
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

function decodeJsonObject(segment: string, maximumBytes: number): Record<string, unknown> {
  const bytes = decodeBase64Url(segment, maximumBytes);
  let text: string;
  try {
    text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new Error("jwt_json_encoding_invalid");
  }
  let value: unknown;
  try {
    value = JSON.parse(text) as unknown;
  } catch {
    throw new Error("jwt_json_invalid");
  }
  assertNoDuplicateTopLevelKeys(text);
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("jwt_json_not_object");
  }
  return value as Record<string, unknown>;
}

function assertNoDuplicateTopLevelKeys(text: string): void {
  let index = skipWhitespace(text, 0);
  if (text[index] !== "{") throw new Error("jwt_json_not_object");
  index = skipWhitespace(text, index + 1);
  const keys = new Set<string>();
  if (text[index] === "}") return;

  while (index < text.length) {
    const keyResult = readJsonString(text, index);
    const key = JSON.parse(text.slice(index, keyResult.end)) as string;
    if (keys.has(key)) throw new Error("jwt_duplicate_claim");
    keys.add(key);
    index = skipWhitespace(text, keyResult.end);
    if (text[index] !== ":") throw new Error("jwt_json_invalid");
    index = skipJsonValue(text, skipWhitespace(text, index + 1));
    index = skipWhitespace(text, index);
    if (text[index] === "}") return;
    if (text[index] !== ",") throw new Error("jwt_json_invalid");
    index = skipWhitespace(text, index + 1);
  }
  throw new Error("jwt_json_invalid");
}

function skipWhitespace(text: string, index: number): number {
  while (index < text.length && /\s/.test(text[index] ?? "")) index += 1;
  return index;
}

function readJsonString(text: string, start: number): { end: number } {
  if (text[start] !== '"') throw new Error("jwt_json_invalid");
  let escaped = false;
  for (let index = start + 1; index < text.length; index += 1) {
    const character = text[index];
    if (escaped) {
      escaped = false;
    } else if (character === "\\") {
      escaped = true;
    } else if (character === '"') {
      return { end: index + 1 };
    }
  }
  throw new Error("jwt_json_invalid");
}

function skipJsonValue(text: string, start: number): number {
  if (text[start] === '"') return readJsonString(text, start).end;
  let objectDepth = 0;
  let arrayDepth = 0;
  let inString = false;
  let escaped = false;

  for (let index = start; index < text.length; index += 1) {
    const character = text[index];
    if (inString) {
      if (escaped) escaped = false;
      else if (character === "\\") escaped = true;
      else if (character === '"') inString = false;
      continue;
    }
    if (character === '"') inString = true;
    else if (character === "{") objectDepth += 1;
    else if (character === "}") {
      if (objectDepth === 0 && arrayDepth === 0) return index;
      objectDepth -= 1;
    } else if (character === "[") arrayDepth += 1;
    else if (character === "]") arrayDepth -= 1;
    else if (character === "," && objectDepth === 0 && arrayDepth === 0) return index;

    if (objectDepth < 0 || arrayDepth < 0) throw new Error("jwt_json_invalid");
  }
  throw new Error("jwt_json_invalid");
}

function parseJwt(token: string, config: AccessTokenTrustConfig): ParsedJwt {
  if (new TextEncoder().encode(token).byteLength > MAX_ACCESS_TOKEN_BYTES) {
    throw new Error("access_token_too_large");
  }
  const segments = token.split(".");
  if (segments.length !== 3) throw new Error("jwt_segment_count");
  const [encodedHeader, encodedPayload, encodedSignature] = segments as [string, string, string];
  const headerValue = decodeJsonObject(encodedHeader, 2 * 1024);
  const payload = decodeJsonObject(encodedPayload, 12 * 1024);

  const headerKeys = Object.keys(headerValue);
  if (headerKeys.some((key) => key !== "alg" && key !== "kid" && key !== "typ")) {
    throw new Error("jwt_header_field_unapproved");
  }
  const alg = headerValue.alg;
  const kid = headerValue.kid;
  const typ = headerValue.typ;
  if ((alg !== "RS256" && alg !== "ES256") || !config.algorithms.has(alg)) {
    throw new Error("jwt_algorithm_unapproved");
  }
  if (typeof kid !== "string" || !KID_PATTERN.test(kid)) {
    throw new Error("jwt_kid_invalid");
  }
  if (typ !== undefined && typ !== "JWT") {
    throw new Error("jwt_type_invalid");
  }

  return {
    signingInput: new TextEncoder().encode(`${encodedHeader}.${encodedPayload}`),
    signature: decodeBase64Url(encodedSignature, 1024),
    header: { alg, kid },
    payload,
  };
}

async function readBoundedResponse(response: Response, maximumBytes: number): Promise<Uint8Array> {
  const declaredLength = response.headers.get("content-length");
  if (declaredLength !== null) {
    const parsed = Number(declaredLength);
    if (!Number.isSafeInteger(parsed) || parsed < 0 || parsed > maximumBytes) {
      throw new Error("remote_response_size_invalid");
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
        throw new Error("remote_response_too_large");
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

async function fetchJwks(uri: string, forceRefresh: boolean): Promise<JsonWebKey[]> {
  const cached = jwksCache.get(uri);
  if (!forceRefresh && cached && Date.now() < cached.expiresAt) return cached.keys;

  const controller = new AbortController();
  const timeout = globalThis.setTimeout(() => controller.abort(), JWKS_FETCH_TIMEOUT_MS);
  let response: Response;
  try {
    response = await fetch(uri, {
      method: "GET",
      headers: { accept: "application/json" },
      redirect: "error",
      cache: "no-store",
      credentials: "omit",
      signal: controller.signal,
    });
  } finally {
    globalThis.clearTimeout(timeout);
  }
  if (!response.ok) throw new Error("jwks_http_failure");
  const contentType = response.headers.get("content-type")?.split(";", 1)[0]?.trim().toLowerCase();
  if (contentType !== "application/json") throw new Error("jwks_content_type_invalid");

  const bytes = await readBoundedResponse(response, MAX_JWKS_BYTES);
  let text: string;
  try {
    text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new Error("jwks_encoding_invalid");
  }
  let document: unknown;
  try {
    document = JSON.parse(text) as unknown;
  } catch {
    throw new Error("jwks_json_invalid");
  }
  if (!document || typeof document !== "object" || Array.isArray(document)) {
    throw new Error("jwks_json_invalid");
  }
  const keys = (document as Record<string, unknown>).keys;
  if (!Array.isArray(keys) || keys.length === 0 || keys.length > MAX_JWKS_KEYS) {
    throw new Error("jwks_key_count_invalid");
  }
  const parsedKeys = keys.map((value) => {
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      throw new Error("jwks_key_invalid");
    }
    return value as JsonWebKey;
  });
  jwksCache.set(uri, { keys: parsedKeys, expiresAt: Date.now() + JWKS_CACHE_MS });
  return parsedKeys;
}

function selectJwk(keys: JsonWebKey[], kid: string, algorithm: ApprovedAlgorithm): JsonWebKey {
  const matches = keys.filter((key) => key.kid === kid && key.alg === algorithm);
  if (matches.length !== 1) throw new Error("jwks_key_not_unique");
  const key = matches[0] as JsonWebKey;
  if (key.use !== undefined && key.use !== "sig") throw new Error("jwks_key_use_invalid");
  if (key.key_ops !== undefined && !key.key_ops.includes("verify")) {
    throw new Error("jwks_key_operation_invalid");
  }
  if (algorithm === "RS256") {
    if (key.kty !== "RSA" || typeof key.n !== "string" || typeof key.e !== "string") {
      throw new Error("jwks_rsa_key_invalid");
    }
  } else if (
    key.kty !== "EC" ||
    key.crv !== "P-256" ||
    typeof key.x !== "string" ||
    typeof key.y !== "string"
  ) {
    throw new Error("jwks_ec_key_invalid");
  }
  return key;
}

async function verifySignature(jwt: ParsedJwt, key: JsonWebKey): Promise<boolean> {
  if (jwt.header.alg === "RS256") {
    const algorithm = { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" } as const;
    const cryptoKey = await crypto.subtle.importKey("jwk", key, algorithm, false, ["verify"]);
    return crypto.subtle.verify(algorithm, cryptoKey, jwt.signature, jwt.signingInput);
  }
  if (jwt.signature.byteLength !== 64) throw new Error("jwt_es256_signature_invalid");
  const importAlgorithm = { name: "ECDSA", namedCurve: "P-256" } as const;
  const cryptoKey = await crypto.subtle.importKey("jwk", key, importAlgorithm, false, ["verify"]);
  return crypto.subtle.verify(
    { name: "ECDSA", hash: "SHA-256" },
    cryptoKey,
    jwt.signature,
    jwt.signingInput,
  );
}

function requiredString(payload: Record<string, unknown>, field: string, maximum: number): string {
  const value = payload[field];
  if (typeof value !== "string" || value.length === 0 || value.length > maximum) {
    throw new Error(`jwt_${field}_invalid`);
  }
  return value;
}

function requiredInteger(payload: Record<string, unknown>, field: string): number {
  const value = payload[field];
  if (!Number.isSafeInteger(value)) throw new Error(`jwt_${field}_invalid`);
  return value as number;
}

function validateClaims(
  payload: Record<string, unknown>,
  config: AccessTokenTrustConfig,
): VerifiedSupabaseIdentity {
  const issuer = requiredString(payload, "iss", 8 * 1024);
  const subject = requiredString(payload, "sub", 64);
  const clientId = requiredString(payload, "client_id", 512);
  const role = requiredString(payload, "role", 64);
  const expiresAt = requiredInteger(payload, "exp");
  const issuedAt = requiredInteger(payload, "iat");
  const notBefore = payload.nbf === undefined ? undefined : requiredInteger(payload, "nbf");
  const now = Math.floor(Date.now() / 1000);

  if (issuer !== config.issuer) throw new Error("jwt_issuer_mismatch");
  if (!UUID_PATTERN.test(subject)) throw new Error("jwt_subject_invalid");
  if (clientId !== config.clientId) throw new Error("jwt_client_id_mismatch");
  if (role !== "authenticated") throw new Error("jwt_role_invalid");
  if (expiresAt <= now - CLOCK_SKEW_SECONDS) throw new Error("jwt_expired");
  if (issuedAt > now + CLOCK_SKEW_SECONDS) throw new Error("jwt_issued_in_future");
  if (expiresAt <= issuedAt || expiresAt - issuedAt > MAX_TOKEN_LIFETIME_SECONDS) {
    throw new Error("jwt_lifetime_invalid");
  }
  if (notBefore !== undefined && notBefore > now + CLOCK_SKEW_SECONDS) {
    throw new Error("jwt_not_yet_valid");
  }

  const audienceClaim = payload.aud;
  const audiences =
    typeof audienceClaim === "string"
      ? [audienceClaim]
      : Array.isArray(audienceClaim) && audienceClaim.every((value) => typeof value === "string")
        ? audienceClaim
        : null;
  if (!audiences || !audiences.includes(config.audience)) {
    throw new Error("jwt_audience_mismatch");
  }

  return {
    userId: subject.toLowerCase(),
    issuer,
    audience: config.audience,
    clientId,
    expiresAt,
  };
}

export async function verifySupabaseAccessToken(
  token: string,
  env: unknown,
): Promise<VerifiedSupabaseIdentity> {
  const config = requireTrustConfig(env);
  const jwt = parseJwt(token, config);

  let verified = false;
  for (const forceRefresh of [false, true]) {
    try {
      const keys = await fetchJwks(config.jwksUri, forceRefresh);
      const key = selectJwk(keys, jwt.header.kid, jwt.header.alg);
      verified = await verifySignature(jwt, key);
      if (verified) break;
    } catch (error) {
      if (forceRefresh) throw error;
    }
  }
  if (!verified) throw new Error("jwt_signature_invalid");
  return validateClaims(jwt.payload, config);
}
