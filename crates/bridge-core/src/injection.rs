//! Injection ledger and browser message mapping.
//!
//! Reconciles browser-native roles (`user` vs `assistant`) with authoritative
//! multi-actor logical identities. Injected external actor turns (e.g. Orbit SA)
//! pass through ChatGPT's native user composer, but must remain attributed to their
//! true logical sender rather than being mistaken for human-typed messages.

use chrono::{DateTime, Utc};
use protocol::{CorrelationId, ParticipantId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Formats an external actor message for insertion into the ChatGPT composer.
pub fn format_external_injection(display_name: &str, content: &str) -> String {
    format!("[External participant: {display_name}]\n\n{content}")
}

/// A pending outbound injection waiting for DOM reflection in ChatGPT Web.
#[derive(Debug, Clone)]
pub struct PendingInjection {
    pub correlation_id: CorrelationId,
    pub actor_id: ParticipantId,
    pub snippet: String,
    pub created_at: DateTime<Utc>,
}

/// Ledger tracking outbound injections to resolve incoming native user messages.
#[derive(Debug, Clone, Default)]
pub struct InjectionLedger {
    inner: Arc<Mutex<HashMap<CorrelationId, PendingInjection>>>,
}

impl InjectionLedger {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Record a pending injection sent to the browser.
    pub fn record_injection(
        &self,
        correlation_id: CorrelationId,
        actor_id: ParticipantId,
        content: &str,
    ) {
        let snippet = content
            .chars()
            .take(64)
            .collect::<String>()
            .trim()
            .to_string();

        let mut lock = self.inner.lock().unwrap();
        lock.insert(
            correlation_id.clone(),
            PendingInjection {
                correlation_id,
                actor_id,
                snippet,
                created_at: Utc::now(),
            },
        );
    }

    /// Resolves an incoming native user message:
    /// - If a matching pending injection exists (by correlation ID or content snippet match),
    ///   removes it and returns `Some(actor_id)`.
    /// - If no match is found, returns `None`, indicating the message was typed manually by a human.
    pub fn resolve_user_message(
        &self,
        correlation_id: Option<&CorrelationId>,
        content: &str,
    ) -> Option<ParticipantId> {
        let mut lock = self.inner.lock().unwrap();

        // 1. Direct correlation match if provided
        if let Some(corr) = correlation_id {
            if let Some(pending) = lock.remove(corr) {
                return Some(pending.actor_id);
            }
        }

        // 2. Content snippet / prefix match for composer-reflected text
        let snippet = content.chars().take(64).collect::<String>();
        let mut matched_key = None;
        for (corr, pending) in lock.iter() {
            if snippet.contains(&pending.snippet) || pending.snippet.contains(snippet.trim()) {
                matched_key = Some(corr.clone());
                break;
            }
        }

        if let Some(key) = matched_key {
            let pending = lock.remove(&key).unwrap();
            return Some(pending.actor_id);
        }

        None
    }

    /// Returns the number of currently pending injections.
    pub fn pending_count(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
}
