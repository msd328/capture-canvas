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
