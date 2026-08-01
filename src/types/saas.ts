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

export interface OidcAuthorizationPreparation {
  state: string;
  nonce: string;
  codeChallenge: string;
  codeChallengeMethod: "S256";
  expiresAt: string;
}

export interface OidcTransactionStatus {
  pending: boolean;
  expiresAt: string | null;
  expiresInSeconds: number;
}

export interface OidcTransactionProbe {
  s256Ready: boolean;
  stateRoundTripOk: boolean;
  nonceRetained: boolean;
  replayRejected: boolean;
  verifierKeptNative: boolean;
}
