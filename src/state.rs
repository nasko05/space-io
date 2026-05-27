use crate::space::rate_limit::RateLimiter;
use crate::space::session::SessionStore;
use crate::space::Space;

/// Runtime configuration sourced from CLI flags + env. Shared by every
/// request handler so we don't sprinkle `std::env::var` lookups around.
#[derive(Clone, Debug)]
pub struct AppConfig {
    /// Mark the session cookie `Secure`. Defaults to `true`; opt out with
    /// `HEARTH_INSECURE_COOKIES=1` for local plain-HTTP development.
    pub cookie_secure: bool,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let insecure = std::env::var("HEARTH_INSECURE_COOKIES")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        Self {
            cookie_secure: !insecure,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub space: Space,
    pub sessions: SessionStore,
    pub unlock_limiter: RateLimiter,
    pub config: AppConfig,
}
