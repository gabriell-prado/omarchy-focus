//! Desktop notifications over D-Bus. Each one replaces the previous Focus
//! notification instead of stacking up. Sent from a short-lived thread so a
//! slow or missing notification server never stalls the timer.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;

#[derive(Clone, Default)]
pub struct Notifier {
    last_id: Arc<AtomicU32>,
    disabled: bool,
}

impl Notifier {
    pub fn new() -> Notifier {
        // Tests and headless runs turn notifications off.
        let disabled = std::env::var("OMARCHY_FOCUS_NOTIFY").map(|v| v == "0").unwrap_or(false);
        Notifier { last_id: Arc::default(), disabled }
    }

    pub fn send(&self, title: String, body: String) {
        if self.disabled {
            return;
        }
        let last_id = self.last_id.clone();
        thread::spawn(move || {
            let mut n = notify_rust::Notification::new();
            n.appname("Focus").summary(&title).body(&body).icon("alarm-symbolic");
            let previous = last_id.load(Ordering::Relaxed);
            if previous != 0 {
                n.id(previous);
            }
            match n.show() {
                Ok(handle) => last_id.store(handle.id(), Ordering::Relaxed),
                Err(e) => eprintln!("omarchy-focus: notification failed: {e}"),
            }
        });
    }
}
