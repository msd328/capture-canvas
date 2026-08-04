export interface SecureAuthStatus {
  supported: boolean;
  signedIn: boolean;
  storage: "windows-credential-manager" | "unsupported";
}

export interface SecureAuthProbe {
  supported: boolean;
  roundTripOk: boolean;
  storage: "windows-credential-manager" | "unsupported";
}

export interface OidcClientStatus {
  configured: boolean;
  authorizationEndpointHttps: boolean;
  callbackMode: "custom-scheme" | "loopback" | null;
  scopeCount: number;
  tokenExchangeConfigured: boolean;
  tokenEndpointHttps: boolean;
  issuerHttps: boolean;
  audienceConfigured: boolean;
  jwksUriHttps: boolean;
}

export interface OidcExchangeContractStatus {
  configured: boolean;
  publicClient: boolean;
  authorizationCodeFormSupported: boolean;
  refreshTokenFormSupported: boolean;
  strictResponseParser: boolean;
  boundedHttpsTransportSupported: boolean;
  redirectsDisabled: boolean;
  runtimeProxyDisabled: boolean;
  networkExchangeEnabled: boolean;
  identityValidationEnabled: boolean;
  maxRequestBytes: number;
  maxResponseBytes: number;
  connectTimeoutSeconds: number;
  readTimeoutSeconds: number;
  totalTimeoutSeconds: number;
}

export interface OidcExchangeProbe {
  authorizationCodeFormOk: boolean;
  refreshTokenFormOk: boolean;
  tokenResponseOk: boolean;
  duplicateFieldRejected: boolean;
  unknownFieldRejected: boolean;
  oversizedResponseRejected: boolean;
  transportClientOk: boolean;
  strictJsonHeadersOk: boolean;
  redirectResponseRejected: boolean;
  oversizedDeclaredResponseRejected: boolean;
  secretsKeptNative: boolean;
}

export interface OidcSignInLaunch {
  launched: boolean;
  callbackMode: "loopback";
  expiresAt: string;
}

export type OidcCallbackStage =
  | "idle"
  | "waiting"
  | "codeReceived"
  | "grantTaken"
  | "providerError"
  | "timedOut"
  | "cancelled"
  | "failed";

export interface OidcCallbackStatus {
  stage: OidcCallbackStage;
  pending: boolean;
  codeReceived: boolean;
  providerError: boolean;
  expiresAt: string | null;
}

export interface NativeOidcSessionStatus {
  active: boolean;
  expiresAt: string | null;
  accessTokenNativeOnly: boolean;
  refreshTokenPersisted: boolean;
  reconciliationComplete: boolean;
  refreshCredentialPresent: boolean;
  refreshCredentialInvalid: boolean;
  legacyRefreshCredentialPresent: boolean;
  restorationRequired: boolean;
  restorationAttempted: boolean;
  restorationFailed: boolean;
  automaticRestorationEnabled: boolean;
}

export interface OidcTransactionProbe {
  s256Ready: boolean;
  stateRoundTripOk: boolean;
  nonceRetained: boolean;
  replayRejected: boolean;
  verifierKeptNative: boolean;
}

export type AccountStatus = "active" | "disabled" | "deletion_pending";
export type BillingProvider = "stripe" | "paypal" | "phonepe";
export type SubscriptionStatus =
  | "pending"
  | "active"
  | "grace_period"
  | "past_due"
  | "suspended"
  | "cancelled"
  | "expired";

export interface NativeSubscriptionAccess {
  provider: BillingProvider;
  status: SubscriptionStatus;
  currentPeriodEnd: string | null;
}

export interface NativeAccountAccessStatus {
  configured: boolean;
  endpointHttps: boolean;
  loopbackDevelopment: boolean;
  checked: boolean;
  authenticated: boolean;
  accountStatus: AccountStatus | null;
  subscription: NativeSubscriptionAccess | null;
  entitlementCount: number;
  fullAccess: boolean;
  checkedAt: string | null;
  validUntil: string | null;
  accessTokenNativeOnly: boolean;
}
