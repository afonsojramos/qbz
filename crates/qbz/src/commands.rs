//! Typed command vocabulary for the Slint POC.
//!
//! Slint UI callbacks carry no payload and no behavior. The Rust layer
//! maps each callback to one of these typed commands. In M2 the command
//! handler is wired to `AppRuntime`; for now it logs, which proves the
//! `Slint callback -> typed command -> Rust` path the POC ADR (section 8)
//! requires.

/// Commands the login screen can emit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppCommand {
    /// Begin OAuth sign-in. Per the 2026-05-18 decision the POC uses the
    /// external system-browser flow only (no in-app webview). The former
    /// separate "use system browser" link (and its command) was removed
    /// 2026-07-02 — the one button IS the system-browser flow.
    SignInViaBrowser,
    /// Start an offline-only session with no Qobuz access.
    StartOffline,
    /// Open the Qobuz Terms of Service in the system browser.
    OpenTermsOfService,
}

impl AppCommand {
    /// Stable identifier, used for logging and later instrumentation.
    pub fn id(&self) -> &'static str {
        match self {
            Self::SignInViaBrowser => "sign_in_via_browser",
            Self::StartOffline => "start_offline",
            Self::OpenTermsOfService => "open_terms_of_service",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_ids_are_distinct_and_stable() {
        let all = [
            AppCommand::SignInViaBrowser,
            AppCommand::StartOffline,
            AppCommand::OpenTermsOfService,
        ];
        let ids: Vec<&str> = all.iter().map(AppCommand::id).collect();
        assert_eq!(ids.len(), 3);
        let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), 3, "command ids must be unique");
    }
}
