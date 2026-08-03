const MAX_URL_BYTES = 8 * 1024;
const MAX_AUDIENCE_CHARS = 512;
const MAX_SECRET_CHARS = 4_096;

type ServerEnvironment = Record<string, string | undefined>;
type ServerGlobal = typeof globalThis & {
  process?: { env?: ServerEnvironment };
};

export interface SupabaseServerConfig {
  url: URL;
  authIssuer: URL;
  authAudience: string;
  secretKey: string;
}

export interface SupabaseReadiness {
  provider: "supabase";
  configured: boolean;
  configurationValid: boolean;
  urlTrusted: boolean;
  authIssuerTrusted: boolean;
  audienceConfigured: boolean;
  serverSecretConfigured: boolean;
}

class SupabaseConfigError extends Error {
  constructor(readonly code: string) {
    super(`Supabase server configuration is invalid (${code})`);
    this.name = "SupabaseConfigError";
  }
}

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

function parseTrustedUrl(raw: string, field: "url" | "issuer"): URL {
  if (raw.length > MAX_URL_BYTES) {
    throw new SupabaseConfigError(`${field}_too_large`);
  }

  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    throw new SupabaseConfigError(`${field}_parse`);
  }

  if (url.username || url.password || url.search || url.hash) {
    throw new SupabaseConfigError(`${field}_components`);
  }

  const trustedTransport =
    url.protocol === "https:" || (url.protocol === "http:" && isNumericLoopback(url.hostname));
  if (!trustedTransport) {
    throw new SupabaseConfigError(`${field}_transport`);
  }

  if (!url.hostname) {
    throw new SupabaseConfigError(`${field}_host`);
  }

  return url;
}

function parseAudience(raw: string): string {
  if (
    raw.length > MAX_AUDIENCE_CHARS ||
    [...raw].some((character) => /\s|\p{C}/u.test(character))
  ) {
    throw new SupabaseConfigError("audience_invalid");
  }
  return raw;
}

function parseSecret(raw: string): string {
  if (
    raw.length < 32 ||
    raw.length > MAX_SECRET_CHARS ||
    [...raw].some((character) => /\s|\p{C}/u.test(character))
  ) {
    throw new SupabaseConfigError("secret_invalid");
  }
  return raw;
}

function fromEnvironment(env: unknown): SupabaseServerConfig | null {
  const rawUrl = envValue(env, "RECORDER_SUPABASE_URL");
  const rawIssuer = envValue(env, "RECORDER_AUTH_ISSUER");
  const rawAudience = envValue(env, "RECORDER_AUTH_AUDIENCE");
  const rawSecret = envValue(env, "RECORDER_SUPABASE_SECRET_KEY");
  const values = [rawUrl, rawIssuer, rawAudience, rawSecret];

  if (values.every((value) => value === undefined)) return null;
  if (values.some((value) => value === undefined)) {
    throw new SupabaseConfigError("incomplete");
  }

  const url = parseTrustedUrl(rawUrl!, "url");
  const authIssuer = parseTrustedUrl(rawIssuer!, "issuer");
  const authAudience = parseAudience(rawAudience!);
  const secretKey = parseSecret(rawSecret!);

  if (url.pathname !== "/") {
    throw new SupabaseConfigError("url_path");
  }
  if (authIssuer.origin !== url.origin || authIssuer.pathname.replace(/\/$/, "") !== "/auth/v1") {
    throw new SupabaseConfigError("issuer_mismatch");
  }

  return { url, authIssuer, authAudience, secretKey };
}

export function supabaseReadiness(env: unknown): SupabaseReadiness {
  try {
    const config = fromEnvironment(env);
    if (!config) {
      return {
        provider: "supabase",
        configured: false,
        configurationValid: true,
        urlTrusted: false,
        authIssuerTrusted: false,
        audienceConfigured: false,
        serverSecretConfigured: false,
      };
    }

    return {
      provider: "supabase",
      configured: true,
      configurationValid: true,
      urlTrusted: true,
      authIssuerTrusted: true,
      audienceConfigured: true,
      serverSecretConfigured: true,
    };
  } catch {
    return {
      provider: "supabase",
      configured: false,
      configurationValid: false,
      urlTrusted: false,
      authIssuerTrusted: false,
      audienceConfigured: false,
      serverSecretConfigured: false,
    };
  }
}

export function requireSupabaseServerConfig(env: unknown): SupabaseServerConfig {
  const config = fromEnvironment(env);
  if (!config) {
    throw new SupabaseConfigError("not_configured");
  }
  return config;
}
