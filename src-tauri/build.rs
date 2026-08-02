fn main() {
    for name in [
        "RECORDER_OIDC_AUTHORIZATION_ENDPOINT",
        "RECORDER_OIDC_CLIENT_ID",
        "RECORDER_OIDC_REDIRECT_URI",
        "RECORDER_OIDC_SCOPES",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }

    tauri_build::build();
}
