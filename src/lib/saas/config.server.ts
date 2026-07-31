import { z } from "zod";

const httpsUrl = z
  .string()
  .url()
  .max(2_048)
  .superRefine((value, context) => {
    const url = new URL(value);
    const localDevelopment = url.hostname === "localhost" || url.hostname === "127.0.0.1";
    if (url.protocol !== "https:" && !localDevelopment) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        message: "SaaS endpoints must use HTTPS outside local development",
      });
    }
  });

const saasServerConfigSchema = z
  .object({
    apiOrigin: httpsUrl,
    oidcIssuer: httpsUrl,
    oidcClientId: z.string().trim().min(1).max(512),
    oidcRedirectUri: z.string().url().max(2_048),
  })
  .strict();

export type SaasServerConfig = z.infer<typeof saasServerConfigSchema>;

export type SaasServerConfigState =
  | { enabled: false; missing: string[] }
  | { enabled: true; config: SaasServerConfig };

const requiredEnvironment = {
  SAAS_API_ORIGIN: "apiOrigin",
  OIDC_ISSUER: "oidcIssuer",
  OIDC_CLIENT_ID: "oidcClientId",
  OIDC_REDIRECT_URI: "oidcRedirectUri",
} as const;

/**
 * Read SaaS configuration only on the server. Missing values keep cloud access
 * disabled; partially configured deployments never guess an endpoint or provider.
 */
export function readSaasServerConfig(
  environment: NodeJS.ProcessEnv = process.env,
): SaasServerConfigState {
  const missing = Object.keys(requiredEnvironment).filter((name) => !environment[name]?.trim());
  if (missing.length > 0) {
    return { enabled: false, missing };
  }

  return {
    enabled: true,
    config: saasServerConfigSchema.parse({
      apiOrigin: environment.SAAS_API_ORIGIN,
      oidcIssuer: environment.OIDC_ISSUER,
      oidcClientId: environment.OIDC_CLIENT_ID,
      oidcRedirectUri: environment.OIDC_REDIRECT_URI,
    }),
  };
}
