import { z } from "zod";

const IdSchema = z.string().uuid();
const IsoDateTimeSchema = z.string().datetime({ offset: true });

export const AccountStatusSchema = z.enum(["active", "disabled", "deletion_pending"]);
export const BillingProviderSchema = z.enum(["stripe", "paypal", "phonepe"]);
export const SubscriptionStatusSchema = z.enum([
  "pending",
  "active",
  "grace_period",
  "past_due",
  "suspended",
  "cancelled",
  "expired",
]);

const FeatureKeySchema = z
  .string()
  .min(3)
  .max(64)
  .regex(/^[a-z][a-z0-9_]{2,63}$/);

export const SupabaseAccessRowSchema = z
  .object({
    account_status: AccountStatusSchema,
    subscription_provider: BillingProviderSchema.nullable(),
    subscription_status: SubscriptionStatusSchema.nullable(),
    current_period_end: IsoDateTimeSchema.nullable(),
    entitlements: z.array(FeatureKeySchema).max(64),
    full_access: z.boolean(),
  })
  .strict();
export type SupabaseAccessRow = z.infer<typeof SupabaseAccessRowSchema>;

export const MeAccessResponseSchema = z
  .object({
    userId: IdSchema,
    authenticated: z.literal(true),
    accountStatus: AccountStatusSchema,
    subscription: z
      .object({
        provider: BillingProviderSchema,
        status: SubscriptionStatusSchema,
        currentPeriodEnd: IsoDateTimeSchema.nullable(),
      })
      .strict()
      .nullable(),
    entitlements: z.array(FeatureKeySchema).max(64),
    fullAccess: z.boolean(),
  })
  .strict();
export type MeAccessResponse = z.infer<typeof MeAccessResponseSchema>;
