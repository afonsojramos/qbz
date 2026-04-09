//! Interactive Qobuz login via system browser OAuth.
//!
//! Same flow as the desktop app's v2_start_system_browser_oauth:
//! 1. Extract bundle tokens (app_id, secrets, private_key)
//! 2. Start local HTTP server on random port
//! 3. Open system browser to Qobuz OAuth URL
//! 4. Capture authorization code from redirect
//! 5. Exchange code for session token
//! 6. Save token to system keyring

use qbz_qobuz::QobuzClient;

const OAUTH_TIMEOUT_SECS: u64 = 120;

pub async fn interactive_login() -> Result<(), String> {
    println!("Initializing Qobuz client...");

    // Create client and extract bundle tokens
    let client = QobuzClient::new().map_err(|e| format!("Client error: {}", e))?;
    client.init().await.map_err(|e| format!("Bundle extraction failed: {}", e))?;

    let app_id = client.app_id().await.map_err(|e| format!("No app_id: {}", e))?;

    // Bind to random available port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Failed to bind listener: {}", e))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();

    let redirect_url = format!("http://localhost:{}", port);
    let oauth_url = format!(
        "https://www.qobuz.com/signin/oauth?ext_app_id={}&redirect_url={}",
        app_id,
        urlencoding::encode(&redirect_url),
    );

    // Channel for the auth code
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

    // Local HTTP handler
    let handler = axum::Router::new().route(
        "/",
        axum::routing::get(move |axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>| {
            let tx = tx.clone();
            async move {
                if let Some(code) = params.get("code_autorisation").or_else(|| params.get("code")) {
                    let _ = tx.send(code.clone()).await;
                    axum::response::Html(
                        "<html><body style=\"font-family:system-ui;text-align:center;padding:60px\">\
                         <h2>Login successful!</h2>\
                         <p>You can close this tab and return to the terminal.</p>\
                         </body></html>"
                    )
                } else {
                    axum::response::Html(
                        "<html><body style=\"font-family:system-ui;text-align:center;padding:60px\">\
                         <h2>Login failed</h2>\
                         <p>No authorization code received.</p>\
                         </body></html>"
                    )
                }
            }
        }),
    );

    let server_handle = tokio::spawn(async move {
        axum::serve(listener, handler).await.ok();
    });

    // Open system browser
    println!("\nOpening browser for Qobuz login...");
    println!("If the browser doesn't open, visit:\n  {}\n", oauth_url);

    if let Err(e) = open::that(&oauth_url) {
        eprintln!("Failed to open browser: {}", e);
        eprintln!("Please open this URL manually:\n  {}\n", oauth_url);
    }

    println!("Waiting for login ({}s timeout)...", OAUTH_TIMEOUT_SECS);

    // Wait for code
    let code = tokio::time::timeout(
        std::time::Duration::from_secs(OAUTH_TIMEOUT_SECS),
        rx.recv(),
    )
    .await;

    server_handle.abort();

    let code = match code {
        Ok(Some(c)) => c,
        Ok(None) => return Err("Login cancelled".to_string()),
        Err(_) => return Err(format!("Login timed out after {}s", OAUTH_TIMEOUT_SECS)),
    };

    println!("Authorization code received. Exchanging for session...");

    // Exchange code for session via QobuzClient
    let session = client
        .login_with_oauth_code(&code)
        .await
        .map_err(|e| format!("OAuth exchange failed: {}", e))?;

    println!(
        "\nLogged in as: {} (user_id: {})",
        session.display_name, session.user_id
    );
    println!("Subscription: {}", session.subscription_label);

    // Save token to keyring
    let token = session.user_auth_token.clone();
    save_token_to_keyring(&token)?;

    println!("\nCredentials saved. The daemon will auto-login on next start.");
    Ok(())
}

/// Save OAuth token to system keyring (same service/key as desktop app).
fn save_token_to_keyring(token: &str) -> Result<(), String> {
    const SERVICE: &str = "qbz-player";
    const KEY: &str = "qobuz-oauth-token";

    let entry = keyring::Entry::new(SERVICE, KEY)
        .map_err(|e| format!("Keyring error: {}", e))?;
    entry
        .set_password(token)
        .map_err(|e| format!("Failed to save to keyring: {}", e))?;

    println!("Token saved to system keyring (service: {}, key: {})", SERVICE, KEY);
    Ok(())
}
