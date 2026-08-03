import { requireSupabaseServerConfig } from "./supabase";

type ServerEnvironment = Record<string, string | undefined>;
type ServerGlobal = typeof globalThis & {
  process?: { env?: ServerEnvironment };
};

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

function isNumericLoopback(hostname: string): boolean {
  return hostname === "127.0.0.1" || hostname === "[::1]" || hostname === "::1";
}

export function accessTokenVerificationConfigured(env: unknown): boolean {
  try {
    const supabase = requireSupabaseServerConfig(env);
    const clientId = envValue(env, "RECORDER_AUTH_CLIENT_ID");
    const rawJwksUri = envValue(env, "RECORDER_AUTH_JWKS_URI");
    const rawAlgorithms = envValue(env, "RECORDER_AUTH_ALLOWED_ALGORITHMS");
    if (!clientId || !rawJwksUri || !rawAlgorithms || clientId.length > 512) return false;
    if ([...clientId].some((character) => character === "\0" || /\p{C}/u.test(character))) {
      return false;
    }

    const algorithms = rawAlgorithms
      .split(",")
      .map((value) => value.trim())
      .filter(Boolean);
    if (
      algorithms.length === 0 ||
      algorithms.some((value) => value !== "RS256" && value !== "ES256")
    ) {
      return false;
    }

    const jwksUri = new URL(rawJwksUri);
    const trustedTransport =
      jwksUri.protocol === "https:" ||
      (jwksUri.protocol === "http:" && isNumericLoopback(jwksUri.hostname));
    return (
      trustedTransport &&
      Boolean(jwksUri.hostname) &&
      !jwksUri.username &&
      !jwksUri.password &&
      !jwksUri.search &&
      !jwksUri.hash &&
      jwksUri.origin === supabase.authIssuer.origin &&
      jwksUri.pathname.replace(/\/$/, "") === "/auth/v1/.well-known/jwks.json"
    );
  } catch {
    return false;
  }
}
