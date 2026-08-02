import { z } from "zod";

export const SAAS_API_VERSION = "v1" as const;
export const MAX_RECORDING_UPLOAD_BYTES = 20 * 1024 * 1024 * 1024;
export const MAX_UPLOAD_REQUEST_BYTES = 64 * 1024;

export const RecordingVisibilitySchema = z.enum(["private", "unlisted", "public"]);
export type RecordingVisibility = z.infer<typeof RecordingVisibilitySchema>;

const IdSchema = z.string().uuid();
const IsoDateTimeSchema = z.string().datetime({ offset: true });
const Sha256Schema = z
  .string()
  .regex(/^[a-f0-9]{64}$/, "Expected a lowercase SHA-256 hex digest");

const opaqueProtocolValue = (field: string, minimum: number, maximum: number) =>
  z
    .string()
    .min(minimum, `${field} is too short`)
    .max(maximum, `${field} is too long`)
    .regex(/^[A-Za-z0-9._~-]+$/, `${field} contains unsupported characters`);

export const OidcAuthorizationCallbackSchema = z
  .object({
    code: opaqueProtocolValue("Authorization code", 8, 4_096),
    state: opaqueProtocolValue("Authorization state", 32, 512),
  })
  .strict();
export type OidcAuthorizationCallback = z.infer<typeof OidcAuthorizationCallbackSchema>;

export const OidcPkceTransactionSchema = z
  .object({
    state: opaqueProtocolValue("Authorization state", 32, 512),
    nonce: opaqueProtocolValue("OIDC nonce", 32, 512),
    codeVerifier: opaqueProtocolValue("PKCE verifier", 43, 128),
    redirectUri: z.string().url().max(2_048),
    createdAt: IsoDateTimeSchema,
  })
  .strict();
export type OidcPkceTransaction = z.infer<typeof OidcPkceTransactionSchema>;

export const AuthenticatedUserSchema = z
  .object({
    id: IdSchema,
    email: z.string().email().max(320),
    displayName: z.string().trim().min(1).max(120),
  })
  .strict();
export type AuthenticatedUser = z.infer<typeof AuthenticatedUserSchema>;

export const CloudRecordingSchema = z
  .object({
    id: IdSchema,
    title: z.string().trim().min(1).max(200),
    visibility: RecordingVisibilitySchema,
    status: z.enum(["uploading", "ready", "failed", "deleted"]),
    durationMs: z.number().int().nonnegative().max(24 * 60 * 60 * 1_000),
    fileSizeBytes: z.number().int().positive().max(MAX_RECORDING_UPLOAD_BYTES),
    contentType: z.literal("video/mp4"),
    createdAt: IsoDateTimeSchema,
    updatedAt: IsoDateTimeSchema,
    thumbnailUrl: z.string().url().max(4_096).nullable(),
    playbackUrl: z.string().url().max(4_096).nullable(),
  })
  .strict();
export type CloudRecording = z.infer<typeof CloudRecordingSchema>;

export const CreateUploadSessionRequestSchema = z
  .object({
    localRecordingId: IdSchema,
    title: z.string().trim().min(1).max(200),
    contentType: z.literal("video/mp4"),
    fileSizeBytes: z.number().int().positive().max(MAX_RECORDING_UPLOAD_BYTES),
    durationMs: z.number().int().nonnegative().max(24 * 60 * 60 * 1_000),
    sha256: Sha256Schema,
    visibility: RecordingVisibilitySchema.default("private"),
  })
  .strict();
export type CreateUploadSessionRequest = z.infer<typeof CreateUploadSessionRequestSchema>;

export const UploadSessionSchema = z
  .object({
    id: IdSchema,
    recordingId: IdSchema,
    uploadUrl: z.string().url().max(4_096),
    expiresAt: IsoDateTimeSchema,
    offsetBytes: z.number().int().nonnegative().max(MAX_RECORDING_UPLOAD_BYTES),
    requiredHeaders: z.record(z.string().max(128), z.string().max(4_096)).default({}),
  })
  .strict();
export type UploadSession = z.infer<typeof UploadSessionSchema>;

export const SaasCapabilitiesSchema = z
  .object({
    apiVersion: z.literal(SAAS_API_VERSION),
    configured: z.boolean(),
    authentication: z
      .object({
        configured: z.boolean(),
        protocol: z.literal("oidc-pkce"),
      })
      .strict(),
    database: z
      .object({
        provider: z.literal("supabase"),
        configured: z.boolean(),
        configurationValid: z.boolean(),
        urlTrusted: z.boolean(),
        authIssuerTrusted: z.boolean(),
        audienceConfigured: z.boolean(),
        serverSecretConfigured: z.boolean(),
      })
      .strict(),
    uploads: z
      .object({
        configured: z.boolean(),
        resumable: z.boolean(),
        maxUploadBytes: z.number().int().positive().max(MAX_RECORDING_UPLOAD_BYTES),
        acceptedContentTypes: z.tuple([z.literal("video/mp4")]),
      })
      .strict(),
    visibility: z.array(RecordingVisibilitySchema).length(3),
  })
  .strict();
export type SaasCapabilities = z.infer<typeof SaasCapabilitiesSchema>;

export const SaasApiErrorSchema = z
  .object({
    error: z
      .object({
        code: z.enum([
          "bad_request",
          "method_not_allowed",
          "unsupported_media_type",
          "request_timeout",
          "not_authenticated",
          "not_authorized",
          "not_found",
          "payload_too_large",
          "not_configured",
          "not_implemented",
          "rate_limited",
          "internal_error",
        ]),
        message: z.string().min(1).max(240),
        requestId: IdSchema,
      })
      .strict(),
  })
  .strict();
export type SaasApiError = z.infer<typeof SaasApiErrorSchema>;
