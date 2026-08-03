export {};

declare global {
  interface JsonWebKey {
    /** RFC 7517 key identifier used to select a signing key from a JWKS document. */
    kid?: string;
    /** RFC 7517 intended algorithm. */
    alg?: string;
    /** RFC 7517 intended public-key use. */
    use?: string;
  }
}
