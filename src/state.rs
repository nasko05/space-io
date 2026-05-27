use crate::space::session::SessionStore;
use crate::space::Space;

#[derive(Clone)]
pub struct AppState {
    pub space: Space,
    pub sessions: SessionStore,
}
