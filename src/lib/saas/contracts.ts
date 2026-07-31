import { z } from "zod";

export const SAAS_MVP_LIMITS = {
  titleCharacters: 160,
  uploadBytes: 20 * 1024 * 1024 * 1024,
  durationMilliseconds: 8 * 60 * 60 * 1_000,
  multipartParts: 10_000,
} as const;

const opaqueProtocolValue = (field: string, minimum: number, maximum: number) =>
  z
    .string()
    .min(minimum, `${field} is too short`)
    .max(maximum, `${field} is too long`)
    .regex(/^[A-Za-z0-9._~-]+$/, `${field} contains unsupported characters`);

export const cloudVisibilitySchema = z.enum(["private", "unlisted", "public"]);
export type CloudVisibility = z.infer<typeof cloudVisibilitySchema>;

export const oidcAuthorizationCallbackSchema = z
  .object({
    code: opaqueProtocolValue("Authorization code", 8, 4_096),
    state: opaqueProtocolValue("Authorization state", 32, 512),
  })
  .strict();
export type OidcAuthorizationCallback = z.infer<typeof oidcAuthorizationCallbackSchema>;

export const oidcPkceTransactionSchema = z
  .object({
    state: opaqueProtocolValue("Authorization state", 32, 512),
    nonce: opaqueProtocolValue("OIDC nonce", 32, 512),
    codeVerifier: opaqueProtocolValue("PKCE verifier", 43, 128),
    redirectUri: z.string().url().max(2_048),
    createdAt: z.string().datetime({ offset: true }),
  })
  .strict();
export type OidcPkceTransaction = z.infer<typeof oidcPkceTransactionSchema>;

export const uploadInitiationSchema = z
  .object({
    recordingId: z.string().uuid(),
    title: z.string().trim().min(1).max(SAAS_MVP_LIMITS.titleCharacters),
    contentType: z.literal("video/mp4"),
    fileSizeBytes: z.number().int().positive().max(SAAS_MVP_LIMITS.uploadBytes),
    durationMs: z.number().int().nonnegative().max(SAAS_MVP_LIMITS.durationMilliseconds),
    sha256: z.string().regex(/^[a-f0-9]{64}$/, "SHA-256 must be lowercase hexadecimal"),
    visibility: cloudVisibilitySchema.default("private"),
  })
  .strict();
export type UploadInitiation = z.infer<typeof uploadInitiationSchema>;

export const uploadPartSchema = z
  .object({
    partNumber: z.number().int().min(1).max(SAAS_MVP_LIMITS.multipartParts),
    uploadUrl: z.string().url().max(4_096),
    expiresAt: z.string().datetime({ offset: true }),
  })
  .strict();
export type UploadPart = z.infer<typeof uploadPartSchema>;

export const uploadSessionSchema = z
  .object({
    uploadId: z.string().uuid(),
    recordingId: z.string().uuid(),
    objectKey: z.string().min(1).max(1_024),
    partSizeBytes: z.number().int().positive(),
    parts: z.array(uploadPartSchema).min(1).max(SAAS_MVP_LIMITS.multipartParts),
    expiresAt: z.string().datetime({ offset: true }),
  })
  .strict()
  .superRefine((session, context) => {
    const partNumbers = new Set<number>();
    for (const part of session.parts) {
      if (partNumbers.has(part.partNumber)) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          path: ["parts"],
          message: "Multipart part numbers must be unique",
        });
        return;
      }
      partNumbers.add(part.partNumber);
    }
  });
export type UploadSession = z.infer<typeof uploadSessionSchema>;

export const completedUploadPartSchema = z
  .object({
    partNumber: z.number().int().min(1).max(SAAS_MVP_LIMITS.multipartParts),
    etag: z.string().trim().min(1).max(512),
  })
  .strict();

export const completeUploadSchema = z
  .object({
    uploadId: z.string().uuid(),
    recordingId: z.string().uuid(),
    parts: z
      .array(completedUploadPartSchema)
      .min(1)
      .max(SAAS_MVP_LIMITS.multipartParts),
  })
  .strict()
  .superRefine((completion, context) => {
    const partNumbers = completion.parts.map((part) => part.partNumber);
    const unique = new Set(partNumbers);
    const ascending = partNumbers.every(
      (partNumber, index) => index === 0 || partNumber > partNumbers[index - 1],
    );
    if (unique.size !== partNumbers.length || !ascending) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["parts"],
        message: "Completed parts must be unique and strictly ascending",
      });
    }
  });
export type CompleteUpload = z.infer<typeof completeUploadSchema>;

export const cloudRecordingSchema = z
  .object({
    id: z.string().uuid(),
    ownerId: z.string().uuid(),
    title: z.string().min(1).max(SAAS_MVP_LIMITS.titleCharacters),
    visibility: cloudVisibilitySchema,
    durationMs: z.number().int().nonnegative(),
    fileSizeBytes: z.number().int().nonnegative(),
    status: z.enum(["uploading", "ready", "failed", "deleted"]),
    createdAt: z.string().datetime({ offset: true }),
    thumbnailUrl: z.string().url().max(4_096).nullable(),
    playbackUrl: z.string().url().max(4_096).nullable(),
  })
  .strict();
export type CloudRecording = z.infer<typeof cloudRecordingSchema>;
