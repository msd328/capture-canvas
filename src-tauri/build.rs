fn main() {
    for name in [
        "RECORDER_OIDC_AUTHORIZATION_ENDPOINT",
        "RECORDER_OIDC_CLIENT_ID",
        "RECORDER_OIDC_REDIRECT_URI",
        "RECORDER_OIDC_SCOPES",
        "RECORDER_OIDC_TOKEN_ENDPOINT",
        "RECORDER_OIDC_ISSUER",
        "RECORDER_OIDC_AUDIENCE",
        "RECORDER_OIDC_JWKS_URI",
        "RECORDER_SAAS_ACCESS_ENDPOINT",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }

    tauri_build::build();
}
